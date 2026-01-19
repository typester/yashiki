use crate::core::{Rect, WindowMove};
use crate::macos::{activate_application, AXUIElement, DisplayId, DisplayInfo, WindowInfo};
use core_graphics::geometry::{CGPoint, CGSize};
use yashiki_ipc::{ButtonInfo, ExtendedWindowAttributes, WindowGeometry};

pub struct FocusedWindowInfo {
    pub window_id: u32,
}

/// Trait for querying window and display information from the system.
/// This abstraction allows mocking in tests.
pub trait WindowSystem {
    fn get_on_screen_windows(&self) -> Vec<WindowInfo>;
    fn get_all_displays(&self) -> Vec<DisplayInfo>;
    fn get_focused_window(&self) -> Option<FocusedWindowInfo>;
    /// Get extended window attributes including window_level and button info.
    fn get_extended_attributes(
        &self,
        window_id: u32,
        pid: i32,
        layer: i32,
    ) -> ExtendedWindowAttributes;
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

    fn get_extended_attributes(
        &self,
        window_id: u32,
        pid: i32,
        layer: i32,
    ) -> ExtendedWindowAttributes {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(_) => {
                return ExtendedWindowAttributes {
                    window_level: layer,
                    ..Default::default()
                }
            }
        };

        for ax_win in ax_windows {
            if ax_win.window_id() == Some(window_id) {
                let ax_id = ax_win.identifier().ok();
                let subrole = ax_win.subrole().ok();

                let (close_exists, close_enabled) = ax_win.get_close_button_info();
                let (fullscreen_exists, fullscreen_enabled) = ax_win.get_fullscreen_button_info();
                let (minimize_exists, minimize_enabled) = ax_win.get_minimize_button_info();
                let (zoom_exists, zoom_enabled) = ax_win.get_zoom_button_info();

                return ExtendedWindowAttributes {
                    ax_id,
                    subrole,
                    window_level: layer,
                    close_button: ButtonInfo::new(close_exists, close_enabled),
                    fullscreen_button: ButtonInfo::new(fullscreen_exists, fullscreen_enabled),
                    minimize_button: ButtonInfo::new(minimize_exists, minimize_enabled),
                    zoom_button: ButtonInfo::new(zoom_exists, zoom_enabled),
                };
            }
        }

        ExtendedWindowAttributes {
            window_level: layer,
            ..Default::default()
        }
    }
}

impl Default for MacOSWindowSystem {
    fn default() -> Self {
        Self
    }
}

/// Trait for manipulating windows (side effects).
/// This abstraction allows mocking in tests.
pub trait WindowManipulator {
    fn apply_window_moves(&self, moves: &[WindowMove]);
    fn apply_layout(&self, display_id: DisplayId, frame: &Rect, geometries: &[WindowGeometry]);
    fn focus_window(&self, window_id: u32, pid: i32);
    fn move_window_to_position(&self, window_id: u32, pid: i32, x: i32, y: i32);
    fn set_window_dimensions(&self, window_id: u32, pid: i32, width: u32, height: u32);
    fn set_window_frame(&self, window_id: u32, pid: i32, x: i32, y: i32, width: u32, height: u32);
    fn close_window(&self, window_id: u32, pid: i32);
    fn exec_command(&self, command: &str, path: &str) -> Result<(), String>;
    fn warp_cursor(&self, x: i32, y: i32);
}

/// macOS implementation of WindowManipulator
pub struct MacOSWindowManipulator;

