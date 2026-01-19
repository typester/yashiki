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
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Rect, Tag};

    fn create_test_window(id: u32, pid: i32, app_name: &str) -> Window {
        Window {
            id,
            pid,
            display_id: 1,
            tags: Tag::new(1),
            title: format!("{} Window", app_name),
            app_name: app_name.to_string(),
            app_id: Some(format!("com.test.{}", app_name.to_lowercase())),
            ax_id: None,
            subrole: None,
            window_level: 0,
            close_button: yashiki_ipc::ButtonInfo::default(),
            fullscreen_button: yashiki_ipc::ButtonInfo::default(),
            minimize_button: yashiki_ipc::ButtonInfo::default(),
            zoom_button: yashiki_ipc::ButtonInfo::default(),
            frame: Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            saved_frame: None,
            is_floating: false,
            is_fullscreen: false,
        }
    }

    fn create_test_display(id: u32, name: &str, is_main: bool) -> Display {
        Display::new(
            id,
            name.to_string(),
            Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            is_main,
        )
    }

    #[test]
    fn test_create_snapshot() {
        let mut state = State::new();

        // Add displays
        state
            .displays
            .insert(1, create_test_display(1, "Display 1", true));
        state
            .displays
            .insert(2, create_test_display(2, "Display 2", false));

        // Add windows
        state
            .windows
            .insert(100, create_test_window(100, 1000, "Safari"));
        state
            .windows
            .insert(101, create_test_window(101, 1001, "Terminal"));

        // Set focus
        state.focused = Some(100);
        state.focused_display = 1;
        state.default_layout = "tatami".to_string();

        // Create snapshot
        let snapshot = create_snapshot(&state);

        // Verify snapshot
        match snapshot {
            StateEvent::Snapshot {
                windows,
                displays,
                focused_window_id,
                focused_display_id,
                default_layout,
            } => {
                assert_eq!(windows.len(), 2);
                assert_eq!(displays.len(), 2);
                assert_eq!(focused_window_id, Some(100));
                assert_eq!(focused_display_id, 1);
                assert_eq!(default_layout, "tatami");

                // Check that focused window has is_focused = true
                let safari = windows.iter().find(|w| w.id == 100).unwrap();
                assert!(safari.is_focused);

                // Check that non-focused window has is_focused = false
                let terminal = windows.iter().find(|w| w.id == 101).unwrap();
                assert!(!terminal.is_focused);

                // Check that focused display has is_focused = true
                let display1 = displays.iter().find(|d| d.id == 1).unwrap();
                assert!(display1.is_focused);

                // Check that non-focused display has is_focused = false
                let display2 = displays.iter().find(|d| d.id == 2).unwrap();
                assert!(!display2.is_focused);
            }
            _ => panic!("Expected Snapshot event"),
        }
    }

    #[test]
    fn test_window_to_info_focused() {
        let window = create_test_window(100, 1000, "Safari");

        // When window is focused
        let info = window_to_info(&window, Some(100));
        assert!(info.is_focused);
        assert_eq!(info.id, 100);
        assert_eq!(info.pid, 1000);
        assert_eq!(info.app_name, "Safari");

        // When different window is focused
        let info = window_to_info(&window, Some(999));
        assert!(!info.is_focused);

        // When no window is focused
        let info = window_to_info(&window, None);
        assert!(!info.is_focused);
    }

    #[test]
    fn test_display_to_info_focused() {
        let display = create_test_display(1, "Main Display", true);

        // When display is focused
        let info = display_to_info(&display, 1);
        assert!(info.is_focused);
        assert_eq!(info.id, 1);
        assert_eq!(info.name, "Main Display");
        assert!(info.is_main);

        // When different display is focused
        let info = display_to_info(&display, 2);
        assert!(!info.is_focused);
    }
}
