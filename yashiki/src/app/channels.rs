use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;

use core_foundation_sys::runloop::{
    CFRunLoopGetMain, CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use tokio::sync::mpsc;

use crate::event::Event;
use crate::ipc::{EventBroadcaster, EventServer, IpcServer};
use crate::macos::DisplayReconfigEvent;
use yashiki_ipc::{Command, Response, StateEvent};

pub type IpcCommandWithResponse = (Command, mpsc::Sender<Response>);

pub type SnapshotRequest = tokio::sync::oneshot::Sender<StateEvent>;

pub struct IpcRelay {
    pub cmd_tx: std_mpsc::Sender<IpcCommandWithResponse>,
    pub server_tx: mpsc::Sender<IpcCommandWithResponse>,
    pub server_rx: mpsc::Receiver<IpcCommandWithResponse>,
    pub runloop_source: Arc<AtomicPtr<std::ffi::c_void>>,
}

pub struct EventStreaming {
    pub broadcaster: EventBroadcaster,
    pub event_server_rx: tokio::sync::broadcast::Receiver<StateEvent>,
    pub state_event_rx: std_mpsc::Receiver<StateEvent>,
}

pub struct SnapshotRelay {
    pub request_tx: mpsc::Sender<SnapshotRequest>,
    pub request_rx: mpsc::Receiver<SnapshotRequest>,
    pub main_tx: std_mpsc::Sender<SnapshotRequest>,
}

pub struct TokioChannels {
    pub ipc: IpcRelay,
    pub events: EventStreaming,
    pub snapshots: SnapshotRelay,
    pub event_rx: mpsc::Receiver<Event>,
}

pub struct MainChannels {
    pub ipc_cmd_rx: std_mpsc::Receiver<IpcCommandWithResponse>,
    pub observer_event_tx: std_mpsc::Sender<Event>,
    pub observer_event_rx: std_mpsc::Receiver<Event>,
    pub event_tx: mpsc::Sender<Event>,
    pub state_event_tx: std_mpsc::Sender<StateEvent>,
    pub snapshot_request_rx: std_mpsc::Receiver<SnapshotRequest>,
    pub display_reconfig_tx: std_mpsc::Sender<DisplayReconfigEvent>,
    pub display_reconfig_rx: std_mpsc::Receiver<DisplayReconfigEvent>,
    pub ipc_source: Arc<AtomicPtr<std::ffi::c_void>>,
}

pub fn create_channels() -> (TokioChannels, MainChannels) {
    // Channel: IPC commands (tokio -> main thread)
    let (ipc_cmd_tx, ipc_cmd_rx) = std_mpsc::channel::<IpcCommandWithResponse>();

    // Channel: observer -> main thread
    let (observer_event_tx, observer_event_rx) = std_mpsc::channel::<Event>();

    // Channel: main thread -> tokio
    let (event_tx, event_rx) = mpsc::channel::<Event>(256);

    // Channel for IPC server (tokio internal)
    let (ipc_server_tx, ipc_server_rx) = mpsc::channel::<IpcCommandWithResponse>(256);

    // Event broadcasting for state streaming
    let event_broadcaster = EventBroadcaster::new(256);
    let event_server_rx = event_broadcaster.subscribe();

    // Channel: state events (main thread -> tokio)
    let (state_event_tx, state_event_rx) = std_mpsc::channel::<StateEvent>();

    // Channel: snapshot requests (tokio -> main thread)
    let (snapshot_request_tx, snapshot_request_rx) = mpsc::channel::<SnapshotRequest>(16);
    let (snapshot_request_main_tx, snapshot_request_main_rx) =
        std_mpsc::channel::<SnapshotRequest>();

    // Channel: display reconfiguration events (callback -> main thread)
    let (display_reconfig_tx, display_reconfig_rx) = std_mpsc::channel::<DisplayReconfigEvent>();

    // Note: register_display_callback is called in run_main_loop after source creation

    // Shared pointer to CFRunLoopSource (will be set by main thread)
    let ipc_source = Arc::new(AtomicPtr::new(ptr::null_mut()));
    let ipc_source_clone = Arc::clone(&ipc_source);

    let tokio_channels = TokioChannels {
        ipc: IpcRelay {
            cmd_tx: ipc_cmd_tx,
            server_tx: ipc_server_tx,
            server_rx: ipc_server_rx,
            runloop_source: ipc_source_clone,
        },
        events: EventStreaming {
            broadcaster: event_broadcaster,
            event_server_rx,
            state_event_rx,
        },
        snapshots: SnapshotRelay {
            request_tx: snapshot_request_tx,
            request_rx: snapshot_request_rx,
            main_tx: snapshot_request_main_tx,
        },
        event_rx,
    };

    let main_channels = MainChannels {
        ipc_cmd_rx,
        observer_event_tx,
        observer_event_rx,
        event_tx,
        state_event_tx,
        snapshot_request_rx: snapshot_request_main_rx,
        display_reconfig_tx,
        display_reconfig_rx,
        ipc_source,
    };

    (tokio_channels, main_channels)
}

pub async fn run_async(channels: TokioChannels) {
    // Destructure for partial moves
    let TokioChannels {
        ipc,
        events,
        snapshots,
        mut event_rx,
    } = channels;
    let IpcRelay {
        cmd_tx: ipc_cmd_tx,
        server_tx: ipc_server_tx,
        server_rx: mut ipc_rx,
        runloop_source: ipc_source,
    } = ipc;
    let EventStreaming {
        broadcaster: event_broadcaster,
        event_server_rx,
        state_event_rx,
    } = events;
    let SnapshotRelay {
        request_tx: snapshot_request_tx,
        request_rx: mut snapshot_request_rx,
        main_tx: snapshot_request_main_tx,
    } = snapshots;

    tracing::info!("Tokio runtime started");

    // Start IPC server
    let ipc_server = IpcServer::new(ipc_server_tx);
    tokio::spawn(async move {
        if let Err(e) = ipc_server.run().await {
            tracing::error!("IPC server error: {}", e);
        }
    });

    // Start Event server
    let event_server = EventServer::new(event_server_rx, snapshot_request_tx);
    tokio::spawn(async move {
        if let Err(e) = event_server.run().await {
            tracing::error!("Event server error: {}", e);
        }
    });

    // Spawn task to forward state events from main thread to broadcast channel
    let broadcaster_clone = event_broadcaster.clone();
    std::thread::spawn(move || {
        while let Ok(event) = state_event_rx.recv() {
            broadcaster_clone.send(event);
        }
    });

    loop {
        tokio::select! {
            Some((cmd, resp_tx)) = ipc_rx.recv() => {
                // Forward IPC commands to main thread
                if ipc_cmd_tx.send((cmd, resp_tx)).is_err() {
                    tracing::error!("Failed to forward IPC command to main thread");
                    break;
                }
                // Wake up the main thread's RunLoop immediately
                let source = ipc_source.load(Ordering::Acquire);
                if !source.is_null() {
                    unsafe {
                        CFRunLoopSourceSignal(source as CFRunLoopSourceRef);
                        CFRunLoopWakeUp(CFRunLoopGetMain());
                    }
                }
            }
            Some(event) = event_rx.recv() => {
                tracing::debug!("Received event: {:?}", event);
            }
            Some(snapshot_req) = snapshot_request_rx.recv() => {
                // Forward snapshot request to main thread
                if snapshot_request_main_tx.send(snapshot_req).is_err() {
                    tracing::error!("Failed to forward snapshot request to main thread");
                }
            }
            else => break,
        }
    }

    tracing::info!("Tokio runtime exiting");
}
