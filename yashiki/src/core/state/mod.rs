use std::collections::HashMap;

use super::{Config, Display, RulesEngine, Tag, Window, WindowId};
use crate::effect::Effect;
use crate::event::Event;
use crate::macos::DisplayId;
use crate::platform::WindowSystem;
use yashiki_ipc::{
    Direction, OutputDirection, OutputSpecifier, RuleAction, RuleMatcher, WindowRule,
};

mod display;
mod focus;
mod layout;
mod rules;
mod sync;
mod tags;

use display::*;
use focus::*;
use layout::*;
use rules::*;
use sync::*;
use tags::*;

/// Result of handling display configuration changes
#[derive(Debug, Default)]
pub struct DisplayChangeResult {
    pub window_moves: Vec<WindowMove>,
    pub displays_to_retile: Vec<DisplayId>,
    pub added: Vec<Display>,
    pub removed: Vec<DisplayId>,
}

/// Result of focus_output operation
#[derive(Debug, Clone, PartialEq)]
pub enum FocusOutputResult {
    Window { window_id: WindowId, pid: i32 },
    EmptyDisplay { display_id: DisplayId },
}

/// Result of send_to_output operation
#[derive(Debug)]
pub struct SendToOutputResult {
    pub source_display_id: DisplayId,
    pub target_display_id: DisplayId,
    pub window_moves: Vec<WindowMove>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowMove {
    pub window_id: WindowId,
    pub pid: i32,
    pub old_x: i32,
    pub old_y: i32,
    pub new_x: i32,
    pub new_y: i32,
}

#[derive(Debug, Clone)]
pub struct TrackedProcess {
    pub pid: u32,
    pub _command: String,
}

pub struct State {
    pub windows: HashMap<WindowId, Window>,
    pub displays: HashMap<DisplayId, Display>,
    pub focused: Option<WindowId>,
    pub focused_display: DisplayId,
    pub(crate) default_tag: Tag,
    pub default_layout: String,
    pub tag_layouts: HashMap<u8, String>,
    pub rules_engine: RulesEngine,
    pub tracked_processes: Vec<TrackedProcess>,
    pub config: Config,
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
            rules_engine: RulesEngine::new(),
            tracked_processes: Vec::new(),
            config: Config::new(),
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
                let first_tag = Tag::from_mask(tags).first_tag().unwrap_or(1);
                tracing::info!("Set layout for tag {}: {}", first_tag, layout);
                self.tag_layouts.insert(first_tag as u8, layout);
            }
            None => {
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

    pub fn has_windows_for_pid(&self, pid: i32) -> bool {
        self.windows.values().any(|w| w.pid == pid)
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

    // Display operations - delegated to state/display.rs

    pub fn handle_display_change<W: WindowSystem>(&mut self, ws: &W) -> DisplayChangeResult {
        handle_display_change(self, ws)
    }

    // Sync operations - delegated to state/sync.rs

    pub fn sync_all<W: WindowSystem>(&mut self, ws: &W) -> Vec<WindowMove> {
        sync_all(self, ws)
    }

    pub fn sync_focused_window<W: WindowSystem>(&mut self, ws: &W) -> (bool, Vec<WindowId>) {
        sync_focused_window(self, ws)
    }

    pub fn sync_focused_window_with_hint<W: WindowSystem>(
        &mut self,
        ws: &W,
        pid_hint: Option<i32>,
    ) -> (bool, Vec<WindowId>) {
        sync_focused_window_with_hint(self, ws, pid_hint)
    }

    pub fn sync_pid<W: WindowSystem>(
        &mut self,
        ws: &W,
        pid: i32,
    ) -> (bool, Vec<WindowId>, Vec<WindowMove>) {
        sync_pid(self, ws, pid)
    }

    pub fn handle_event<W: WindowSystem>(
        &mut self,
        ws: &W,
        event: &Event,
    ) -> (bool, Vec<WindowId>, Vec<WindowMove>) {
        match event {
            Event::WindowCreated { pid } | Event::WindowDestroyed { pid } => {
                self.sync_pid(ws, *pid)
            }
            Event::WindowMoved { pid }
            | Event::WindowResized { pid }
            | Event::WindowMiniaturized { pid }
            | Event::WindowDeminiaturized { pid } => {
                let (changed, _, rehide_moves) = self.sync_pid(ws, *pid);
                (changed, vec![], rehide_moves)
            }
            Event::FocusedWindowChanged => {
                let (changed, new_ids) = self.sync_focused_window(ws);
                (changed, new_ids, vec![])
            }
            Event::ApplicationActivated { pid } => {
                let (changed, new_ids) = self.sync_focused_window_with_hint(ws, Some(*pid));
                (changed, new_ids, vec![])
            }
            Event::ApplicationDeactivated | Event::ApplicationHidden | Event::ApplicationShown => {
                (false, vec![], vec![])
            }
        }
    }

    pub fn set_focused(&mut self, window_id: Option<WindowId>) {
        if self.focused != window_id {
            tracing::info!("Focus changed: {:?} -> {:?}", self.focused, window_id);
            self.focused = window_id;
        }
    }

    // Tag operations - delegated to state/tags.rs

    pub fn view_tags(&mut self, tags: u32) -> Vec<WindowMove> {
        view_tags(self, tags)
    }

    pub fn view_tags_on_display(&mut self, tags: u32, display_id: DisplayId) -> Vec<WindowMove> {
        view_tags_on_display(self, tags, display_id)
    }

    pub fn toggle_tags_on_display(&mut self, tags: u32, display_id: DisplayId) -> Vec<WindowMove> {
        toggle_tags_on_display(self, tags, display_id)
    }

    pub fn view_tags_last(&mut self) -> Vec<WindowMove> {
        view_tags_last(self)
    }

    pub fn move_focused_to_tags(&mut self, tags: u32) -> Vec<WindowMove> {
        move_focused_to_tags(self, tags)
    }

    pub fn toggle_focused_window_tags(&mut self, tags: u32) -> Vec<WindowMove> {
        toggle_focused_window_tags(self, tags)
    }

    pub fn toggle_focused_fullscreen(&mut self) -> Option<(DisplayId, bool, u32, i32)> {
        toggle_focused_fullscreen(self)
    }

    pub fn toggle_focused_float(&mut self) -> Option<(DisplayId, bool, u32, i32)> {
        toggle_focused_float(self)
    }

    // Focus operations - delegated to state/focus.rs

    pub fn focus_window(&self, direction: Direction) -> Option<(WindowId, i32)> {
        focus_window(self, direction)
    }

    pub fn swap_window(&mut self, direction: Direction) -> Option<DisplayId> {
        swap_window(self, direction)
    }

    pub fn focus_output(&mut self, direction: OutputDirection) -> Option<FocusOutputResult> {
        focus_output(self, direction)
    }

    pub fn send_to_output(&mut self, direction: OutputDirection) -> Option<SendToOutputResult> {
        send_to_output(self, direction)
    }

    // Layout operations - delegated to state/layout.rs

    pub fn visible_windows_on_display(&self, display_id: DisplayId) -> Vec<&Window> {
        visible_windows_on_display(self, display_id)
    }

    pub(crate) fn compute_layout_changes(&mut self, display_id: DisplayId) -> Vec<WindowMove> {
        compute_layout_changes(self, display_id)
    }

    // Rule operations - delegated to state/rules.rs

    pub fn add_rule(&mut self, rule: WindowRule) {
        add_rule(self, rule)
    }

    pub fn remove_rule(&mut self, matcher: &RuleMatcher, action: &RuleAction) -> bool {
        remove_rule(self, matcher, action)
    }

    #[cfg(test)]
    pub fn should_ignore_window(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
        ax_id: Option<&str>,
        subrole: Option<&str>,
    ) -> bool {
        should_ignore_window(self, app_name, app_id, title, ax_id, subrole)
    }

    pub fn apply_rules_to_new_window(&mut self, window_id: WindowId) -> Vec<Effect> {
        apply_rules_to_new_window(self, window_id)
    }

    pub fn apply_rules_to_all_windows(&mut self) -> (Vec<DisplayId>, Vec<Effect>, Vec<WindowId>) {
        apply_rules_to_all_windows(self)
    }

    #[cfg(test)]
    pub fn apply_rules_to_window_extended(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
        ext: &yashiki_ipc::ExtendedWindowAttributes,
    ) -> super::RuleApplicationResult {
        apply_rules_to_window_extended(self, app_name, app_id, title, ext)
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
    use crate::platform::mock::{
        create_test_display, create_test_window, create_test_window_with_layer, MockWindowSystem,
    };
    use layout::compute_hide_position_for_display;
    use yashiki_ipc::ExtendedWindowAttributes;

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

        assert_eq!(state.visible_tags().mask(), 0b1);

        let moves = state.view_tags(0b10);
        assert_eq!(state.visible_tags().mask(), 0b10);

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

        state.toggle_tags_on_display(0b10, display_id);
        assert_eq!(state.visible_tags().mask(), 0b11);

        state.toggle_tags_on_display(0b01, display_id);
        assert_eq!(state.visible_tags().mask(), 0b10);
    }

    #[test]
    fn test_toggle_tags_prevents_empty() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);
        let display_id = state.focused_display;

        let moves = state.toggle_tags_on_display(0b01, display_id);
        assert_eq!(state.visible_tags().mask(), 0b1);
        assert!(moves.is_empty());
    }