impl WindowManipulator for MacOSWindowManipulator {
    fn apply_window_moves(&self, moves: &[WindowMove]) {
        use std::collections::HashMap;

        let mut by_pid: HashMap<i32, Vec<&WindowMove>> = HashMap::new();
        for m in moves {
            by_pid.entry(m.pid).or_default().push(m);
        }

        for (pid, pid_moves) in by_pid {
            let app = AXUIElement::application(pid);
            let ax_windows = match app.windows() {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                    continue;
                }
            };

            for m in pid_moves {
                let mut found = false;
                for ax_win in &ax_windows {
                    if let Some(wid) = ax_win.window_id() {
                        if wid == m.window_id {
                            let new_pos = CGPoint::new(m.new_x as f64, m.new_y as f64);
                            if let Err(e) = ax_win.set_position(new_pos) {
                                tracing::warn!(
                                    "Failed to move window (id={}, pid={}, to=({}, {})): {}",
                                    m.window_id,
                                    m.pid,
                                    m.new_x,
                                    m.new_y,
                                    e
                                );
                            } else {
                                tracing::debug!(
                                    "Moved window (id={}, pid={}) to ({}, {})",
                                    m.window_id,
                                    m.pid,
                                    m.new_x,
                                    m.new_y
                                );
                            }
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    tracing::warn!(
                        "Could not find AX window for id {} (pid {})",
                        m.window_id,
                        m.pid
                    );
                }
            }
        }
    }

    fn apply_layout(&self, display_id: DisplayId, frame: &Rect, geometries: &[WindowGeometry]) {
        use std::collections::HashMap;

        let offset_x = frame.x;
        let offset_y = frame.y;

        // Need to get window PIDs - we'll look them up via AX
        // Build a map of window_id -> geometry
        let geom_map: HashMap<u32, &WindowGeometry> =
            geometries.iter().map(|g| (g.id, g)).collect();

        // Get all on-screen windows to find PIDs
        let window_infos = crate::macos::get_on_screen_windows();
        let mut by_pid: HashMap<i32, Vec<(u32, &WindowGeometry)>> = HashMap::new();
        for info in &window_infos {
            if let Some(geom) = geom_map.get(&info.window_id) {
                by_pid
                    .entry(info.pid)
                    .or_default()
                    .push((info.window_id, geom));
            }
        }

        for (pid, windows) in by_pid {
            let app = AXUIElement::application(pid);
            let ax_windows = match app.windows() {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                    continue;
                }
            };

            for (window_id, geom) in windows {
                let mut found = false;
                for ax_win in &ax_windows {
                    if let Some(wid) = ax_win.window_id() {
                        if wid == window_id {
                            let new_x = geom.x + offset_x;
                            let new_y = geom.y + offset_y;
                            let new_pos = CGPoint::new(new_x as f64, new_y as f64);
                            let new_size = CGSize::new(geom.width as f64, geom.height as f64);

                            if let Err(e) = ax_win.set_position(new_pos) {
                                tracing::warn!(
                                    "Failed to set position for window {}: {}",
                                    window_id,
                                    e
                                );
                            }
                            if let Err(e) = ax_win.set_size(new_size) {
                                tracing::warn!(
                                    "Failed to set size for window {}: {}",
                                    window_id,
                                    e
                                );
                            }

                            tracing::debug!(
                                "Applied layout to window {} (pid={}) on display {}: ({}, {}) {}x{}",
                                window_id,
                                pid,
                                display_id,
                                new_x,
                                new_y,
                                geom.width,
                                geom.height
                            );
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    tracing::warn!(
                        "Could not find AX window for id {} (pid {}) when applying layout",
                        window_id,
                        pid
                    );
                }
            }
        }
    }

    fn focus_window(&self, window_id: u32, pid: i32) {
        activate_application(pid);

        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                return;
            }
        };

        for ax_win in &ax_windows {
            if let Some(wid) = ax_win.window_id() {
                if wid == window_id {
                    if let Err(e) = ax_win.raise() {
                        tracing::warn!("Failed to raise window {}: {}", window_id, e);
                    } else {
                        tracing::debug!("Raised window {} (pid {})", window_id, pid);
                    }
                    return;
                }
            }
        }

        tracing::warn!(
            "Could not find AX window for id {} (pid {})",
            window_id,
            pid
        );
    }

