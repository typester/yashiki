use yashiki_ipc::{ExtendedWindowAttributes, RuleAction, RuleMatcher, WindowRule};

use crate::macos::DisplayId;

/// Result of applying rules to a window
#[derive(Debug, Default)]
pub struct RuleApplicationResult {
    pub tags: Option<u32>,
    pub display_id: Option<DisplayId>,
    pub position: Option<(i32, i32)>,
    pub dimensions: Option<(u32, u32)>,
    pub is_floating: Option<bool>,
}

/// Engine for managing and matching window rules.
/// Handles rule storage, ordering, and matching logic.
#[derive(Debug, Default)]
pub struct RulesEngine {
    rules: Vec<WindowRule>,
}

impl RulesEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn rules(&self) -> &[WindowRule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn add_rule(&mut self, rule: WindowRule) {
        tracing::info!("Adding rule: {:?} -> {:?}", rule.matcher, rule.action);
        self.rules.push(rule);
        self.rules
            .sort_by_key(|r| std::cmp::Reverse(r.specificity()));
    }

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

    pub fn should_ignore(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
        ext: &ExtendedWindowAttributes,
    ) -> bool {
        self.rules.iter().any(|rule| {
            matches!(rule.action, RuleAction::Ignore)
                && rule.matcher.matches_extended(app_name, app_id, title, ext)
        })
    }

    pub fn has_matching_non_ignore_rule(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
        ext: &ExtendedWindowAttributes,
    ) -> bool {
        self.rules.iter().any(|rule| {
            !matches!(rule.action, RuleAction::Ignore)
                && rule.matcher.matches_extended(app_name, app_id, title, ext)
        })
    }

    pub fn get_matching_rules<'a>(
        &'a self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
        ext: &ExtendedWindowAttributes,
    ) -> Vec<&'a WindowRule> {
        self.rules
            .iter()
            .filter(|rule| rule.matcher.matches_extended(app_name, app_id, title, ext))
            .collect()
    }

    /// Apply matching rules and return the result.
    /// Note: Output resolution must be done by the caller since it requires State access.
    pub fn apply_rules(
        &self,
        app_name: &str,
        app_id: Option<&str>,
        title: &str,
        ext: &ExtendedWindowAttributes,
    ) -> RuleApplicationResult {
        let matching_rules = self.get_matching_rules(app_name, app_id, title, ext);
        let mut result = RuleApplicationResult::default();

        for rule in matching_rules {
            match &rule.action {
                RuleAction::Ignore => {}
                RuleAction::Float => {
                    if result.is_floating.is_none() {
                        result.is_floating = Some(true);
                    }
                }
                RuleAction::NoFloat => {
                    if result.is_floating.is_none() {
                        result.is_floating = Some(false);
                    }
                }
                RuleAction::Tags { tags: t } => {
                    if result.tags.is_none() {
                        result.tags = Some(*t);
                    }
                }
                RuleAction::Output { .. } => {
                    // Output resolution requires State.resolve_output()
                    // This will be handled by the caller
                }
                RuleAction::Position { x, y } => {
                    if result.position.is_none() {
                        result.position = Some((*x, *y));
                    }
                }
                RuleAction::Dimensions { width, height } => {
                    if result.dimensions.is_none() {
                        result.dimensions = Some((*width, *height));
                    }
                }
            }
        }

        // Default to floating for non-normal layer windows
        if result.is_floating.is_none() && ext.window_level != 0 {
            result.is_floating = Some(true);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yashiki_ipc::GlobPattern;

    fn create_ignore_rule(subrole: &str) -> WindowRule {
        WindowRule {
            matcher: RuleMatcher {
                app_name: None,
                app_id: None,
                title: None,
                ax_id: None,
                subrole: Some(GlobPattern::new(subrole)),
                window_level: None,
                close_button: None,
                fullscreen_button: None,
                minimize_button: None,
                zoom_button: None,
            },
            action: RuleAction::Ignore,
        }
    }

    fn create_float_rule(app_name: &str) -> WindowRule {
        WindowRule {
            matcher: RuleMatcher {
                app_name: Some(GlobPattern::new(app_name)),
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
        }
    }

    #[test]
    fn test_add_rule() {
        let mut engine = RulesEngine::new();
        assert!(engine.is_empty());

        engine.add_rule(create_ignore_rule("AXUnknown"));
        assert_eq!(engine.rules().len(), 1);
    }

    #[test]
    fn test_remove_rule() {
        let mut engine = RulesEngine::new();
        let rule = create_ignore_rule("AXUnknown");
        engine.add_rule(rule.clone());

        let removed = engine.remove_rule(&rule.matcher, &rule.action);
        assert!(removed);
        assert!(engine.is_empty());
    }

    #[test]
    fn test_should_ignore() {
        let mut engine = RulesEngine::new();
        engine.add_rule(create_ignore_rule("AXUnknown"));

        let ext = ExtendedWindowAttributes {
            subrole: Some("AXUnknown".to_string()),
            ..Default::default()
        };

        assert!(engine.should_ignore("Firefox", None, "Menu", &ext));

        let ext_standard = ExtendedWindowAttributes {
            subrole: Some("AXStandardWindow".to_string()),
            ..Default::default()
        };

        assert!(!engine.should_ignore("Firefox", None, "Window", &ext_standard));
    }

    #[test]
    fn test_apply_rules_float() {
        let mut engine = RulesEngine::new();
        engine.add_rule(create_float_rule("Finder"));

        let ext = ExtendedWindowAttributes::default();
        let result = engine.apply_rules("Finder", None, "Window", &ext);

        assert_eq!(result.is_floating, Some(true));
    }

    #[test]
    fn test_non_normal_layer_defaults_to_float() {
        let engine = RulesEngine::new();

        let ext = ExtendedWindowAttributes {
            window_level: 8,
            ..Default::default()
        };
        let result = engine.apply_rules("Raycast", None, "Window", &ext);

        assert_eq!(result.is_floating, Some(true));
    }
}