    #[test]
    fn test_move_focused_to_tags() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        let moves = state.move_focused_to_tags(0b10);

        assert_eq!(state.windows.get(&100).unwrap().tags.mask(), 0b10);

        assert!(!moves.is_empty());
    }

    #[test]
    fn test_focus_window_next() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        let result = state.focus_window(Direction::Next);
        assert!(result.is_some());

        let (window_id, _pid) = result.unwrap();
        assert_eq!(window_id, 101);
    }

    #[test]
    fn test_focus_window_prev() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        let result = state.focus_window(Direction::Prev);
        assert!(result.is_some());

        let (window_id, _pid) = result.unwrap();
        assert_eq!(window_id, 102);
    }

    #[test]
    fn test_focus_window_directional() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

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

        let result = state.focus_output(OutputDirection::Next);
        assert!(result.is_some());
        assert_eq!(state.focused_display, 2);

        match result.unwrap() {
            FocusOutputResult::Window { window_id, .. } => assert_eq!(window_id, 101),
            FocusOutputResult::EmptyDisplay { .. } => panic!("Expected Window result"),
        }
    }

    #[test]
    fn test_focus_output_empty_display() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        assert_eq!(state.focused_display, 1);

        let result = state.focus_output(OutputDirection::Next);
        assert!(result.is_some());
        assert_eq!(state.focused_display, 2);

        match result.unwrap() {
            FocusOutputResult::EmptyDisplay { display_id } => assert_eq!(display_id, 2),
            FocusOutputResult::Window { .. } => panic!("Expected EmptyDisplay result"),
        }
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

        ws.add_window(create_test_window(
            101, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
        ));

        let (changed, new_ids, rehide_moves) = state.sync_pid(&ws, 1000);
        assert!(changed);
        assert_eq!(state.windows.len(), 2);
        assert_eq!(new_ids, vec![101]);
        assert!(rehide_moves.is_empty());
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

        ws.remove_window(101);

        let (changed, new_ids, rehide_moves) = state.sync_pid(&ws, 1000);
        assert!(changed);
        assert!(new_ids.is_empty());
        assert_eq!(state.windows.len(), 1);
        assert!(state.windows.contains_key(&100));
        assert!(!state.windows.contains_key(&101));
        assert!(rehide_moves.is_empty());
    }

    #[test]
    fn test_sync_pid_clears_focused_when_focused_window_removed() {
        let mut ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 0.0, 0.0, 800.0, 600.0),
                create_test_window(101, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
            ]);

        let mut state = State::new();
        state.sync_all(&ws);
        state.focused = Some(101);

        ws.remove_window(101);
        let (changed, _, _) = state.sync_pid(&ws, 1000);

        assert!(changed);
        assert_eq!(state.focused, None);
    }

    #[test]
    fn test_visible_windows_on_display() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        let visible = state.visible_windows_on_display(1);
        assert_eq!(visible.len(), 3);

        state.move_focused_to_tags(0b10);

        let visible = state.visible_windows_on_display(1);
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn test_handle_display_change_display_added() {
        let ws1 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws1);
        assert_eq!(state.displays.len(), 1);

        let ws2 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let result = state.handle_display_change(&ws2);

        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].id, 2);
        assert!(result.removed.is_empty());
        assert_eq!(state.displays.len(), 2);
    }

    #[test]
    fn test_handle_display_change_display_removed() {
        let ws1 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws1);
        assert_eq!(state.displays.len(), 2);

        let ws2 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let result = state.handle_display_change(&ws2);

        assert!(result.added.is_empty());
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0], 2);
        assert_eq!(state.displays.len(), 1);
    }

    #[test]
    fn test_handle_display_change_orphaned_windows() {
        let ws1 = MockWindowSystem::new()
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
        state.sync_all(&ws1);

        assert_eq!(state.windows.get(&101).unwrap().display_id, 2);

        let ws2 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 2000.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let result = state.handle_display_change(&ws2);

        assert_eq!(state.windows.get(&101).unwrap().display_id, 1);

        assert!(result.displays_to_retile.contains(&1));
    }

    #[test]
    fn test_should_ignore_window_with_ignore_rule() {
        use yashiki_ipc::{GlobPattern, RuleAction, RuleMatcher, WindowRule};

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: None,
                app_id: None,
                title: None,
                ax_id: None,
                subrole: Some(GlobPattern::new("AXUnknown")),
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Ignore,
        });

        assert!(state.should_ignore_window("Firefox", None, "Menu", None, Some("AXUnknown")));

        assert!(!state.should_ignore_window(
            "Firefox",
            None,
            "Window",
            None,
            Some("AXStandardWindow")
        ));

        assert!(!state.should_ignore_window("Firefox", None, "Window", None, None));
    }

    #[test]
    fn test_should_ignore_window_with_app_specific_rule() {
        use yashiki_ipc::{GlobPattern, RuleAction, RuleMatcher, WindowRule};

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: None,
                app_id: Some(GlobPattern::new("org.mozilla.firefox")),
                title: None,
                ax_id: None,
                subrole: Some(GlobPattern::new("AXUnknown")),
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Ignore,
        });

        assert!(state.should_ignore_window(
            "Firefox",
            Some("org.mozilla.firefox"),
            "Menu",
            None,
            Some("AXUnknown")
        ));

        assert!(!state.should_ignore_window(
            "Safari",
            Some("com.apple.Safari"),
            "Menu",
            None,
            Some("AXUnknown")
        ));

        assert!(!state.should_ignore_window(
            "Firefox",
            Some("org.mozilla.firefox"),
            "Window",
            None,
            Some("AXStandardWindow")
        ));
    }

    #[test]
    fn test_should_ignore_window_no_rules() {
        let state = State::new();

        assert!(!state.should_ignore_window("Firefox", None, "Window", None, Some("AXUnknown")));
        assert!(!state.should_ignore_window("Safari", None, "Window", None, None));
    }

    #[test]
    fn test_swap_window_next() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        state.focused = Some(100);

        let display = state.displays.get(&1).unwrap();
        let initial_order = display.window_order.clone();

        let result = state.swap_window(Direction::Next);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 1);

        let display = state.displays.get(&1).unwrap();
        let new_order = &display.window_order;

        let old_100_idx = initial_order.iter().position(|&id| id == 100).unwrap();
        let new_100_idx = new_order.iter().position(|&id| id == 100).unwrap();

        assert_ne!(old_100_idx, new_100_idx);
    }

    #[test]
    fn test_swap_window_prev() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        state.focused = Some(101);

        let result = state.swap_window(Direction::Prev);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_swap_window_floating_does_nothing() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        state.windows.get_mut(&100).unwrap().is_floating = true;
        state.focused = Some(100);

        let result = state.swap_window(Direction::Next);
        assert!(result.is_none());
    }

    #[test]
    fn test_swap_window_fullscreen_does_nothing() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        state.windows.get_mut(&100).unwrap().is_fullscreen = true;
        state.focused = Some(100);

        let result = state.swap_window(Direction::Next);
        assert!(result.is_none());
    }

    #[test]
    fn test_swap_window_single_visible_does_nothing() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 0.0, 0.0, 960.0, 1080.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        let result = state.swap_window(Direction::Next);
        assert!(result.is_none());
    }

    #[test]
    fn test_swap_window_directional_right() {
        let ws = setup_mock_system();
        let mut state = State::new();
        state.sync_all(&ws);

        state.focused = Some(100);

        let result = state.swap_window(Direction::Right);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 1);

        let display = state.displays.get(&1).unwrap();
        let idx_100 = display
            .window_order
            .iter()
            .position(|&id| id == 100)
            .unwrap();
        let idx_101 = display
            .window_order
            .iter()
            .position(|&id| id == 101)
            .unwrap();

        assert!(idx_101 < idx_100);
    }

    #[test]
    fn test_float_nofloat_first_match_wins() {
        use yashiki_ipc::{GlobPattern, RuleAction, RuleMatcher, WindowRule};

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: None,
                app_id: Some(GlobPattern::new("org.mozilla.firefox")),
                title: None,
                ax_id: None,
                subrole: Some(GlobPattern::new("AXStandardWindow")),
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::NoFloat,
        });

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: None,
                app_id: Some(GlobPattern::new("org.mozilla.firefox")),
                title: Some(GlobPattern::new("*")),
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Float,
        });

        let ext = ExtendedWindowAttributes {
            ax_id: None,
            subrole: Some("AXStandardWindow".to_string()),
            window_level: 0,
            ..Default::default()
        };
        let result = state.apply_rules_to_window_extended(
            "Firefox",
            Some("org.mozilla.firefox"),
            "Some Title",
            &ext,
        );
        assert_eq!(result.is_floating, Some(false));

        let ext_unknown = ExtendedWindowAttributes {
            ax_id: None,
            subrole: Some("AXUnknown".to_string()),
            window_level: 0,
            ..Default::default()
        };
        let result_unknown = state.apply_rules_to_window_extended(
            "Firefox",
            Some("org.mozilla.firefox"),
            "Dropdown",
            &ext_unknown,
        );
        assert_eq!(result_unknown.is_floating, Some(true));
    }

    #[test]
    fn test_nofloat_rule_sets_floating_false() {
        use yashiki_ipc::{GlobPattern, RuleAction, RuleMatcher, WindowRule};

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new("Terminal")),
                app_id: None,
                title: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::NoFloat,
        });

        let ext = ExtendedWindowAttributes::default();
        let result = state.apply_rules_to_window_extended("Terminal", None, "Window", &ext);

        assert_eq!(result.is_floating, Some(false));
    }

    #[test]
    fn test_no_float_rules_returns_none() {
        let state = State::new();

        let ext = ExtendedWindowAttributes::default();
        let result = state.apply_rules_to_window_extended("Safari", None, "Window", &ext);

        assert_eq!(result.is_floating, None);
    }

    #[test]
    fn test_non_normal_layer_window_not_managed_by_default() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window_with_layer(
                100, 1000, "Raycast", 100.0, 100.0, 800.0, 600.0, 8,
            )]);

        let mut state = State::new();
        state.sync_all(&ws);

        assert!(state.windows.is_empty());
    }

    #[test]
    fn test_non_normal_layer_window_managed_with_float_rule() {
        use yashiki_ipc::GlobPattern;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window_with_layer(
                100, 1000, "Raycast", 100.0, 100.0, 800.0, 600.0, 8,
            )]);

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new("Raycast")),
                app_id: None,
                title: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Float,
        });

        state.sync_all(&ws);
        state.apply_rules_to_new_window(100);

        assert_eq!(state.windows.len(), 1);
        let window = state.windows.get(&100).unwrap();
        assert!(window.is_floating);
    }

    #[test]
    fn test_non_normal_layer_window_managed_with_tags_rule() {
        use yashiki_ipc::GlobPattern;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window_with_layer(
                100, 1000, "Raycast", 100.0, 100.0, 800.0, 600.0, 8,
            )]);

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new("Raycast")),
                app_id: None,
                title: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Tags { tags: 2 },
        });

        state.sync_all(&ws);
        state.apply_rules_to_new_window(100);

        assert_eq!(state.windows.len(), 1);
        let window = state.windows.get(&100).unwrap();
        assert!(window.is_floating);
        assert_eq!(window.tags.mask(), 2);
    }

    #[test]
    fn test_non_normal_layer_window_can_be_tiled_with_no_float() {
        use yashiki_ipc::GlobPattern;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window_with_layer(
                100, 1000, "Raycast", 100.0, 100.0, 800.0, 600.0, 8,
            )]);

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new("Raycast")),
                app_id: None,
                title: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::NoFloat,
        });

        state.sync_all(&ws);
        state.apply_rules_to_new_window(100);

        assert_eq!(state.windows.len(), 1);
        let window = state.windows.get(&100).unwrap();
        assert!(!window.is_floating);
    }

    #[test]
    fn test_normal_layer_window_managed_as_usual() {
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )]);

        let mut state = State::new();
        state.sync_all(&ws);

        assert_eq!(state.windows.len(), 1);
        let window = state.windows.get(&100).unwrap();
        assert!(!window.is_floating);
    }

    #[test]
    fn test_non_normal_layer_window_ignored_with_ignore_rule() {
        use yashiki_ipc::GlobPattern;

        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window_with_layer(
                100, 1000, "Raycast", 100.0, 100.0, 800.0, 600.0, 8,
            )]);

        let mut state = State::new();

        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new("Raycast")),
                app_id: None,
                title: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Float,
        });
        state.add_rule(WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new("Raycast")),
                app_id: None,
                title: None,
                ax_id: None,
                subrole: None,
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Ignore,
        });

        state.sync_all(&ws);

        assert!(state.windows.is_empty());
    }

    #[test]
    fn test_apply_rules_defaults_to_float_for_non_normal_layer() {
        let state = State::new();

        let ext = ExtendedWindowAttributes {
            window_level: 8,
            ..Default::default()
        };
        let result = state.apply_rules_to_window_extended("Raycast", None, "Window", &ext);

        assert_eq!(result.is_floating, Some(true));
    }

    #[test]
    fn test_apply_rules_normal_layer_no_default_float() {
        let state = State::new();

        let ext = ExtendedWindowAttributes {
            window_level: 0,
            ..Default::default()
        };
        let result = state.apply_rules_to_window_extended("Safari", None, "Window", &ext);

        assert_eq!(result.is_floating, None);
    }

    #[test]
    fn test_send_to_output_visible_on_target() {
        // Window tags match target display's visible_tags
        // → Window should stay visible, no hide moves
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        // Both displays show tag 1 (default), window has tag 1
        assert_eq!(state.windows.get(&100).unwrap().tags.mask(), 1);
        assert_eq!(state.displays.get(&1).unwrap().visible_tags.mask(), 1);
        assert_eq!(state.displays.get(&2).unwrap().visible_tags.mask(), 1);

        let result = state.send_to_output(OutputDirection::Next);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.source_display_id, 1);
        assert_eq!(result.target_display_id, 2);
        // Window should stay visible - no hide moves needed
        assert!(result.window_moves.is_empty());

        // Window should be on target display
        assert_eq!(state.windows.get(&100).unwrap().display_id, 2);
        // Window should not be hidden
        assert!(!state.windows.get(&100).unwrap().is_hidden());
    }

    #[test]
    fn test_send_to_output_hidden_on_target() {
        // Window tags don't match target display's visible_tags
        // → Window should be hidden (moves generated)
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        // Move window to tag 2
        state.windows.get_mut(&100).unwrap().tags = Tag::new(2);

        // Display 1 shows tag 2, Display 2 shows tag 1
        state.displays.get_mut(&1).unwrap().visible_tags = Tag::new(2);
        state.displays.get_mut(&2).unwrap().visible_tags = Tag::new(1);

        let result = state.send_to_output(OutputDirection::Next);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.source_display_id, 1);
        assert_eq!(result.target_display_id, 2);
        // Window should be hidden - hide move generated
        assert_eq!(result.window_moves.len(), 1);
        assert_eq!(result.window_moves[0].window_id, 100);

        // Window should be on target display
        assert_eq!(state.windows.get(&100).unwrap().display_id, 2);
        // Window should be hidden (saved_frame is Some)
        assert!(state.windows.get(&100).unwrap().is_hidden());
    }

    #[test]
    fn test_send_to_output_updates_window_order() {
        // Verify window_order is updated on both displays
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

        // Window 100 is on display 1, window 101 is on display 2
        assert!(state.displays.get(&1).unwrap().window_order.contains(&100));
        assert!(state.displays.get(&2).unwrap().window_order.contains(&101));

        let result = state.send_to_output(OutputDirection::Next);
        assert!(result.is_some());

        // Window 100 should be removed from display 1's order and added to display 2's order
        assert!(!state.displays.get(&1).unwrap().window_order.contains(&100));
        assert!(state.displays.get(&2).unwrap().window_order.contains(&100));
    }

    #[test]
    fn test_send_to_output_already_hidden_becomes_visible() {
        // Window is already hidden on source display, becomes visible on target display
        // → Window should be shown with correct target display position
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        // Window has tag 2, display 1 shows tag 1 → window is hidden
        state.windows.get_mut(&100).unwrap().tags = Tag::new(2);
        state.displays.get_mut(&1).unwrap().visible_tags = Tag::new(1);
        state.displays.get_mut(&2).unwrap().visible_tags = Tag::new(2);

        // Hide the window on display 1
        let _hide_moves = state.compute_layout_changes(1);
        assert!(state.windows.get(&100).unwrap().is_hidden());

        // Now send to output - target display shows tag 2, so window should become visible
        let result = state.send_to_output(OutputDirection::Next);
        assert!(result.is_some());

        let result = result.unwrap();
        // Window should be shown - show move generated
        assert_eq!(result.window_moves.len(), 1);
        let show_move = &result.window_moves[0];
        assert_eq!(show_move.window_id, 100);
        // The new position should be on target display (x >= 1920), not source display
        assert!(
            show_move.new_x >= 1920,
            "Window should be shown on target display (x={})",
            show_move.new_x
        );

        // Window should no longer be hidden
        assert!(!state.windows.get(&100).unwrap().is_hidden());
    }

    #[test]
    fn test_per_display_hide_position_single_display() {
        // Single display should use bottom-right corner
        let ws = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![]);

        let mut state = State::new();
        state.sync_all(&ws);

        // Test with 800x600 window
        let (x, y) = compute_hide_position_for_display(&state, 1, 800, 600);
        // Bottom-right corner: (1920-1, 1080-1) = (1919, 1079)
        // Window extends right & down from this point, staying off-screen
        assert_eq!(x, 1919);
        assert_eq!(y, 1079);
    }

    #[test]
    fn test_per_display_hide_position_horizontal_layout() {
        // Horizontal layout: display 1 (left), display 2 (right)
        // Display 1: has right-adjacent display, window at bottom-right would extend into display 2
        // Display 2: no right-adjacent display, bottom-right is safe
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![]);

        let mut state = State::new();
        state.sync_all(&ws);

        // Test with 800x600 window
        // Display 1 (left): has right-adjacent, so bottom-right is unsafe
        // Falls back to bottom-left: (0 - 800 + 1, 1079) = (-799, 1079)
        // Window extends right from -799, so only 1px (at x=0) is visible on display
        let (x1, y1) = compute_hide_position_for_display(&state, 1, 800, 600);
        assert_eq!(x1, -799); // Bottom-left x with offset
        assert_eq!(y1, 1079); // Bottom-left y

        // Display 2 (right): no right-adjacent, bottom-right (3839, 1079) is safe
        let (x2, y2) = compute_hide_position_for_display(&state, 2, 800, 600);
        assert_eq!(x2, 3839); // 1920 + 1920 - 1
        assert_eq!(y2, 1079);
    }

    #[test]
    fn test_per_display_hide_position_vertical_layout() {
        // Vertical layout: display 1 (top), display 2 (bottom)
        // Display 1: has bottom-adjacent display, window at bottom corners would extend into display 2
        // Display 2: no bottom-adjacent display, bottom-right is safe
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 0.0, 1080.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![]);

        let mut state = State::new();
        state.sync_all(&ws);

        // Test with 800x600 window
        // Display 1 (top): has bottom-adjacent, so bottom-right and bottom-left are unsafe
        // Falls back to top-right: (1919, 0 - 600 + 1) = (1919, -599)
        // Window extends down from -599, so only 1px (at y=0) is visible on display
        let (x1, y1) = compute_hide_position_for_display(&state, 1, 800, 600);
        assert_eq!(x1, 1919); // Top-right x
        assert_eq!(y1, -599); // Top-right y with offset

        // Display 2 (bottom): no bottom-adjacent, bottom-right (1919, 2159) is safe
        let (x2, y2) = compute_hide_position_for_display(&state, 2, 800, 600);
        assert_eq!(x2, 1919);
        assert_eq!(y2, 2159); // 1080 + 1080 - 1
    }

    #[test]
    fn test_per_display_hide_position_different_sizes() {
        // Main display (1920x1080), secondary display (1440x900) to the right
        // y ranges overlap (0-1080 vs 0-900), so they are adjacent
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1440.0, 900.0),
            ])
            .with_windows(vec![]);

        let mut state = State::new();
        state.sync_all(&ws);

        // Test with 800x600 window
        // Display 1: has right-adjacent (y ranges 0-1080 and 0-900 overlap)
        // bottom-right is unsafe, falls back to bottom-left: (0 - 800 + 1, 1079) = (-799, 1079)
        let (x1, y1) = compute_hide_position_for_display(&state, 1, 800, 600);
        assert_eq!(x1, -799); // Bottom-left x with offset
        assert_eq!(y1, 1079); // Bottom-left y

        // Display 2: no right-adjacent, bottom-right (3359, 899) is safe
        let (x2, y2) = compute_hide_position_for_display(&state, 2, 800, 600);
        assert_eq!(x2, 3359); // 1920 + 1440 - 1
        assert_eq!(y2, 899);
    }

    #[test]
    fn test_windows_hidden_to_their_own_display() {
        // Windows on different displays should be hidden to their respective display's hide position
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

        // Window 100 is on display 1, window 101 is on display 2
        assert_eq!(state.windows.get(&100).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&101).unwrap().display_id, 2);

        // Switch display 1 to tag 2 (windows have tag 1)
        let moves = state.view_tags_on_display(2, 1);

        // Only window 100 should be hidden (it's on display 1)
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].window_id, 100);
        // Should be hidden to display 1's hide position (-799, 1079) - bottom-left with offset
        // because display 1 has right-adjacent display 2
        // Window is 800x600, so offset x = 0 - 800 + 1 = -799
        assert_eq!(moves[0].new_x, -799);
        assert_eq!(moves[0].new_y, 1079);

        // Switch display 2 to tag 2
        let moves2 = state.view_tags_on_display(2, 2);

        // Window 101 should be hidden to display 2's hide position (3839, 1079)
        assert_eq!(moves2.len(), 1);
        assert_eq!(moves2[0].window_id, 101);
        assert_eq!(moves2[0].new_x, 3839);
        assert_eq!(moves2[0].new_y, 1079);
    }

    #[test]
    fn test_orphan_tracking_on_display_removal() {
        // When a display is removed, windows on that display should be orphaned
        // and their original display should be tracked in orphaned_from
        let ws1 = MockWindowSystem::new()
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
        state.sync_all(&ws1);

        // Window 101 is on display 2
        assert_eq!(state.windows.get(&101).unwrap().display_id, 2);
        assert_eq!(state.windows.get(&101).unwrap().orphaned_from, None);

        // Remove display 2
        let ws2 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 2000.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let _result = state.handle_display_change(&ws2);

        // Window 101 should now be on display 1 (fallback) with orphaned_from = Some(2)
        assert_eq!(state.windows.get(&101).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&101).unwrap().orphaned_from, Some(2));

        // Window 100 should not be affected (was already on display 1)
        assert_eq!(state.windows.get(&100).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&100).unwrap().orphaned_from, None);
    }

    #[test]
    fn test_orphan_restoration_on_display_return() {
        // When a display returns, orphaned windows should be restored to their original display
        let ws1 = MockWindowSystem::new()
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
        state.sync_all(&ws1);

        // Remove display 2
        let ws2 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 100.0, 100.0, 800.0, 600.0), // macOS moves it
            ])
            .with_focused(Some(100));

        let _result = state.handle_display_change(&ws2);
        assert_eq!(state.windows.get(&101).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&101).unwrap().orphaned_from, Some(2));

        // Bring display 2 back
        let ws3 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 100.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let result = state.handle_display_change(&ws3);

        // Window 101 should be restored to display 2
        assert_eq!(state.windows.get(&101).unwrap().display_id, 2);
        assert_eq!(state.windows.get(&101).unwrap().orphaned_from, None);

        // Both displays should be retiled
        assert!(result.displays_to_retile.contains(&1));
        assert!(result.displays_to_retile.contains(&2));
    }

    #[test]
    fn test_orphan_state_cleared_on_intentional_move() {
        // When user intentionally moves a window via send_to_output, orphan state should be cleared
        let ws = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws);

        // Manually set orphaned_from to simulate a previous orphan state
        state.windows.get_mut(&100).unwrap().orphaned_from = Some(2);

        // User sends window to next output
        let result = state.send_to_output(OutputDirection::Next);
        assert!(result.is_some());

        // orphaned_from should be cleared (user intentionally moved it)
        assert_eq!(state.windows.get(&100).unwrap().orphaned_from, None);
        assert_eq!(state.windows.get(&100).unwrap().display_id, 2);
    }

    #[test]
    fn test_rapid_display_reconnect_preserves_window_position() {
        // Simulates sleep/wake: display removed then quickly re-added
        // Window should end up on its original display
        let ws1 = MockWindowSystem::new()
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
        state.sync_all(&ws1);

        // Step 1: Sleep - display 2 disconnects
        let ws2 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                create_test_window(101, 1001, "Terminal", 100.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let _result = state.handle_display_change(&ws2);

        // Verify window 101 is orphaned
        assert_eq!(state.windows.get(&101).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&101).unwrap().orphaned_from, Some(2));

        // Step 2: Wake - display 2 reconnects
        // Note: macOS might have moved the window to display 1 physically,
        // but the orphaned_from tracking should still restore it
        let ws3 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![
                create_test_window(100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0),
                // Window 101 is still physically on display 1 (macOS didn't move it back)
                create_test_window(101, 1001, "Terminal", 100.0, 100.0, 800.0, 600.0),
            ])
            .with_focused(Some(100));

        let result = state.handle_display_change(&ws3);

        // Window 101 should be restored to display 2
        assert_eq!(state.windows.get(&101).unwrap().display_id, 2);
        assert_eq!(state.windows.get(&101).unwrap().orphaned_from, None);

        // Both displays should need retiling
        assert!(result.displays_to_retile.contains(&1));
        assert!(result.displays_to_retile.contains(&2));
    }

    #[test]
    fn test_multi_stage_orphan_preserves_original_display() {
        // Tests that when a window is orphaned multiple times (e.g., multiple displays disconnect),
        // the original orphaned_from value is preserved
        let ws1 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
                create_test_display(3, 3840.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 4000.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let mut state = State::new();
        state.sync_all(&ws1);

        // Window is on display 3
        assert_eq!(state.windows.get(&100).unwrap().display_id, 3);
        assert_eq!(state.windows.get(&100).unwrap().orphaned_from, None);

        // Remove display 3 - window moves to display 2 (or 1 as fallback)
        // For this test, let's assume the main display is display 1
        let ws2 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(2, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let _result = state.handle_display_change(&ws2);

        // Window should be on fallback display (1) with orphaned_from = 3
        assert_eq!(state.windows.get(&100).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&100).unwrap().orphaned_from, Some(3));

        // Now remove display 2 as well - this should NOT change orphaned_from
        // because the window is already orphaned from display 3
        let ws3 = MockWindowSystem::new()
            .with_displays(vec![create_test_display(1, 0.0, 0.0, 1920.0, 1080.0)])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let _result = state.handle_display_change(&ws3);

        // Window is still on display 1, orphaned_from should still be 3 (not 1)
        assert_eq!(state.windows.get(&100).unwrap().display_id, 1);
        assert_eq!(state.windows.get(&100).unwrap().orphaned_from, Some(3));

        // When display 3 comes back, window should be restored
        let ws4 = MockWindowSystem::new()
            .with_displays(vec![
                create_test_display(1, 0.0, 0.0, 1920.0, 1080.0),
                create_test_display(3, 1920.0, 0.0, 1920.0, 1080.0),
            ])
            .with_windows(vec![create_test_window(
                100, 1000, "Safari", 100.0, 100.0, 800.0, 600.0,
            )])
            .with_focused(Some(100));

        let _result = state.handle_display_change(&ws4);

        // Window should be restored to display 3
        assert_eq!(state.windows.get(&100).unwrap().display_id, 3);
        assert_eq!(state.windows.get(&100).unwrap().orphaned_from, None);
    }
}
