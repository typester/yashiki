use super::{Display, Rect, Tag, Window, WindowId};
use crate::effect::Effect;
use crate::event::Event;
use crate::macos::DisplayId;
use crate::platform::WindowSystem;
use std::collections::{HashMap, HashSet};
use yashiki_ipc::{
    Direction, OutputDirection, OutputSpecifier, RuleAction, RuleMatcher, WindowRule,
};

#[derive(Debug, Clone, PartialEq)]
pub struct WindowMove {
    pub window_id: WindowId,
    pub pid: i32,
    pub old_x: i32,
    pub old_y: i32,
    pub new_x: i32,
    pub new_y: i32,
}

pub struct State {
    pub windows: HashMap<WindowId, Window>,
    pub displays: HashMap<DisplayId, Display>,
    pub focused: Option<WindowId>,
    pub focused_display: DisplayId,
    default_tag: Tag,
    pub default_layout: String,
    pub tag_layouts: HashMap<u8, String>,
    pub exec_path: String,
    pub rules: Vec<WindowRule>,
}

impl State {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            displays: HashMap::new(),
            focused: None,
            focused_display: 0,
            default_tag: Tag::new(1),
            default_layout: "tatami".to_string(),
            tag_layouts: HashMap::new(),
            exec_path: String::new(),
            rules: Vec::new(),
        }
    }

    pub fn set_default_layout(&mut self, layout: String) {
        tracing::info!("Set default layout: {}", layout);
        self.default_layout = layout;
    }

    pub fn set_layout_on_display(
        &mut self,
        tags: Option<u32>,
        display_id: Option<DisplayId>,
        layout: String,
    ) {
        let target_display = display_id.unwrap_or(self.focused_display);

        match tags {
            Some(tags) => {
                // Set layout for specific tag (use first tag from mask)
                let first_tag = Tag::from_mask(tags).first_tag().unwrap_or(1);
                tracing::info!("Set layout for tag {}: {}", first_tag, layout);
                self.tag_layouts.insert(first_tag as u8, layout);
            }
            None => {
                // Set layout for current tag on target display
                let Some(disp) = self.displays.get(&target_display) else {
                    return;
                };
                if let Some(current_tag) = disp.visible_tags.first_tag() {
                    tracing::info!(
                        "Set layout for current tag {} on display {}: {}",
                        current_tag,
                        target_display,
                        layout
                    );
                    self.tag_layouts.insert(current_tag as u8, layout.clone());
                }
                // Also update the display's current layout
                let disp = self.displays.get_mut(&target_display).unwrap();
                disp.previous_layout = disp.current_layout.take();
                disp.current_layout = Some(layout);
            }
        }
    }

    pub fn get_layout_on_display(&self, tags: Option<u32>, display_id: Option<DisplayId>) -> &str {
        let target_display = display_id.unwrap_or(self.focused_display);

        match tags {
            Some(tags) => {
                let first_tag = Tag::from_mask(tags).first_tag().unwrap_or(1);
                self.resolve_layout_for_tag(first_tag as u8)
            }
            None => self.current_layout_for_display(target_display),
        }
    }

    pub fn resolve_layout_for_tag(&self, tag: u8) -> &str {
        self.tag_layouts
            .get(&tag)
            .map(|s| s.as_str())
            .unwrap_or(&self.default_layout)
    }

    pub fn current_layout(&self) -> &str {
        self.displays
            .get(&self.focused_display)
            .and_then(|d| d.current_layout.as_deref())
            .unwrap_or(&self.default_layout)
    }

    pub fn current_layout_for_display(&self, display_id: DisplayId) -> &str {
        self.displays
            .get(&display_id)
            .and_then(|d| d.current_layout.as_deref())
            .unwrap_or(&self.default_layout)
    }

    pub fn visible_tags(&self) -> Tag {
        self.displays
            .get(&self.focused_display)
            .map(|d| d.visible_tags)
            .unwrap_or(Tag::new(1))
    }

    pub fn resolve_output(&self, spec: &OutputSpecifier) -> Option<DisplayId> {
        match spec {
            OutputSpecifier::Id(id) => {
                if self.displays.contains_key(id) {
                    Some(*id)
                } else {
                    None
                }
            }
            OutputSpecifier::Name(name) => {
                // Case-insensitive partial match
                let name_lower = name.to_lowercase();
                self.displays
                    .values()
                    .find(|d| d.name.to_lowercase().contains(&name_lower))
                    .map(|d| d.id)
            }
        }
    }

    pub fn get_target_display(
        &self,
        output: Option<&OutputSpecifier>,
    ) -> Result<DisplayId, String> {
        match output {
            Some(spec) => self
                .resolve_output(spec)
                .ok_or_else(|| format!("Output not found: {:?}", spec)),
            None => Ok(self.focused_display),
        }
    }

    pub fn handle_display_change<W: WindowSystem>(
        &mut self,
        ws: &W,
    ) -> (Vec<WindowMove>, Vec<DisplayId>) {
        let display_infos = ws.get_all_displays();
        let current_ids: HashSet<_> = display_infos.iter().map(|d| d.id).collect();
        let removed_ids: Vec<_> = self
            .displays
            .keys()
            .filter(|id| !current_ids.contains(id))
            .copied()
            .collect();

        if removed_ids.is_empty() {
            // No displays were removed, just update frames and add new displays
            self.sync_all(ws);
            return (vec![], vec![]);
        }

        tracing::info!("Displays disconnected: {:?}", removed_ids);

        // Find fallback display (prefer main display, then any remaining display)
        let fallback_display = display_infos
            .iter()
            .find(|d| d.is_main)
            .or_else(|| display_infos.first())
            .map(|d| d.id);

        let Some(fallback_id) = fallback_display else {
            tracing::warn!("No fallback display available");
            return (vec![], vec![]);
        };

        // Find orphaned windows and move them to the fallback display
        let mut window_moves = Vec::new();
        let mut affected_displays = HashSet::new();

        for window in self.windows.values_mut() {
            if removed_ids.contains(&window.display_id) {
                tracing::info!(
                    "Moving orphaned window {} ({}) from display {} to {}",
                    window.id,
                    window.app_name,
                    window.display_id,
                    fallback_id
                );
                window.display_id = fallback_id;
                affected_displays.insert(fallback_id);
            }
        }

        // Update focused_display if it was removed
        if removed_ids.contains(&self.focused_display) {
            tracing::info!(
                "Focused display {} was removed, switching to {}",
                self.focused_display,
                fallback_id
            );
            self.focused_display = fallback_id;
        }

        // Remove disconnected displays from state
        for id in &removed_ids {
            self.displays.remove(id);
        }

        // Sync remaining displays (update frames, add new displays)
        self.sync_all(ws);

        // Compute window moves for hiding/showing based on new tag visibility
        for display_id in &affected_displays {
            let moves = self.compute_layout_changes_for_display(*display_id);
            window_moves.extend(moves);
        }

        let displays_to_retile: Vec<_> = affected_displays.into_iter().collect();
        (window_moves, displays_to_retile)
    }

    pub fn sync_all<W: WindowSystem>(&mut self, ws: &W) {
        // Sync displays
        let display_infos = ws.get_all_displays();
        for info in &display_infos {
            self.displays
                .entry(info.id)
                .and_modify(|display| {
                    display.name = info.name.clone();
                    display.frame = Rect::from_bounds(&info.frame);
                    display.is_main = info.is_main;
                })
                .or_insert_with(|| {
                    Display::new(
                        info.id,
                        info.name.clone(),
                        Rect::from_bounds(&info.frame),
                        info.is_main,
                    )
                });
            if info.is_main && self.focused_display == 0 {
                self.focused_display = info.id;
            }
        }

        // Remove displays that no longer exist
        let current_ids: HashSet<_> = display_infos.iter().map(|d| d.id).collect();
        self.displays.retain(|id, _| current_ids.contains(id));

        // Sync windows
        let window_infos = ws.get_on_screen_windows();
        self.sync_with_window_infos(&window_infos);
        self.sync_focused_window(ws);

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

    pub fn sync_focused_window<W: WindowSystem>(&mut self, ws: &W) {
        self.sync_focused_window_with_hint(ws, None);
    }

    /// Sync focused window, with optional PID hint for fallback when accessibility API fails
    /// (common with Electron apps like Microsoft Teams)
    pub fn sync_focused_window_with_hint<W: WindowSystem>(
        &mut self,
        ws: &W,
        pid_hint: Option<i32>,
    ) {
        // Try the normal accessibility API first
        if let Some(focused_info) = ws.get_focused_window() {
            let window_id = focused_info.window_id;
            if let Some(window) = self.windows.get(&window_id) {
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
                return;
            }
        }

        // Fallback: if we have a PID hint (from ApplicationActivated event),
        // find a visible window for that PID
        if let Some(pid) = pid_hint {
            let visible_tags = self.visible_tags();
            let pid_windows: Vec<_> = self
                .windows
                .values()
                .filter(|w| w.pid == pid && w.tags.intersects(visible_tags) && !w.is_hidden())
                .collect();

            if let Some(window) = pid_windows.first() {
                tracing::debug!(
                    "Focus fallback: using window {} for pid {} (accessibility API unavailable)",
                    window.id,
                    pid
                );
                let window_id = window.id;
                let display_id = window.display_id;
                self.set_focused(Some(window_id));
                if self.focused_display != display_id {
                    self.focused_display = display_id;
                }
                return;
            }
        }

        self.set_focused(None);
    }

    /// Sync windows for a specific PID.
    /// Returns (changed, new_window_ids) where changed is true if window count changed,
    /// and new_window_ids contains the IDs of newly added windows.
    pub fn sync_pid<W: WindowSystem>(&mut self, ws: &W, pid: i32) -> (bool, Vec<WindowId>) {
        let window_infos = ws.get_on_screen_windows();
        let pid_window_infos: Vec<_> = window_infos.iter().filter(|w| w.pid == pid).collect();

        let current_ids: HashSet<WindowId> = self
            .windows
            .values()
            .filter(|w| w.pid == pid)
            .map(|w| w.id)
            .collect();
        let new_ids: HashSet<WindowId> = pid_window_infos.iter().map(|w| w.window_id).collect();

        let mut changed = false;
        let mut added_window_ids = Vec::new();

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
                added_window_ids.push(*id);
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
                        tracing::debug!(
                            "Window updated: [{}] {} ({}) pos=({},{}) -> ({},{})",
                            window.id,
                            window.title,
                            window.app_name,
                            window.frame.x,
                            window.frame.y,
                            new_frame.x,
                            new_frame.y
                        );
                        window.title = new_title;
                        window.frame = new_frame;
                        window.display_id = new_display_id;
                    }
                }
            }
        }

        (changed, added_window_ids)
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

    /// Handle an event.
    /// Returns (needs_retile, new_window_ids) where needs_retile is true if window count changed.
    pub fn handle_event<W: WindowSystem>(
        &mut self,
        ws: &W,
        event: &Event,
    ) -> (bool, Vec<WindowId>) {
        match event {
            Event::WindowCreated { pid } | Event::WindowDestroyed { pid } => {
                self.sync_pid(ws, *pid)
            }
            Event::WindowMoved { pid }
            | Event::WindowResized { pid }
            | Event::WindowMiniaturized { pid }
            | Event::WindowDeminiaturized { pid } => {
                let (_, _) = self.sync_pid(ws, *pid);
                (false, vec![])
            }
            Event::FocusedWindowChanged => {
                self.sync_focused_window(ws);
                (false, vec![])
            }
            Event::ApplicationActivated { pid } => {
                self.sync_focused_window_with_hint(ws, Some(*pid));
                (false, vec![])
            }
            Event::ApplicationDeactivated | Event::ApplicationHidden | Event::ApplicationShown => {
                (false, vec![])
            }
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
            self.remove_from_window_order(*id);
            self.windows.remove(id);
        }

        // Add new windows
        for info in window_infos {
            if !self.windows.contains_key(&info.window_id) {
                let display_id = self.find_display_for_bounds(&info.bounds);
                let window = Window::from_window_info(info, self.default_tag, display_id);
                self.add_to_window_order(window.id, display_id);
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

    // Tag operations - now operate on focused_display or specified display

    pub fn view_tags(&mut self, tags: u32) -> Vec<WindowMove> {
        self.view_tags_on_display(tags, self.focused_display)
    }

    pub fn view_tags_on_display(&mut self, tags: u32, display_id: DisplayId) -> Vec<WindowMove> {
        let new_visible = Tag::from_mask(tags);
        let first_tag = new_visible.first_tag().unwrap_or(1);
        let new_layout = self.resolve_layout_for_tag(first_tag as u8).to_string();
        let Some(disp) = self.displays.get_mut(&display_id) else {
            return vec![];
        };
        if disp.visible_tags == new_visible {
            return vec![];
        }
        tracing::info!(
            "View tags on display {}: {} -> {}, layout: {:?} -> {}",
            display_id,
            disp.visible_tags.mask(),
            new_visible.mask(),
            disp.current_layout,
            new_layout
        );
        disp.previous_visible_tags = disp.visible_tags;
        disp.visible_tags = new_visible;
        disp.previous_layout = disp.current_layout.take();
        disp.current_layout = Some(new_layout);
        self.compute_layout_changes_for_display(display_id)
    }

    pub fn toggle_tags_on_display(&mut self, tags: u32, display_id: DisplayId) -> Vec<WindowMove> {
        let Some(disp) = self.displays.get_mut(&display_id) else {
            return vec![];
        };
        let tag = Tag::from_mask(tags);
        let new_visible = disp.visible_tags.toggle(tag);
        if new_visible.mask() == 0 {
            return vec![];
        }
        tracing::info!(
            "Toggle tags on display {}: {} -> {}",
            display_id,
            disp.visible_tags.mask(),
            new_visible.mask()
        );
        disp.previous_visible_tags = disp.visible_tags;
        disp.visible_tags = new_visible;
        self.compute_layout_changes_for_display(display_id)
    }

    pub fn view_tags_last(&mut self) -> Vec<WindowMove> {
        let Some(disp) = self.displays.get_mut(&self.focused_display) else {
            return vec![];
        };
        if disp.visible_tags == disp.previous_visible_tags {
            return vec![];
        }
        tracing::info!(
            "View tags last on display {}: {} -> {}, layout: {:?} -> {:?}",
            self.focused_display,
            disp.visible_tags.mask(),
            disp.previous_visible_tags.mask(),
            disp.current_layout,
            disp.previous_layout
        );
        std::mem::swap(&mut disp.visible_tags, &mut disp.previous_visible_tags);
        std::mem::swap(&mut disp.current_layout, &mut disp.previous_layout);
        self.compute_layout_changes_for_display(self.focused_display)
    }

    pub fn move_focused_to_tags(&mut self, tags: u32) -> Vec<WindowMove> {
        let Some(focused_id) = self.focused else {
            return vec![];
        };
        let new_tags = Tag::from_mask(tags);
        let display_id = if let Some(window) = self.windows.get_mut(&focused_id) {
            tracing::info!("Move window {} to tags {}", window.id, new_tags.mask());
            window.tags = new_tags;
            window.display_id
        } else {
            return vec![];
        };
        self.compute_layout_changes_for_display(display_id)
    }

    pub fn toggle_focused_window_tags(&mut self, tags: u32) -> Vec<WindowMove> {
        let Some(focused_id) = self.focused else {
            return vec![];
        };
        let tag = Tag::from_mask(tags);
        let display_id = if let Some(window) = self.windows.get_mut(&focused_id) {
            let new_tags = window.tags.toggle(tag);
            if new_tags.mask() == 0 {
                return vec![];
            }
            tracing::info!(
                "Toggle window {} tags: {} -> {}",
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
                    && !w.is_hidden()
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
        // Hide windows to bottom-right corner (AeroSpace-style)
        // Position window's top-left at screen's bottom-right, so entire window is off-screen
        // Subtract 1 pixel offset like AeroSpace does
        let hide_x = display.frame.x + display.frame.width as i32 - 1;
        let hide_y = display.frame.y + display.frame.height as i32 - 1;

        let mut moves = Vec::new();

        for window in self.windows.values_mut() {
            if window.display_id != display_id {
                tracing::trace!(
                    "Skipping window {} - display {} != {}",
                    window.id,
                    window.display_id,
                    display_id
                );
                continue;
            }

            let should_be_visible = window.tags.intersects(visible_tags);
            let is_visible = !window.is_hidden();

            tracing::debug!(
                "Window {}: tags={}, visible_tags={}, should_visible={}, frame=({},{}), is_visible={}, saved_frame={:?}",
                window.id,
                window.tags.mask(),
                visible_tags.mask(),
                should_be_visible,
                window.frame.x,
                window.frame.y,
                is_visible,
                window.saved_frame.as_ref().map(|f| (f.x, f.y))
            );

            if should_be_visible && !is_visible {
                if let Some(saved) = window.saved_frame.take() {
                    tracing::debug!(
                        "Showing window {} from ({}, {}) to ({}, {})",
                        window.id,
                        window.frame.x,
                        window.frame.y,
                        saved.x,
                        saved.y
                    );
                    moves.push(WindowMove {
                        window_id: window.id,
                        pid: window.pid,
                        old_x: window.frame.x,
                        old_y: window.frame.y,
                        new_x: saved.x,
                        new_y: saved.y,
                    });
                    window.frame = saved;
                }
            } else if !should_be_visible && is_visible {
                tracing::debug!(
                    "Hiding window {} from ({}, {}) to ({}, {})",
                    window.id,
                    window.frame.x,
                    window.frame.y,
                    hide_x,
                    hide_y
                );
                moves.push(WindowMove {
                    window_id: window.id,
                    pid: window.pid,
                    old_x: window.frame.x,
                    old_y: window.frame.y,
                    new_x: hide_x,
                    new_y: hide_y,
                });
                window.saved_frame = Some(window.frame);
                window.frame.x = hide_x;
                window.frame.y = hide_y;
            }
        }

        moves
    }

    /// Get windows visible on a specific display for tiling, sorted by window_order.
    /// Excludes floating windows.
    pub fn visible_windows_on_display(&self, display_id: DisplayId) -> Vec<&Window> {
        let Some(display) = self.displays.get(&display_id) else {
            return vec![];
        };
        let mut windows: Vec<&Window> = self
            .windows
            .values()
            .filter(|w| {
                w.display_id == display_id
                    && w.tags.intersects(display.visible_tags)
                    && !w.is_hidden()
                    && !w.is_floating
            })
            .collect();

        // Sort by window_order (windows not in order go to end, sorted by ID)
        windows.sort_by_key(|w| {
            display
                .window_order
                .iter()
                .position(|&id| id == w.id)
                .map(|p| (0, p))
                .unwrap_or((1, w.id as usize))
        });
        windows
    }

    /// Add window to display's window_order if not present
    fn add_to_window_order(&mut self, window_id: WindowId, display_id: DisplayId) {
        if let Some(display) = self.displays.get_mut(&display_id) {
            if !display.window_order.contains(&window_id) {
                display.window_order.push(window_id);
            }
        }
    }

    /// Remove window from all display window_orders
    fn remove_from_window_order(&mut self, window_id: WindowId) {
        for display in self.displays.values_mut() {
            display.window_order.retain(|&id| id != window_id);
        }
    }

    // Rule management

    /// Add a rule and sort by specificity (more specific rules first)
    pub fn add_rule(&mut self, rule: WindowRule) {
        tracing::info!("Adding rule: {:?} -> {:?}", rule.matcher, rule.action);
        self.rules.push(rule);
        // Sort by specificity in descending order (more specific first)
        self.rules
            .sort_by(|a, b| b.specificity().cmp(&a.specificity()));
    }

    /// Remove a rule matching the given matcher and action
    pub fn remove_rule(&mut self, matcher: &RuleMatcher, action: &RuleAction) -> bool {
        let initial_len = self.rules.len();
        self.rules
            .retain(|r| &r.matcher != matcher || &r.action != action);
        let removed = self.rules.len() < initial_len;
        if removed {
            tracing::info!("Removed rule: {:?} -> {:?}", matcher, action);
        }
        removed
    }

    /// Get all rules that match a window
    pub fn get_matching_rules(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
    ) -> Vec<&WindowRule> {
        self.rules
            .iter()
            .filter(|rule| rule.matcher.matches(app_name, app_id, title))
            .collect()
    }

    /// Apply matching rules to a window and return effects to execute.
    /// Returns (tags, display_id, position, dimensions, is_floating)
    pub fn apply_rules_to_window(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
    ) -> (
        Option<u32>,
        Option<DisplayId>,
        Option<(i32, i32)>,
        Option<(u32, u32)>,
        bool,
    ) {
        let matching_rules = self.get_matching_rules(app_name, app_id, title);

        let mut tags: Option<u32> = None;
        let mut output: Option<DisplayId> = None;
        let mut position: Option<(i32, i32)> = None;
        let mut dimensions: Option<(u32, u32)> = None;
        let mut is_floating = false;

        // Apply rules in order (most specific first due to sorting)
        for rule in matching_rules {
            match &rule.action {
                RuleAction::Float => {
                    is_floating = true;
                }
                RuleAction::NoFloat => {
                    is_floating = false;
                }
                RuleAction::Tags { tags: t } => {
                    if tags.is_none() {
                        tags = Some(*t);
                    }
                }
                RuleAction::Output { output: o } => {
                    if output.is_none() {
                        // Resolve output specifier to display ID
                        output = self.resolve_output(o);
                    }
                }
                RuleAction::Position { x, y } => {
                    if position.is_none() {
                        position = Some((*x, *y));
                    }
                }
                RuleAction::Dimensions { width, height } => {
                    if dimensions.is_none() {
                        dimensions = Some((*width, *height));
                    }
                }
            }
        }

        (tags, output, position, dimensions, is_floating)
    }

    /// Apply rules to a newly created window.
    /// Modifies the window in place (tags, display_id, is_floating) and returns
    /// Vec<Effect> for position and dimensions to be executed.
    pub fn apply_rules_to_new_window(&mut self, window_id: WindowId) -> Vec<Effect> {
        // Get app_name, app_id, title, and pid from the window
        let (app_name, app_id, title, pid) = {
            let Some(window) = self.windows.get(&window_id) else {
                return vec![];
            };
            (
                window.app_name.clone(),
                window.app_id.clone(),
                window.title.clone(),
                window.pid,
            )
        };

        // Apply rules
        let (tags, output, position, dimensions, is_floating) =
            self.apply_rules_to_window(&app_name, app_id.as_deref(), &title);

        // Modify the window
        if let Some(window) = self.windows.get_mut(&window_id) {
            if let Some(tag_mask) = tags {
                window.tags = Tag::new(tag_mask);
                tracing::info!(
                    "Applied rule: window {} tags set to {}",
                    window_id,
                    tag_mask
                );
            }
            if let Some(display_id) = output {
                window.display_id = display_id;
                tracing::info!(
                    "Applied rule: window {} display set to {}",
                    window_id,
                    display_id
                );
            }
            if is_floating {
                window.is_floating = true;
                tracing::info!("Applied rule: window {} set to floating", window_id);
            }
        }

        // Build effects for position and dimensions
        let mut effects = Vec::new();

        if let Some((x, y)) = position {
            tracing::info!(
                "Rule requires position for window {} (pid {}): ({}, {})",
                window_id,
                pid,
                x,
                y
            );
            effects.push(Effect::MoveWindowToPosition {
                window_id,
                pid,
                x,
                y,
            });
        }

        if let Some((width, height)) = dimensions {
            tracing::info!(
                "Rule requires dimensions for window {} (pid {}): ({}, {})",
                window_id,
                pid,
                width,
                height
            );
            effects.push(Effect::SetWindowDimensions {
                window_id,
                pid,
                width,
                height,
            });
        }

        effects
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::mock::{create_test_display, create_test_window, MockWindowSystem};

    fn setup_mock_system() -> MockWindowSystem {
        MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0),
                create_test_window(101, 1001, "Terminal", 960.0, 0.0, 960.0, 1080.0),
                create_test_window(102, 1002, "VSCode", 0.0, 0.0, 960.0, 540.0),
            ])
            .with_focused(Some(100))
    }

    #[test]
    fn test_sync_all_initializes_state() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        assert_eq!(state.windows.len(), 3);
        assert_eq!(state.displays.len(), 1);
        assert_eq!(state.focused, Some(100));
        assert_eq!(state.focused_display, 1);
    }

    #[test]
    fn test_view_tags_switches_tags() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        // Initial visible tag is 1
        assert_eq!(state.visible_tags().mask(), 0b1);

        // Switch to tag 2 (bitmask 0b10)
        let moves = state.view_tags(0b10);
        assert_eq!(state.visible_tags().mask(), 0b10);

        // All windows should be hidden (moved off-screen)
        assert_eq!(moves.len(), 3);
    }

    #[test]
    fn test_view_tags_last_toggles_back() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        state.view_tags(0b10);
        assert_eq!(state.visible_tags().mask(), 0b10);

        state.view_tags_last();
        assert_eq!(state.visible_tags().mask(), 0b1);
    }

    #[test]
    fn test_toggle_tags() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);
        let display_id = state.focused_display;

        // Toggle tag 2 on (so visible = tag 1 | tag 2)
        state.toggle_tags_on_display(0b10, display_id);
        assert_eq!(state.visible_tags().mask(), 0b11);

        // Toggle tag 1 off (so visible = tag 2 only)
        state.toggle_tags_on_display(0b01, display_id);
        assert_eq!(state.visible_tags().mask(), 0b10);
    }

    #[test]
    fn test_toggle_tags_prevents_empty() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);
        let display_id = state.focused_display;

        // Try to toggle off the only visible tag - should do nothing
        let moves = state.toggle_tags_on_display(0b01, display_id);
        assert_eq!(state.visible_tags().mask(), 0b1);
        assert!(moves.is_empty());
    }

    #[test]
    fn test_move_focused_to_tags() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        // Move focused window (100) to tag 2 (bitmask 0b10)
        let moves = state.move_focused_to_tags(0b10);

        // Window 100 should now have tag 2
        assert_eq!(state.windows.get(&100).unwrap().tags.mask(), 0b10);

        // Window should be hidden (moved off-screen) since tag 2 is not visible
        assert!(!moves.is_empty());
    }

    #[test]
    fn test_focus_window_next() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        // Focus next from window 100
        let result = state.focus_window(Direction::Next);
        assert!(result.is_some());

        let (window_id, _pid) = result.unwrap();
        // Should cycle to next window (sorted by ID)
        assert_eq!(window_id, 101);
    }

    #[test]
    fn test_focus_window_prev() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        // Focus prev from window 100 should wrap around
        let result = state.focus_window(Direction::Prev);
        assert!(result.is_some());

        let (window_id, _pid) = result.unwrap();
        // Should wrap around to last window
        assert_eq!(window_id, 102);
    }

    #[test]
    fn test_focus_window_directional() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        // Focus right from window 100 (at 0,0) should find window 101 (at 960,0)
        let result = state.focus_window(Direction::Right);
        assert!(result.is_some());

        let (window_id, _pid) = result.unwrap();
        assert_eq!(window_id, 101);
    }

    #[test]
    fn test_multi_display_focus_output() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 2000.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        assert_eq!(state.focused_display, 1);

        // Focus next output
        let result = state.focus_output(OutputDirection::Next);
        assert!(result.is_some());
        assert_eq!(state.focused_display, 2);

        // Should return window on display 2
        let (window_id, _) = result.unwrap();
        assert_eq!(window_id, 101);
    }

    #[test]
    fn test_sync_pid_adds_new_windows() {
        let mut ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0,
            )]);

        let mut state = State::new();
        state.sync_all(&ws);
        assert_eq!(state.windows.len(), 1);

        // Add a new window for PID 1000
        ws.add_window(create_test_window(
            101, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
        ));

        let (changed, new_ids) = state.sync_pid(&ws, 1000);
        assert!(changed);
        assert_eq!(state.windows.len(), 2);
        assert_eq!(new_ids, vec![101]);
    }

    #[test]
    fn test_sync_pid_removes_closed_windows() {
        let mut ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0),
                create_test_window(101, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
            ]);

        let mut state = State::new();
        state.sync_all(&ws);
        assert_eq!(state.windows.len(), 2);

        // Remove window 101
        ws.remove_window(101);

        let (changed, new_ids) = state.sync_pid(&ws, 1000);
        assert!(changed);
        assert!(new_ids.is_empty()); // No new windows when removing
        assert_eq!(state.windows.len(), 1);
        assert!(state.windows.contains_key(&100));
        assert!(!state.windows.contains_key(&101));
    }

    #[test]
    fn test_visible_windows_on_display() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        let visible = state.visible_windows_on_display(1);
        assert_eq!(visible.len(), 3);

        // Move one window to tag 2 (bitmask 0b10)
        state.move_focused_to_tags(0b10);

        let visible = state.visible_windows_on_display(1);
        assert_eq!(visible.len(), 2);
    }
}
