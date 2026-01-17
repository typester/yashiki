use super::{offscreen_x, Display, Rect, Tag, Window, WindowId};
use crate::event::Event;
use crate::macos::{get_all_displays, get_focused_window, get_on_screen_windows, DisplayId};
use std::collections::{HashMap, HashSet};
use yashiki_ipc::{Direction, OutputDirection};

#[derive(Debug, Clone)]
pub struct WindowMove {
    pub window_id: WindowId,
    pub pid: i32,
    pub x: i32,
    pub y: i32,
}

pub struct State {
    pub windows: HashMap<WindowId, Window>,
    pub displays: HashMap<DisplayId, Display>,
    pub focused: Option<WindowId>,
    pub focused_display: DisplayId,
    default_tag: Tag,
}

impl State {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            displays: HashMap::new(),
            focused: None,
            focused_display: 0,
            default_tag: Tag::new(1),
        }
    }

    pub fn visible_tags(&self) -> Tag {
        self.displays
            .get(&self.focused_display)
            .map(|d| d.visible_tags)
            .unwrap_or(Tag::new(1))
    }

    pub fn sync_all(&mut self) {
        // Sync displays
        let display_infos = get_all_displays();
        for info in &display_infos {
            if !self.displays.contains_key(&info.id) {
                let display = Display::new(info.id, Rect::from_bounds(&info.frame));
                self.displays.insert(info.id, display);
            } else if let Some(display) = self.displays.get_mut(&info.id) {
                display.frame = Rect::from_bounds(&info.frame);
            }
            if info.is_main && self.focused_display == 0 {
                self.focused_display = info.id;
            }
        }

        // Remove displays that no longer exist
        let current_ids: HashSet<_> = display_infos.iter().map(|d| d.id).collect();
        self.displays.retain(|id, _| current_ids.contains(id));

        // Sync windows
        let window_infos = get_on_screen_windows();
        self.sync_with_window_infos(&window_infos);
        self.sync_focused_window();

        tracing::info!(
            "State initialized with {} displays, {} windows",
            self.displays.len(),
            self.windows.len()
        );
        for d in self.displays.values() {
            tracing::debug!("  Display {}: {:?}", d.id, d.frame);
        }
        for window in self.windows.values() {
            tracing::debug!(
                "  - [{}] {} ({}) on display {}",
                window.id,
                window.title,
                window.app_name,
                window.display_id
            );
        }
    }

    pub fn sync_focused_window(&mut self) {
        let focused_window = match get_focused_window() {
            Ok(win) => win,
            Err(_) => {
                self.set_focused(None);
                return;
            }
        };

        let pid = match focused_window.pid() {
            Ok(pid) => pid,
            Err(_) => {
                self.set_focused(None);
                return;
            }
        };

        let position = focused_window.position().ok();
        let size = focused_window.size().ok();

        // Find matching window by PID and bounds
        let window = self.windows.values().find(|w| {
            if w.pid != pid {
                return false;
            }
            if let (Some(pos), Some(sz)) = (&position, &size) {
                let x_match = (w.frame.x - pos.x as i32).abs() <= 1;
                let y_match = (w.frame.y - pos.y as i32).abs() <= 1;
                let w_match = (w.frame.width as i32 - sz.width as i32).abs() <= 1;
                let h_match = (w.frame.height as i32 - sz.height as i32).abs() <= 1;
                x_match && y_match && w_match && h_match
            } else {
                true
            }
        });

        if let Some(window) = window {
            let window_id = window.id;
            let display_id = window.display_id;
            self.set_focused(Some(window_id));
            if self.focused_display != display_id {
                tracing::info!(
                    "Focused display changed: {} -> {}",
                    self.focused_display,
                    display_id
                );
                self.focused_display = display_id;
            }
        } else {
            self.set_focused(None);
        }
    }

    /// Sync windows for a specific PID. Returns true if window count changed.
    pub fn sync_pid(&mut self, pid: i32) -> bool {
        let window_infos = get_on_screen_windows();
        let pid_window_infos: Vec<_> = window_infos.iter().filter(|w| w.pid == pid).collect();

        let current_ids: HashSet<WindowId> = self
            .windows
            .values()
            .filter(|w| w.pid == pid)
            .map(|w| w.id)
            .collect();
        let new_ids: HashSet<WindowId> = pid_window_infos.iter().map(|w| w.window_id).collect();

        let mut changed = false;

        // Remove windows that no longer exist
        for id in current_ids.difference(&new_ids) {
            if let Some(window) = self.windows.remove(id) {
                tracing::info!(
                    "Window removed: [{}] {} ({})",
                    window.id,
                    window.title,
                    window.app_name
                );
                changed = true;
            }
        }

        // Add new windows
        for id in new_ids.difference(&current_ids) {
            if let Some(info) = pid_window_infos.iter().find(|w| w.window_id == *id) {
                let display_id = self.find_display_for_bounds(&info.bounds);
                let window = Window::from_window_info(info, self.default_tag, display_id);
                tracing::info!(
                    "Window added: [{}] {} ({}) on display {}",
                    window.id,
                    window.title,
                    window.app_name,
                    display_id
                );
                self.windows.insert(window.id, window);
                changed = true;
            }
        }

        // Update existing windows
        for id in current_ids.intersection(&new_ids) {
            if let Some(info) = pid_window_infos.iter().find(|w| w.window_id == *id) {
                let new_title = info.name.clone().unwrap_or_default();
                let new_frame = Rect::from_bounds(&info.bounds);
                let new_display_id = self.find_display_for_bounds(&info.bounds);

                if let Some(window) = self.windows.get_mut(id) {
                    let title_changed = window.title != new_title;
                    let frame_changed = window.frame.x != new_frame.x
                        || window.frame.y != new_frame.y
                        || window.frame.width != new_frame.width
                        || window.frame.height != new_frame.height;

                    if title_changed || frame_changed {
                        window.title = new_title;
                        window.frame = new_frame;
                        window.display_id = new_display_id;
                        tracing::debug!(
                            "Window updated: [{}] {} ({})",
                            window.id,
                            window.title,
                            window.app_name
                        );
                    }
                }
            }
        }

        changed
    }

    fn find_display_for_bounds(&self, bounds: &crate::macos::Bounds) -> DisplayId {
        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;

        for display in self.displays.values() {
            let dx = display.frame.x as f64;
            let dy = display.frame.y as f64;
            let dw = display.frame.width as f64;
            let dh = display.frame.height as f64;

            if cx >= dx && cx < dx + dw && cy >= dy && cy < dy + dh {
                return display.id;
            }
        }

        // Fallback to focused display or first display
        if self.focused_display != 0 {
            self.focused_display
        } else {
            self.displays.keys().next().copied().unwrap_or(0)
        }
    }

    /// Handle an event. Returns true if window count changed (needs retile).
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::WindowCreated { pid } | Event::WindowDestroyed { pid } => self.sync_pid(*pid),
            Event::WindowMoved { pid }
            | Event::WindowResized { pid }
            | Event::WindowMiniaturized { pid }
            | Event::WindowDeminiaturized { pid } => {
                self.sync_pid(*pid);
                false
            }
            Event::FocusedWindowChanged { .. } | Event::ApplicationActivated { .. } => {
                self.sync_focused_window();
                false
            }
            Event::ApplicationDeactivated { .. }
            | Event::ApplicationHidden { .. }
            | Event::ApplicationShown { .. } => false,
        }
    }

    pub fn set_focused(&mut self, window_id: Option<WindowId>) {
        if self.focused != window_id {
            tracing::info!("Focus changed: {:?} -> {:?}", self.focused, window_id);
            self.focused = window_id;
        }
    }

    fn sync_with_window_infos(&mut self, window_infos: &[crate::macos::WindowInfo]) {
        let current_ids: HashSet<WindowId> = self.windows.keys().copied().collect();
        let new_ids: HashSet<WindowId> = window_infos.iter().map(|w| w.window_id).collect();

        // Remove windows that no longer exist
        for id in current_ids.difference(&new_ids) {
            self.windows.remove(id);
        }

        // Add new windows
        for info in window_infos {
            if !self.windows.contains_key(&info.window_id) {
                let display_id = self.find_display_for_bounds(&info.bounds);
                let window = Window::from_window_info(info, self.default_tag, display_id);
                self.windows.insert(window.id, window);
            }
        }

        // Update existing windows
        for info in window_infos {
            let new_display_id = self.find_display_for_bounds(&info.bounds);
            if let Some(window) = self.windows.get_mut(&info.window_id) {
                window.title = info.name.clone().unwrap_or_default();
                window.frame = Rect::from_bounds(&info.bounds);
                window.display_id = new_display_id;
            }
        }
    }

    // Tag operations - now operate on focused_display

    pub fn view_tag(&mut self, tag: u32) -> Vec<WindowMove> {
        let Some(disp) = self.displays.get_mut(&self.focused_display) else {
            return vec![];
        };
        let new_visible = Tag::new(tag);
        if disp.visible_tags == new_visible {
            return vec![];
        }
        tracing::info!(
            "View tag on display {}: {} -> {}",
            self.focused_display,
            disp.visible_tags.mask(),
            new_visible.mask()
        );
        disp.visible_tags = new_visible;
        self.compute_layout_changes_for_display(self.focused_display)
    }

    pub fn toggle_view_tag(&mut self, tag: u32) -> Vec<WindowMove> {
        let Some(disp) = self.displays.get_mut(&self.focused_display) else {
            return vec![];
        };
        let tag = Tag::new(tag);
        let new_visible = disp.visible_tags.toggle(tag);
        if new_visible.mask() == 0 {
            return vec![];
        }
        tracing::info!(
            "Toggle view tag on display {}: {} -> {}",
            self.focused_display,
            disp.visible_tags.mask(),
            new_visible.mask()
        );
        disp.visible_tags = new_visible;
        self.compute_layout_changes_for_display(self.focused_display)
    }

    pub fn move_focused_to_tag(&mut self, tag: u32) -> Vec<WindowMove> {
        let Some(focused_id) = self.focused else {
            return vec![];
        };
        let new_tag = Tag::new(tag);
        let display_id = if let Some(window) = self.windows.get_mut(&focused_id) {
            tracing::info!("Move window {} to tag {}", window.id, new_tag.mask());
            window.tags = new_tag;
            window.display_id
        } else {
            return vec![];
        };
        self.compute_layout_changes_for_display(display_id)
    }

    pub fn toggle_focused_window_tag(&mut self, tag: u32) -> Vec<WindowMove> {
        let Some(focused_id) = self.focused else {
            return vec![];
        };
        let tag = Tag::new(tag);
        let display_id = if let Some(window) = self.windows.get_mut(&focused_id) {
            let new_tags = window.tags.toggle(tag);
            if new_tags.mask() == 0 {
                return vec![];
            }
            tracing::info!(
                "Toggle window {} tag: {} -> {}",
                window.id,
                window.tags.mask(),
                new_tags.mask()
            );
            window.tags = new_tags;
            window.display_id
        } else {
            return vec![];
        };
        self.compute_layout_changes_for_display(display_id)
    }

    pub fn focus_window(&self, direction: Direction) -> Option<(WindowId, i32)> {
        let visible_tags = self.visible_tags();
        let visible: Vec<_> = self
            .windows
            .values()
            .filter(|w| {
                w.display_id == self.focused_display
                    && w.tags.intersects(visible_tags)
                    && !w.is_offscreen()
            })
            .collect();

        if visible.is_empty() {
            return None;
        }

        match direction {
            Direction::Next | Direction::Prev => {
                self.focus_window_stack(&visible, direction == Direction::Next)
            }
            Direction::Left | Direction::Right | Direction::Up | Direction::Down => {
                self.focus_window_directional(&visible, direction)
            }
        }
    }

    fn focus_window_stack(&self, visible: &[&Window], forward: bool) -> Option<(WindowId, i32)> {
        if visible.is_empty() {
            return None;
        }

        let mut sorted: Vec<_> = visible.iter().map(|w| (w.id, w.pid)).collect();
        sorted.sort_by_key(|(id, _)| *id);

        let current_idx = self
            .focused
            .and_then(|id| sorted.iter().position(|(wid, _)| *wid == id));

        let next_idx = match current_idx {
            Some(idx) => {
                if forward {
                    (idx + 1) % sorted.len()
                } else {
                    (idx + sorted.len() - 1) % sorted.len()
                }
            }
            None => 0,
        };

        Some(sorted[next_idx])
    }

    fn focus_window_directional(
        &self,
        visible: &[&Window],
        direction: Direction,
    ) -> Option<(WindowId, i32)> {
        let focused_id = self.focused?;
        let focused = visible.iter().find(|w| w.id == focused_id)?;

        let (fx, fy) = focused.center();
        let mut best: Option<(&Window, i32)> = None;

        for window in visible {
            if window.id == focused_id {
                continue;
            }

            let (wx, wy) = window.center();

            let is_candidate = match direction {
                Direction::Left => wx < fx,
                Direction::Right => wx > fx,
                Direction::Up => wy < fy,
                Direction::Down => wy > fy,
                _ => false,
            };

            if !is_candidate {
                continue;
            }

            let distance = (wx - fx).abs() + (wy - fy).abs();

            match &best {
                Some((_, best_dist)) if distance < *best_dist => {
                    best = Some((window, distance));
                }
                None => {
                    best = Some((window, distance));
                }
                _ => {}
            }
        }

        best.map(|(w, _)| (w.id, w.pid))
    }

    /// Focus the next/prev output (display). Returns window to focus on target display.
    pub fn focus_output(&mut self, direction: OutputDirection) -> Option<(WindowId, i32)> {
        if self.displays.len() <= 1 {
            return None;
        }

        let mut display_ids: Vec<_> = self.displays.keys().copied().collect();
        display_ids.sort();

        let current_idx = display_ids
            .iter()
            .position(|&id| id == self.focused_display)?;

        let next_idx = match direction {
            OutputDirection::Next => (current_idx + 1) % display_ids.len(),
            OutputDirection::Prev => (current_idx + display_ids.len() - 1) % display_ids.len(),
        };

        let target_display_id = display_ids[next_idx];
        tracing::info!(
            "Focus output: {} -> {} ({:?})",
            self.focused_display,
            target_display_id,
            direction
        );
        self.focused_display = target_display_id;

        // Return a window on the target display to focus
        let visible = self.visible_windows_on_display(target_display_id);
        visible.first().map(|w| (w.id, w.pid))
    }

    /// Send focused window to next/prev output. Returns (source_display, target_display) for retiling.
    pub fn send_to_output(&mut self, direction: OutputDirection) -> Option<(DisplayId, DisplayId)> {
        let focused_id = self.focused?;

        if self.displays.len() <= 1 {
            return None;
        }

        let mut display_ids: Vec<_> = self.displays.keys().copied().collect();
        display_ids.sort();

        let source_display_id = self.windows.get(&focused_id)?.display_id;
        let current_idx = display_ids.iter().position(|&id| id == source_display_id)?;

        let next_idx = match direction {
            OutputDirection::Next => (current_idx + 1) % display_ids.len(),
            OutputDirection::Prev => (current_idx + display_ids.len() - 1) % display_ids.len(),
        };

        let target_display_id = display_ids[next_idx];

        if source_display_id == target_display_id {
            return None;
        }

        let target_display = self.displays.get(&target_display_id)?;
        let target_x = target_display.frame.x;
        let target_y = target_display.frame.y;

        let window = self.windows.get_mut(&focused_id)?;
        tracing::info!(
            "Send window {} to output: {} -> {}",
            window.id,
            source_display_id,
            target_display_id
        );
        window.display_id = target_display_id;
        window.frame.x = target_x;
        window.frame.y = target_y;

        // Also update focused_display to follow the window
        self.focused_display = target_display_id;

        Some((source_display_id, target_display_id))
    }

    fn compute_layout_changes_for_display(&mut self, display_id: DisplayId) -> Vec<WindowMove> {
        let Some(display) = self.displays.get(&display_id) else {
            return vec![];
        };
        let visible_tags = display.visible_tags;

        let mut moves = Vec::new();

        for window in self.windows.values_mut() {
            if window.display_id != display_id {
                continue;
            }

            let should_be_visible = window.tags.intersects(visible_tags);
            let is_visible = !window.is_offscreen();

            if should_be_visible && !is_visible {
                if let Some(saved) = window.saved_frame.take() {
                    tracing::debug!("Showing window {} at ({}, {})", window.id, saved.x, saved.y);
                    moves.push(WindowMove {
                        window_id: window.id,
                        pid: window.pid,
                        x: saved.x,
                        y: saved.y,
                    });
                    window.frame.x = saved.x;
                    window.frame.y = saved.y;
                }
            } else if !should_be_visible && is_visible {
                tracing::debug!(
                    "Hiding window {} (was at ({}, {}))",
                    window.id,
                    window.frame.x,
                    window.frame.y
                );
                window.saved_frame = Some(window.frame);
                moves.push(WindowMove {
                    window_id: window.id,
                    pid: window.pid,
                    x: offscreen_x(),
                    y: window.frame.y,
                });
                window.frame.x = offscreen_x();
            }
        }

        moves
    }

    /// Get windows visible on a specific display
    pub fn visible_windows_on_display(&self, display_id: DisplayId) -> Vec<&Window> {
        let Some(display) = self.displays.get(&display_id) else {
            return vec![];
        };
        self.windows
            .values()
            .filter(|w| {
                w.display_id == display_id
                    && w.tags.intersects(display.visible_tags)
                    && !w.is_offscreen()
            })
            .collect()
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