    fn move_window_to_position(&self, window_id: u32, pid: i32, x: i32, y: i32) {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                return;
            }
        };

        for ax_win in &ax_windows {
            if let Some(wid) = ax_win.window_id() {
                if wid == window_id {
                    let new_pos = CGPoint::new(x as f64, y as f64);
                    if let Err(e) = ax_win.set_position(new_pos) {
                        tracing::warn!(
                            "Failed to move window {} to ({}, {}): {}",
                            window_id,
                            x,
                            y,
                            e
                        );
                    } else {
                        tracing::info!("Moved window {} to ({}, {})", window_id, x, y);
                    }
                    return;
                }
            }
        }

        tracing::warn!(
            "Could not find AX window for id {} (pid {})",
            window_id,
            pid
        );
    }

    fn set_window_dimensions(&self, window_id: u32, pid: i32, width: u32, height: u32) {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                return;
            }
        };

        for ax_win in &ax_windows {
            if let Some(wid) = ax_win.window_id() {
                if wid == window_id {
                    let new_size = CGSize::new(width as f64, height as f64);
                    if let Err(e) = ax_win.set_size(new_size) {
                        tracing::warn!(
                            "Failed to resize window {} to {}x{}: {}",
                            window_id,
                            width,
                            height,
                            e
                        );
                    } else {
                        tracing::info!("Resized window {} to {}x{}", window_id, width, height);
                    }
                    return;
                }
            }
        }

        tracing::warn!(
            "Could not find AX window for id {} (pid {})",
            window_id,
            pid
        );
    }

    fn set_window_frame(&self, window_id: u32, pid: i32, x: i32, y: i32, width: u32, height: u32) {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                return;
            }
        };

        for ax_win in &ax_windows {
            if let Some(wid) = ax_win.window_id() {
                if wid == window_id {
                    let new_pos = CGPoint::new(x as f64, y as f64);
                    let new_size = CGSize::new(width as f64, height as f64);

                    if let Err(e) = ax_win.set_position(new_pos) {
                        tracing::warn!(
                            "Failed to move window {} to ({}, {}): {}",
                            window_id,
                            x,
                            y,
                            e
                        );
                    }
                    if let Err(e) = ax_win.set_size(new_size) {
                        tracing::warn!(
                            "Failed to resize window {} to {}x{}: {}",
                            window_id,
                            width,
                            height,
                            e
                        );
                    }

                    tracing::info!(
                        "Set window {} frame to ({}, {}) {}x{}",
                        window_id,
                        x,
                        y,
                        width,
                        height
                    );
                    return;
                }
            }
        }

        tracing::warn!(
            "Could not find AX window for id {} (pid {})",
            window_id,
            pid
        );
    }

    fn close_window(&self, window_id: u32, pid: i32) {
        let app = AXUIElement::application(pid);
        let ax_windows = match app.windows() {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to get windows for pid {}: {}", pid, e);
                return;
            }
        };

        for ax_win in &ax_windows {
            if let Some(wid) = ax_win.window_id() {
                if wid == window_id {
                    match ax_win.close_button() {
                        Ok(close_btn) => {
                            if let Err(e) = close_btn.press() {
                                tracing::warn!(
                                    "Failed to press close button for window {}: {}",
                                    window_id,
                                    e
                                );
                            } else {
                                tracing::info!("Closed window {} (pid {})", window_id, pid);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to get close button for window {}: {}",
                                window_id,
                                e
                            );
                        }
                    }
                    return;
                }
            }
        }

        tracing::warn!(
            "Could not find AX window for id {} (pid {})",
            window_id,
            pid
        );
    }

    fn exec_command(&self, command: &str, path: &str) -> Result<(), String> {
        crate::macos::exec_command(command, path)
    }

    fn warp_cursor(&self, x: i32, y: i32) {
        use core_graphics::display::CGWarpMouseCursorPosition;

        let point = CGPoint::new(x as f64, y as f64);
        let result = unsafe { CGWarpMouseCursorPosition(point) };
        if result != 0 {
            tracing::warn!("Failed to warp cursor to ({}, {}): error {}", x, y, result);
        } else {
            tracing::debug!("Warped cursor to ({}, {})", x, y);
        }
    }
}

impl Default for MacOSWindowManipulator {
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

        fn get_extended_attributes(
            &self,
            _window_id: u32,
            _pid: i32,
            layer: i32,
        ) -> ExtendedWindowAttributes {
            // In tests, return default extended attributes with provided layer
            ExtendedWindowAttributes {
                window_level: layer,
                close_button: ButtonInfo::new(true, Some(true)),
                fullscreen_button: ButtonInfo::new(true, Some(true)),
                minimize_button: ButtonInfo::new(true, Some(true)),
                zoom_button: ButtonInfo::new(true, Some(true)),
                ..Default::default()
            }
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
            name: format!("Display {}", id),
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
            bundle_id: None,
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
