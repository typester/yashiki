use serde::{Deserialize, Serialize};

/// Glob pattern for matching strings.
/// Supports: exact match, prefix (*suffix), suffix (prefix*), contains (*middle*)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobPattern(pub String);

impl GlobPattern {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    /// Check if the pattern matches a given string (case-insensitive)
    pub fn matches(&self, s: &str) -> bool {
        let pattern = self.0.to_lowercase();
        let s = s.to_lowercase();

        if !pattern.contains('*') {
            // Exact match
            return pattern == s;
        }

        // Special case: "*" matches everything
        if pattern == "*" {
            return true;
        }

        let starts_with_star = pattern.starts_with('*');
        let ends_with_star = pattern.ends_with('*');

        if starts_with_star && ends_with_star {
            // *middle* - contains
            let middle = &pattern[1..pattern.len() - 1];
            s.contains(middle)
        } else if starts_with_star {
            // *suffix - ends with
            let suffix = &pattern[1..];
            s.ends_with(suffix)
        } else if ends_with_star {
            // prefix* - starts with
            let prefix = &pattern[..pattern.len() - 1];
            s.starts_with(prefix)
        } else {
            // No wildcard (shouldn't reach here but handle gracefully)
            pattern == s
        }
    }

    /// Get the specificity of this pattern. Higher is more specific.
    /// Exact match > prefix/suffix > contains > wildcard only
    pub fn specificity(&self) -> u32 {
        let pattern = &self.0;

        if !pattern.contains('*') {
            // Exact match - highest specificity (length * 4)
            return (pattern.len() as u32) * 4;
        }

        let starts_with_star = pattern.starts_with('*');
        let ends_with_star = pattern.ends_with('*');

        if pattern == "*" {
            // Matches everything - lowest
            return 0;
        }

        if starts_with_star && ends_with_star {
            // *middle* - contains (length * 1)
            let middle_len = pattern.len().saturating_sub(2);
            middle_len as u32
        } else {
            // prefix* or *suffix (length * 2)
            let len = pattern.len().saturating_sub(1);
            (len as u32) * 2
        }
    }

    pub fn pattern(&self) -> &str {
        &self.0
    }
}

/// Matcher for window rules - matches on app_name, app_id, and/or title
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleMatcher {
    /// Pattern to match against app name (e.g., "Safari", "*Chrome*")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<GlobPattern>,
    /// Pattern to match against bundle identifier (e.g., "com.apple.Safari", "com.google.*")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<GlobPattern>,
    /// Pattern to match against window title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<GlobPattern>,
}

impl RuleMatcher {
    pub fn new(app_name: Option<GlobPattern>, title: Option<GlobPattern>) -> Self {
        Self {
            app_name,
            app_id: None,
            title,
        }
    }

    pub fn with_app_id(
        app_name: Option<GlobPattern>,
        app_id: Option<GlobPattern>,
        title: Option<GlobPattern>,
    ) -> Self {
        Self {
            app_name,
            app_id,
            title,
        }
    }

    /// Check if this matcher matches the given app_name, app_id, and title
    pub fn matches(&self, app_name: &str, app_id: Option<&str>, title: &str) -> bool {
        let app_matches = self
            .app_name
            .as_ref()
            .map(|p| p.matches(app_name))
            .unwrap_or(true);
        let app_id_matches = self
            .app_id
            .as_ref()
            .map(|p| app_id.map(|id| p.matches(id)).unwrap_or(false))
            .unwrap_or(true);
        let title_matches = self
            .title
            .as_ref()
            .map(|p| p.matches(title))
            .unwrap_or(true);
        app_matches && app_id_matches && title_matches
    }

    /// Get the combined specificity of this matcher
    pub fn specificity(&self) -> u32 {
        let app_spec = self.app_name.as_ref().map(|p| p.specificity()).unwrap_or(0);
        let app_id_spec = self.app_id.as_ref().map(|p| p.specificity()).unwrap_or(0);
        let title_spec = self.title.as_ref().map(|p| p.specificity()).unwrap_or(0);
        app_spec + app_id_spec + title_spec
    }
}

