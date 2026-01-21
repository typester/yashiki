use std::collections::HashSet;

use super::super::{Tag, WindowId};
use crate::effect::Effect;
use crate::macos::DisplayId;
use yashiki_ipc::{ExtendedWindowAttributes, RuleAction, RuleMatcher, WindowRule};

use super::super::state::{RuleApplicationResult, State, WindowMove};

pub fn add_rule(state: &mut State, rule: WindowRule) {
    tracing::info!("Adding rule: {:?} -> {:?}", rule.matcher, rule.action);
    state.rules.push(rule);
    state
        .rules
        .sort_by_key(|r| std::cmp::Reverse(r.specificity()));
}

pub fn remove_rule(state: &mut State, matcher: &RuleMatcher, action: &RuleAction) -> bool {
    let initial_len = state.rules.len();
    state
        .rules
        .retain(|r| &r.matcher != matcher || &r.action != action);
    let removed = state.rules.len() < initial_len;
    if removed {
        tracing::info!("Removed rule: {:?} -> {:?}", matcher, action);
    }
    removed
}

#[cfg(test)]
pub fn should_ignore_window(
    state: &State,
    app_name: &str,
    app_id: Option<&str>,
    title: &str,
    ax_id: Option<&str>,
    subrole: Option<&str>,
) -> bool {
    let ext = ExtendedWindowAttributes {
        ax_id: ax_id.map(|s| s.to_string()),
        subrole: subrole.map(|s| s.to_string()),
        window_level: 0,
        ..Default::default()
    };
    should_ignore_window_extended(state, app_name, app_id, title, &ext)
}

pub fn should_ignore_window_extended(
    state: &State,
    app_name: &str,
    app_id: Option<&str>,
    title: &str,
    ext: &ExtendedWindowAttributes,
) -> bool {
    state.rules.iter().any(|rule| {
        matches!(rule.action, RuleAction::Ignore)
            && rule.matcher.matches_extended(app_name, app_id, title, ext)
    })
}

pub fn has_matching_non_ignore_rule(
    state: &State,
    app_name: &str,
    app_id: Option<&str>,
    title: &str,
    ext: &ExtendedWindowAttributes,
) -> bool {
    state.rules.iter().any(|rule| {
        !matches!(rule.action, RuleAction::Ignore)
            && rule.matcher.matches_extended(app_name, app_id, title, ext)
    })
}

pub fn get_matching_rules_extended<'a>(
    state: &'a State,
    app_name: &str,
    app_id: Option<&str>,
    title: &str,
    ext: &ExtendedWindowAttributes,
) -> Vec<&'a WindowRule> {
    state
        .rules
        .iter()
        .filter(|rule| rule.matcher.matches_extended(app_name, app_id, title, ext))
        .collect()
}

