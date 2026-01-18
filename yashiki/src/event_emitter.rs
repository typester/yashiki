use crate::core::{Display, State, Window};
use std::sync::mpsc as std_mpsc;
use yashiki_ipc::{OutputInfo, StateEvent, WindowInfo};

/// Event emitter for sending state change events from the main thread to the tokio thread.
/// Uses std::sync::mpsc for thread-safe communication.
pub struct EventEmitter {
    tx: std_mpsc::Sender<StateEvent>,
}

impl EventEmitter {
    pub fn new(tx: std_mpsc::Sender<StateEvent>) -> Self {
        Self { tx }
    }

    /// Send an event to subscribers
    fn emit(&self, event: StateEvent) {
        if let Err(e) = self.tx.send(event) {
            tracing::debug!("Failed to emit event (no receivers?): {}", e);
        }
    }

    /// Emit a window created event
    pub fn emit_window_created(&self, window: &Window, focused: Option<u32>) {
        self.emit(StateEvent::WindowCreated {
            window: window_to_info(window, focused),
        });
    }

    /// Emit a window destroyed event
    pub fn emit_window_destroyed(&self, window_id: u32) {
        self.emit(StateEvent::WindowDestroyed { window_id });
    }

    /// Emit a window updated event
    pub fn emit_window_updated(&self, window: &Window, focused: Option<u32>) {
        self.emit(StateEvent::WindowUpdated {
            window: window_to_info(window, focused),
        });
    }

    /// Emit a window focused event
    pub fn emit_window_focused(&self, window_id: Option<u32>) {
        self.emit(StateEvent::WindowFocused { window_id });
    }

    /// Emit a display focused event
    pub fn emit_display_focused(&self, display_id: u32) {
        self.emit(StateEvent::DisplayFocused { display_id });
    }

    /// Emit a display added event
    pub fn emit_display_added(&self, display: &Display, focused_display: u32) {
        self.emit(StateEvent::DisplayAdded {
            display: display_to_info(display, focused_display),
        });
    }

    /// Emit a display removed event
    pub fn emit_display_removed(&self, display_id: u32) {
        self.emit(StateEvent::DisplayRemoved { display_id });
    }

    /// Emit a display updated event
    pub fn emit_display_updated(&self, display: &Display, focused_display: u32) {
        self.emit(StateEvent::DisplayUpdated {
            display: display_to_info(display, focused_display),
        });
    }

    /// Emit a tags changed event
    pub fn emit_tags_changed(&self, display_id: u32, visible_tags: u32, previous_tags: u32) {
        self.emit(StateEvent::TagsChanged {
            display_id,
            visible_tags,
            previous_tags,
        });
    }

    /// Emit a layout changed event
    pub fn emit_layout_changed(&self, display_id: u32, layout: &str) {
        self.emit(StateEvent::LayoutChanged {
            display_id,
            layout: layout.to_string(),
        });
    }

    /// Create and emit a snapshot event from current state
    pub fn emit_snapshot(&self, state: &State) {
        self.emit(create_snapshot(state));
    }
}

/// Create a snapshot event from current state
pub fn create_snapshot(state: &State) -> StateEvent {
    let windows: Vec<WindowInfo> = state
        .windows
        .values()
        .map(|w| window_to_info(w, state.focused))
        .collect();

    let displays: Vec<OutputInfo> = state
        .displays
        .values()
        .map(|d| display_to_info(d, state.focused_display))
        .collect();

    StateEvent::Snapshot {
        windows,
        displays,
        focused_window_id: state.focused,
        focused_display_id: state.focused_display,
        default_layout: state.default_layout.clone(),
    }
}

/// Convert a Window to WindowInfo
pub fn window_to_info(window: &Window, focused: Option<u32>) -> WindowInfo {
    WindowInfo {
        id: window.id,
        pid: window.pid,
        title: window.title.clone(),
        app_name: window.app_name.clone(),
        app_id: window.app_id.clone(),
        tags: window.tags.mask(),
        x: window.frame.x,
        y: window.frame.y,
        width: window.frame.width,
        height: window.frame.height,
        is_focused: focused == Some(window.id),
        is_floating: window.is_floating,
        is_fullscreen: window.is_fullscreen,
    }
}

/// Convert a Display to OutputInfo
pub fn display_to_info(display: &Display, focused_display: u32) -> OutputInfo {
    OutputInfo {
        id: display.id,
        name: display.name.clone(),
        x: display.frame.x,
        y: display.frame.y,
        width: display.frame.width,
        height: display.frame.height,
        is_main: display.is_main,
        visible_tags: display.visible_tags.mask(),
        is_focused: focused_display == display.id,
    }
}