/// Action to apply when a rule matches
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RuleAction {
    /// Exclude from tiling (floating)
    Float,
    /// Include in tiling (default behavior)
    NoFloat,
    /// Set initial tags (bitmask)
    Tags { tags: u32 },
    /// Set initial display
    Output { output: OutputSpecifier },
    /// Set initial position (for floating windows)
    Position { x: i32, y: i32 },
    /// Set initial dimensions (for floating windows)
    Dimensions { width: u32, height: u32 },
}

/// A window rule: a matcher + action pair
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRule {
    pub matcher: RuleMatcher,
    pub action: RuleAction,
}

impl WindowRule {
    pub fn new(matcher: RuleMatcher, action: RuleAction) -> Self {
        Self { matcher, action }
    }

    pub fn specificity(&self) -> u32 {
        self.matcher.specificity()
    }
}

/// Information about a rule for list-rules output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // Window operations
    WindowFocus {
        direction: Direction,
    },
    WindowSwap {
        direction: Direction,
    },
    WindowClose,
    WindowToggleFloat,
    WindowToggleFullscreen,
    WindowMoveToTag {
        tags: u32,
    },
    WindowToggleTag {
        tags: u32,
    },

    // Tag operations
    TagView {
        tags: u32,
        output: Option<OutputSpecifier>,
    },
    TagToggle {
        tags: u32,
        output: Option<OutputSpecifier>,
    },
    TagViewLast,

    // Output (display) operations
    OutputFocus {
        direction: OutputDirection,
    },
    OutputSend {
        direction: OutputDirection,
    },

    // Layout operations
    LayoutSetDefault {
        layout: String,
    },
    LayoutSet {
        tags: Option<u32>,
        output: Option<OutputSpecifier>,
        layout: String,
    },
    LayoutGet {
        tags: Option<u32>,
        output: Option<OutputSpecifier>,
    },
    LayoutCommand {
        layout: Option<String>,
        cmd: String,
        args: Vec<String>,
    },
    Retile {
        output: Option<OutputSpecifier>,
    },

    // Keybinding operations
    Bind {
        key: String,
        action: Box<Command>,
    },
    Unbind {
        key: String,
    },
    ListBindings,

    // Queries
    ListWindows,
    ListOutputs,
    GetState,
    FocusedWindow,

    // Exec
    Exec {
        command: String,
    },
    ExecOrFocus {
        app_name: String,
        command: String,
    },

    // Exec path
    GetExecPath,
    SetExecPath {
        path: String,
    },
    AddExecPath {
        path: String,
        append: bool,
    },

    // Rules
    RuleAdd {
        rule: WindowRule,
    },
    RuleDel {
        matcher: RuleMatcher,
        action: RuleAction,
    },
    ListRules,
    ApplyRules,

    // Control
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputDirection {
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputSpecifier {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Error { message: String },
    Windows { windows: Vec<WindowInfo> },
    Outputs { outputs: Vec<OutputInfo> },
    State { state: StateInfo },
    Bindings { bindings: Vec<BindingInfo> },
    Rules { rules: Vec<RuleInfo> },
    WindowId { id: Option<u32> },
    Layout { layout: String },
    ExecPath { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingInfo {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_main: bool,
    pub visible_tags: u32,
    pub is_focused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: i32,
    pub title: String,
    pub app_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    pub tags: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_focused: bool,
    pub is_floating: bool,
    pub is_fullscreen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateInfo {
    pub visible_tags: u32,
    pub focused_window_id: Option<u32>,
    pub window_count: usize,
    pub default_layout: String,
    pub current_layout: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_tag_view_serialization() {
        let cmd = Command::TagView {
            tags: 1,
            output: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"tag_view\""));
        assert!(json.contains("\"tags\":1"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::TagView { tags, .. } => assert_eq!(tags, 1),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_window_focus_serialization() {
        let cmd = Command::WindowFocus {
            direction: Direction::Next,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"window_focus\""));
        assert!(json.contains("\"direction\":\"next\""));
    }

    #[test]
    fn test_command_bind_serialization() {
        let cmd = Command::Bind {
            key: "alt-1".to_string(),
            action: Box::new(Command::TagView {
                tags: 1,
                output: None,
            }),
        };
        let json = serde_json::to_string(&cmd).unwrap();

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::Bind { key, action } => {
                assert_eq!(key, "alt-1");
                match *action {
                    Command::TagView { tags, .. } => assert_eq!(tags, 1),
                    _ => panic!("Wrong inner variant"),
                }
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_command_serialization() {
        let cmd = Command::LayoutCommand {
            layout: None,
            cmd: "set-main-ratio".to_string(),
            args: vec!["0.6".to_string()],
        };
        let json = serde_json::to_string(&cmd).unwrap();

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutCommand { layout, cmd, args } => {
                assert_eq!(layout, None);
                assert_eq!(cmd, "set-main-ratio");
                assert_eq!(args, vec!["0.6"]);
            }
            _ => panic!("Wrong variant"),
        }

        // With layout specified
        let cmd = Command::LayoutCommand {
            layout: Some("tatami".to_string()),
            cmd: "set-outer-gap".to_string(),
            args: vec!["10".to_string()],
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"layout\":\"tatami\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutCommand { layout, cmd, args } => {
                assert_eq!(layout, Some("tatami".to_string()));
                assert_eq!(cmd, "set-outer-gap");
                assert_eq!(args, vec!["10"]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_direction_serialization() {
        let cases = [
            (Direction::Left, "\"left\""),
            (Direction::Right, "\"right\""),
            (Direction::Up, "\"up\""),
            (Direction::Down, "\"down\""),
            (Direction::Next, "\"next\""),
            (Direction::Prev, "\"prev\""),
        ];

        for (direction, expected) in cases {
            let json = serde_json::to_string(&direction).unwrap();
            assert_eq!(json, expected);

            let deserialized: Direction = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, direction);
        }
    }

    #[test]
    fn test_output_direction_serialization() {
        let next = OutputDirection::Next;
        let prev = OutputDirection::Prev;

        assert_eq!(serde_json::to_string(&next).unwrap(), "\"next\"");
        assert_eq!(serde_json::to_string(&prev).unwrap(), "\"prev\"");
    }

    #[test]
    fn test_response_ok_serialization() {
        let resp = Response::Ok;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{\"type\":\"ok\"}");

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        matches!(deserialized, Response::Ok);
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = Response::Error {
            message: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Error { message } => assert_eq!(message, "something went wrong"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_windows_serialization() {
        let resp = Response::Windows {
            windows: vec![WindowInfo {
                id: 123,
                pid: 456,
                title: "Test Window".to_string(),
                app_name: "TestApp".to_string(),
                app_id: None,
                tags: 0b0001,
                x: 100,
                y: 200,
                width: 800,
                height: 600,
                is_focused: true,
                is_floating: false,
                is_fullscreen: false,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Windows { windows } => {
                assert_eq!(windows.len(), 1);
                assert_eq!(windows[0].id, 123);
                assert_eq!(windows[0].title, "Test Window");
                assert!(windows[0].is_focused);
                assert!(!windows[0].is_floating);
                assert!(!windows[0].is_fullscreen);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_state_serialization() {
        let resp = Response::State {
            state: StateInfo {
                visible_tags: 0b0011,
                focused_window_id: Some(42),
                window_count: 5,
                default_layout: "tatami".to_string(),
                current_layout: Some("byobu".to_string()),
            },
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::State { state } => {
                assert_eq!(state.visible_tags, 0b0011);
                assert_eq!(state.focused_window_id, Some(42));
                assert_eq!(state.window_count, 5);
                assert_eq!(state.default_layout, "tatami");
                assert_eq!(state.current_layout, Some("byobu".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_set_default_serialization() {
        let cmd = Command::LayoutSetDefault {
            layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"layout_set_default\""));
        assert!(json.contains("\"layout\":\"tatami\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutSetDefault { layout } => assert_eq!(layout, "tatami"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_set_serialization() {
        // Without tags (current tag)
        let cmd = Command::LayoutSet {
            tags: None,
            output: None,
            layout: "byobu".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"layout_set\""));
        assert!(json.contains("\"layout\":\"byobu\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutSet { tags, layout, .. } => {
                assert_eq!(tags, None);
                assert_eq!(layout, "byobu");
            }
            _ => panic!("Wrong variant"),
        }

        // With tags
        let cmd = Command::LayoutSet {
            tags: Some(3),
            output: None,
            layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"tags\":3"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutSet { tags, layout, .. } => {
                assert_eq!(tags, Some(3));
                assert_eq!(layout, "tatami");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_layout_get_serialization() {
        // Without tags (current layout)
        let cmd = Command::LayoutGet {
            tags: None,
            output: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"layout_get\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutGet { tags, .. } => assert_eq!(tags, None),
            _ => panic!("Wrong variant"),
        }

        // With tags
        let cmd = Command::LayoutGet {
            tags: Some(2),
            output: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"tags\":2"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::LayoutGet { tags, .. } => assert_eq!(tags, Some(2)),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_layout_serialization() {
        let resp = Response::Layout {
            layout: "tatami".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"layout\""));
        assert!(json.contains("\"layout\":\"tatami\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Layout { layout } => assert_eq!(layout, "tatami"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_bindings_serialization() {
        let resp = Response::Bindings {
            bindings: vec![BindingInfo {
                key: "alt-1".to_string(),
                action: "tag-view 1".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Bindings { bindings } => {
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].key, "alt-1");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_get_exec_path_serialization() {
        let cmd = Command::GetExecPath;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"get_exec_path\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Command::GetExecPath));
    }

    #[test]
    fn test_command_set_exec_path_serialization() {
        let cmd = Command::SetExecPath {
            path: "/opt/homebrew/bin:/usr/local/bin".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"set_exec_path\""));
        assert!(json.contains("\"path\":\"/opt/homebrew/bin:/usr/local/bin\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::SetExecPath { path } => {
                assert_eq!(path, "/opt/homebrew/bin:/usr/local/bin");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_add_exec_path_serialization() {
        // Prepend (default)
        let cmd = Command::AddExecPath {
            path: "/opt/homebrew/bin".to_string(),
            append: false,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"add_exec_path\""));
        assert!(json.contains("\"append\":false"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::AddExecPath { path, append } => {
                assert_eq!(path, "/opt/homebrew/bin");
                assert!(!append);
            }
            _ => panic!("Wrong variant"),
        }

        // Append
        let cmd = Command::AddExecPath {
            path: "/usr/local/bin".to_string(),
            append: true,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"append\":true"));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::AddExecPath { path, append } => {
                assert_eq!(path, "/usr/local/bin");
                assert!(append);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_exec_path_serialization() {
        let resp = Response::ExecPath {
            path: "/opt/homebrew/bin:/usr/local/bin".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"exec_path\""));
        assert!(json.contains("\"path\":\"/opt/homebrew/bin:/usr/local/bin\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::ExecPath { path } => {
                assert_eq!(path, "/opt/homebrew/bin:/usr/local/bin");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_glob_pattern_exact_match() {
        let pattern = GlobPattern::new("Safari");
        assert!(pattern.matches("Safari"));
        assert!(pattern.matches("safari")); // case insensitive
        assert!(!pattern.matches("Safari Browser"));
        assert!(!pattern.matches("Google Safari"));
    }

    #[test]
    fn test_glob_pattern_prefix() {
        let pattern = GlobPattern::new("Google*");
        assert!(pattern.matches("Google Chrome"));
        assert!(pattern.matches("Google"));
        assert!(!pattern.matches("Not Google Chrome"));
    }

    #[test]
    fn test_glob_pattern_suffix() {
        let pattern = GlobPattern::new("*Editor");
        assert!(pattern.matches("Code Editor"));
        assert!(pattern.matches("Editor"));
        assert!(!pattern.matches("Editor Pro"));
    }

    #[test]
    fn test_glob_pattern_contains() {
        let pattern = GlobPattern::new("*Dialog*");
        assert!(pattern.matches("Save Dialog"));
        assert!(pattern.matches("Dialog Box"));
        assert!(pattern.matches("Open Dialog Window"));
        assert!(!pattern.matches("Diag"));
    }

    #[test]
    fn test_glob_pattern_wildcard_only() {
        let pattern = GlobPattern::new("*");
        assert!(pattern.matches("anything"));
        assert!(pattern.matches(""));
    }

    #[test]
    fn test_glob_pattern_specificity() {
        let exact = GlobPattern::new("Safari");
        let prefix = GlobPattern::new("Safari*");
        let suffix = GlobPattern::new("*Safari");
        let contains = GlobPattern::new("*Safari*");
        let wildcard = GlobPattern::new("*");

        assert!(exact.specificity() > prefix.specificity());
        assert!(prefix.specificity() > contains.specificity());
        assert!(suffix.specificity() > contains.specificity());
        assert!(contains.specificity() > wildcard.specificity());
        assert_eq!(wildcard.specificity(), 0);
    }

    #[test]
    fn test_rule_matcher_app_name_only() {
        let matcher = RuleMatcher::new(Some(GlobPattern::new("Safari")), None);
        assert!(matcher.matches("Safari", None, "Any Title"));
        assert!(matcher.matches("Safari", None, ""));
        assert!(!matcher.matches("Chrome", None, "Any Title"));
    }

    #[test]
    fn test_rule_matcher_title_only() {
        let matcher = RuleMatcher::new(None, Some(GlobPattern::new("*Preferences*")));
        assert!(matcher.matches("Any App", None, "Preferences"));
        assert!(matcher.matches("Safari", None, "Safari Preferences"));
        assert!(!matcher.matches("Safari", None, "Settings"));
    }

    #[test]
    fn test_rule_matcher_both() {
        let matcher = RuleMatcher::new(
            Some(GlobPattern::new("Safari")),
            Some(GlobPattern::new("*Preferences*")),
        );
        assert!(matcher.matches("Safari", None, "Preferences"));
        assert!(matcher.matches("Safari", None, "Safari Preferences"));
        assert!(!matcher.matches("Safari", None, "Main Window"));
        assert!(!matcher.matches("Chrome", None, "Preferences"));
    }

    #[test]
    fn test_rule_matcher_app_id_only() {
        let matcher =
            RuleMatcher::with_app_id(None, Some(GlobPattern::new("com.apple.Safari")), None);
        assert!(matcher.matches("Safari", Some("com.apple.Safari"), "Any Title"));
        assert!(matcher.matches("Any App", Some("com.apple.Safari"), ""));
        assert!(!matcher.matches("Safari", Some("com.google.Chrome"), "Any Title"));
        // app_id pattern requires app_id to be present
        assert!(!matcher.matches("Safari", None, "Any Title"));
    }

    #[test]
    fn test_rule_matcher_app_id_with_wildcard() {
        let matcher = RuleMatcher::with_app_id(None, Some(GlobPattern::new("com.google.*")), None);
        assert!(matcher.matches("Chrome", Some("com.google.Chrome"), "Any Title"));
        assert!(matcher.matches("Meet", Some("com.google.meet"), "Any Title"));
        assert!(!matcher.matches("Safari", Some("com.apple.Safari"), "Any Title"));
    }

    #[test]
    fn test_rule_matcher_app_name_and_app_id() {
        let matcher = RuleMatcher::with_app_id(
            Some(GlobPattern::new("Safari")),
            Some(GlobPattern::new("com.apple.Safari")),
            None,
        );
        assert!(matcher.matches("Safari", Some("com.apple.Safari"), "Any Title"));
        // Both must match
        assert!(!matcher.matches("Safari", Some("com.other.Safari"), "Any Title"));
        assert!(!matcher.matches("Chrome", Some("com.apple.Safari"), "Any Title"));
    }

    #[test]
    fn test_rule_matcher_app_id_specificity() {
        let app_name_only = RuleMatcher::new(Some(GlobPattern::new("Safari")), None);
        let app_id_only =
            RuleMatcher::with_app_id(None, Some(GlobPattern::new("com.apple.Safari")), None);
        let both = RuleMatcher::with_app_id(
            Some(GlobPattern::new("Safari")),
            Some(GlobPattern::new("com.apple.Safari")),
            None,
        );

        // Both app_name and app_id should be more specific than either alone
        assert!(both.specificity() > app_name_only.specificity());
        assert!(both.specificity() > app_id_only.specificity());
    }

    #[test]
    fn test_rule_action_serialization() {
        let cases: Vec<(RuleAction, &str)> = vec![
            (RuleAction::Float, "\"action\":\"float\""),
            (RuleAction::NoFloat, "\"action\":\"no_float\""),
            (RuleAction::Tags { tags: 2 }, "\"action\":\"tags\""),
            (
                RuleAction::Output {
                    output: OutputSpecifier::Id(1),
                },
                "\"action\":\"output\"",
            ),
            (
                RuleAction::Position { x: 100, y: 200 },
                "\"action\":\"position\"",
            ),
            (
                RuleAction::Dimensions {
                    width: 800,
                    height: 600,
                },
                "\"action\":\"dimensions\"",
            ),
        ];

        for (action, expected_pattern) in cases {
            let json = serde_json::to_string(&action).unwrap();
            assert!(
                json.contains(expected_pattern),
                "Expected '{}' in '{}'",
                expected_pattern,
                json
            );
        }
    }

    #[test]
    fn test_window_rule_specificity() {
        let rule1 = WindowRule::new(
            RuleMatcher::new(Some(GlobPattern::new("Safari")), None),
            RuleAction::Float,
        );
        let rule2 = WindowRule::new(
            RuleMatcher::new(
                Some(GlobPattern::new("Safari")),
                Some(GlobPattern::new("*Preferences*")),
            ),
            RuleAction::Float,
        );

        // Rule with both app_name and title should be more specific
        assert!(rule2.specificity() > rule1.specificity());
    }

    #[test]
    fn test_command_rule_add_serialization() {
        let cmd = Command::RuleAdd {
            rule: WindowRule::new(
                RuleMatcher::new(Some(GlobPattern::new("Safari")), None),
                RuleAction::Float,
            ),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"rule_add\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::RuleAdd { rule } => {
                assert!(rule.matcher.app_name.is_some());
                assert!(matches!(rule.action, RuleAction::Float));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_rule_del_serialization() {
        let cmd = Command::RuleDel {
            matcher: RuleMatcher::new(Some(GlobPattern::new("Finder")), None),
            action: RuleAction::Float,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"rule_del\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        match deserialized {
            Command::RuleDel { matcher, action } => {
                assert!(matcher.app_name.is_some());
                assert!(matches!(action, RuleAction::Float));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_command_list_rules_serialization() {
        let cmd = Command::ListRules;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"list_rules\""));

        let deserialized: Command = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Command::ListRules));
    }

    #[test]
    fn test_response_rules_serialization() {
        let resp = Response::Rules {
            rules: vec![RuleInfo {
                app_name: Some("Safari".to_string()),
                app_id: None,
                title: None,
                action: "float".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"rules\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Rules { rules } => {
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0].app_name, Some("Safari".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_rules_with_app_id_serialization() {
        let resp = Response::Rules {
            rules: vec![RuleInfo {
                app_name: None,
                app_id: Some("com.apple.Safari".to_string()),
                title: None,
                action: "float".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"app_id\":\"com.apple.Safari\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Rules { rules } => {
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0].app_id, Some("com.apple.Safari".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_windows_with_app_id() {
        let resp = Response::Windows {
            windows: vec![WindowInfo {
                id: 123,
                pid: 456,
                title: "Test Window".to_string(),
                app_name: "Safari".to_string(),
                app_id: Some("com.apple.Safari".to_string()),
                tags: 0b0001,
                x: 100,
                y: 200,
                width: 800,
                height: 600,
                is_focused: true,
                is_floating: false,
                is_fullscreen: false,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"app_id\":\"com.apple.Safari\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Windows { windows } => {
                assert_eq!(windows.len(), 1);
                assert_eq!(windows[0].app_id, Some("com.apple.Safari".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }
}