pub fn apply_rules_to_window_extended(
    state: &State,
    app_name: &str,
    app_id: Option<&str>,
    title: &str,
    ext: &ExtendedWindowAttributes,
) -> RuleApplicationResult {
    let matching_rules = get_matching_rules_extended(state, app_name, app_id, title, ext);
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
            RuleAction::Output { output: o } => {
                if result.display_id.is_none() {
                    result.display_id = state.resolve_output(o);
                }
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

    if result.is_floating.is_none() && ext.window_level != 0 {
        result.is_floating = Some(true);
    }

    result
}

pub fn apply_rules_to_new_window(state: &mut State, window_id: WindowId) -> Vec<Effect> {
    let (app_name, app_id, title, ext, pid) = {
        let Some(window) = state.windows.get(&window_id) else {
            return vec![];
        };
        (
            window.app_name.clone(),
            window.app_id.clone(),
            window.title.clone(),
            window.extended_attributes(),
            window.pid,
        )
    };

    let rule_result =
        apply_rules_to_window_extended(state, &app_name, app_id.as_deref(), &title, &ext);

    if let Some(window) = state.windows.get_mut(&window_id) {
        if let Some(tag_mask) = rule_result.tags {
            window.tags = Tag::from_mask(tag_mask);
            tracing::info!(
                "Applied rule: window {} tags set to {}",
                window_id,
                tag_mask
            );
        }
        if let Some(display_id) = rule_result.display_id {
            window.display_id = display_id;
            tracing::info!(
                "Applied rule: window {} display set to {}",
                window_id,
                display_id
            );
        }
        if let Some(floating) = rule_result.is_floating {
            window.is_floating = floating;
            tracing::info!(
                "Applied rule: window {} set to floating={}",
                window_id,
                floating
            );
        }
    }

    let mut effects = Vec::new();

    if let Some((x, y)) = rule_result.position {
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

    if let Some((width, height)) = rule_result.dimensions {
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

    let hide_move = compute_hide_for_window(state, window_id);
    if let Some(window_move) = hide_move {
        effects.push(Effect::ApplyWindowMoves(vec![window_move]));
    }

    effects
}

fn compute_hide_for_window(state: &mut State, window_id: WindowId) -> Option<WindowMove> {
    let (display_id, window_tags, window_frame, window_pid, is_already_hidden) = {
        let window = state.windows.get(&window_id)?;
        (
            window.display_id,
            window.tags,
            window.frame,
            window.pid,
            window.is_hidden(),
        )
    };

    if is_already_hidden {
        return None;
    }

    let visible_tags = state.displays.get(&display_id)?.visible_tags;
    let (hide_x, hide_y) = super::layout::compute_global_hide_position(state);

    let should_be_visible = window_tags.intersects(visible_tags);

    if !should_be_visible {
        tracing::info!(
            "Hiding window {} (tags {} don't match visible {})",
            window_id,
            window_tags.mask(),
            visible_tags.mask()
        );

        if let Some(window) = state.windows.get_mut(&window_id) {
            window.saved_frame = Some(window.frame);
            window.frame.x = hide_x;
            window.frame.y = hide_y;
        }

        Some(WindowMove {
            window_id,
            pid: window_pid,
            old_x: window_frame.x,
            old_y: window_frame.y,
            new_x: hide_x,
            new_y: hide_y,
        })
    } else {
        None
    }
}

pub fn apply_rules_to_all_windows(
    state: &mut State,
) -> (Vec<DisplayId>, Vec<Effect>, Vec<WindowId>) {
    if state.rules.is_empty() {
        return (vec![], vec![], vec![]);
    }

    let mut affected_displays = HashSet::new();
    let mut effects = Vec::new();
    let mut removed_window_ids = Vec::new();

    let window_ids: Vec<WindowId> = state.windows.keys().copied().collect();

    let windows_to_remove: Vec<(WindowId, DisplayId)> = window_ids
        .iter()
        .filter_map(|&id| {
            let window = state.windows.get(&id)?;
            let ext = window.extended_attributes();
            if should_ignore_window_extended(
                state,
                &window.app_name,
                window.app_id.as_deref(),
                &window.title,
                &ext,
            ) {
                Some((id, window.display_id))
            } else {
                None
            }
        })
        .collect();

    for (window_id, display_id) in &windows_to_remove {
        if let Some(window) = state.windows.remove(window_id) {
            tracing::info!(
                "Removed window {} ({}) due to ignore rule",
                window_id,
                window.app_name
            );
            affected_displays.insert(*display_id);
            removed_window_ids.push(*window_id);

            if state.focused == Some(*window_id) {
                state.focused = None;
            }
        }
    }

    let window_ids: Vec<WindowId> = window_ids
        .into_iter()
        .filter(|id| !windows_to_remove.iter().any(|(rid, _)| rid == id))
        .collect();

    for window_id in window_ids {
        let (app_name, app_id, title, ext, pid, original_tags, original_display_id) = {
            let Some(window) = state.windows.get(&window_id) else {
                continue;
            };
            (
                window.app_name.clone(),
                window.app_id.clone(),
                window.title.clone(),
                window.extended_attributes(),
                window.pid,
                window.tags,
                window.display_id,
            )
        };

        let rule_result =
            apply_rules_to_window_extended(state, &app_name, app_id.as_deref(), &title, &ext);

        let new_tags = rule_result.tags.map(Tag::from_mask);
        let new_display_id = rule_result.display_id;

        let tags_changed = new_tags.is_some() && new_tags != Some(original_tags);
        let display_changed =
            new_display_id.is_some() && new_display_id != Some(original_display_id);

        if let Some(window) = state.windows.get_mut(&window_id) {
            if let Some(tag_mask) = rule_result.tags {
                window.tags = Tag::from_mask(tag_mask);
                tracing::info!(
                    "Applied rule: window {} ({}) tags set to {}",
                    window_id,
                    app_name,
                    tag_mask
                );
            }
            if let Some(display_id) = rule_result.display_id {
                window.display_id = display_id;
                tracing::info!(
                    "Applied rule: window {} ({}) display set to {}",
                    window_id,
                    app_name,
                    display_id
                );
            }
            if let Some(floating) = rule_result.is_floating {
                window.is_floating = floating;
                tracing::info!(
                    "Applied rule: window {} ({}) set to floating={}",
                    window_id,
                    app_name,
                    floating
                );
            }
        }

        if tags_changed || display_changed {
            affected_displays.insert(original_display_id);
            if let Some(new_disp) = new_display_id {
                affected_displays.insert(new_disp);
            }
        }

        if let Some((x, y)) = rule_result.position {
            effects.push(Effect::MoveWindowToPosition {
                window_id,
                pid,
                x,
                y,
            });
        }

        if let Some((width, height)) = rule_result.dimensions {
            effects.push(Effect::SetWindowDimensions {
                window_id,
                pid,
                width,
                height,
            });
        }
    }

    let display_ids: Vec<_> = affected_displays.into_iter().collect();
    (display_ids, effects, removed_window_ids)
}
