use crate::macos::{DisplayInfo, WindowInfo};

pub struct FocusedWindowInfo {
    pub window_id: u32,
}

/// Trait for querying window and display information from the system.
/// This abstraction allows mocking in tests.
pub trait WindowSystem {
    fn get_on_screen_windows(&self) -> Vec<WindowInfo>;
    fn get_all_displays(&self) -> Vec<DisplayInfo>;
    fn get_focused_window(&self) -> Option<FocusedWindowInfo>;
}

/// macOS implementation of WindowSystem
pub struct MacOSWindowSystem;

impl WindowSystem for MacOSWindowSystem {
    fn get_on_screen_windows(&self) -> Vec<WindowInfo> {
        crate::macos::get_on_screen_windows()
    }

    fn get_all_displays(&self) -> Vec<DisplayInfo> {
        crate::macos::get_all_displays()
    }

    fn get_focused_window(&self) -> Option<FocusedWindowInfo> {
        match crate::macos::get_focused_window() {
            Ok(ax_element) => ax_element
                .window_id()
                .map(|id| FocusedWindowInfo { window_id: id }),
            Err(_) => None,
        }
    }
}

impl Default for MacOSWindowSystem {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use crate::macos::{Bounds, DisplayId};

    #[derive(Default)]
    pub struct MockWindowSystem {
        pub windows: Vec<WindowInfo>,
        pub displays: Vec<DisplayInfo>,
        pub focused_window_id: Option<u32>,
    }

    impl MockWindowSystem {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_windows(mut self, windows: Vec<WindowInfo>) -> Self {
            self.windows = windows;
            self
        }

        pub fn with_displays(mut self, displays: Vec<DisplayInfo>) -> Self {
            self.displays = displays;
            self
        }

        pub fn with_focused(mut self, window_id: Option<u32>) -> Self {
            self.focused_window_id = window_id;
            self
        }

        pub fn set_focused(&mut self, window_id: Option<u32>) {
            self.focused_window_id = window_id;
        }

        pub fn add_window(&mut self, info: WindowInfo) {
            self.windows.push(info);
        }

        pub fn remove_window(&mut self, window_id: u32) {
            self.windows.retain(|w| w.window_id != window_id);
        }
    }

    impl WindowSystem for MockWindowSystem {
        fn get_on_screen_windows(&self) -> Vec<WindowInfo> {
            self.windows.clone()
        }

        fn get_all_displays(&self) -> Vec<DisplayInfo> {
            self.displays.clone()
        }

        fn get_focused_window(&self) -> Option<FocusedWindowInfo> {
            self.focused_window_id
                .map(|id| FocusedWindowInfo { window_id: id })
        }
    }

    pub fn create_test_display(
        id: DisplayId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> DisplayInfo {
        DisplayInfo {
            id,
            frame: Bounds {
                x,
                y,
                width,
                height,
            },
            is_main: id == 1,
        }
    }

    pub fn create_test_window(
        window_id: u32,
        pid: i32,
        owner_name: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> WindowInfo {
        WindowInfo {
            pid,
            window_id,
            name: Some(format!("{} Window", owner_name)),
            owner_name: owner_name.to_string(),
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
            layer: 0,
        }
    }
}
