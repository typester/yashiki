use std::cell::RefCell;

use crate::core::{FocusOutputResult, State};
use crate::effect::{CommandResult, Effect};
use crate::macos::HotkeyManager;
use crate::platform::WindowSystem;
use yashiki_ipc::{
    BindingInfo, ButtonState, Command, OuterGap, OutputInfo, Response, RuleInfo, StateInfo,
    WindowInfo, WindowLevel, WindowLevelName, WindowLevelOther, WindowStatus,
};

fn apply_rules_effects(state: &mut State) -> Vec<Effect> {
    let (affected_displays, mut effects, _removed_window_ids) = state.apply_rules_to_all_windows();

    let mut all_moves = Vec::new();
    for display_id in &affected_displays {
        let moves = state.compute_layout_changes(*display_id);
        all_moves.extend(moves);
    }

    if !all_moves.is_empty() {
        effects.insert(0, Effect::ApplyWindowMoves(all_moves));
    }

    if !affected_displays.is_empty() {
        effects.push(Effect::RetileDisplays(affected_displays));
    }

    effects
}

/// Pure function: processes a command and returns a response with effects.
/// This function does not perform any side effects - it only mutates state and computes effects.
pub fn process_command(
    state: &mut State,
    hotkey_manager: &mut HotkeyManager,
    cmd: &Command,
) -> CommandResult {
    match cmd {
        // Query commands - no effects
        Command::ListWindows { all, debug } => {
            // For all=true, we need system access - handled specially in handle_ipc_command
            // For all=false, we can use state data only
            if *all {
                // Return a marker response; handle_ipc_command will intercept this
                CommandResult::with_response(Response::Windows { windows: vec![] })
            } else {
                let windows: Vec<WindowInfo> = state
                    .windows
                    .values()
                    .map(|w| WindowInfo {
                        id: w.id,
                        pid: w.pid,
                        title: w.title.clone(),
                        app_name: w.app_name.clone(),
                        app_id: w.app_id.clone(),
                        tags: w.tags.mask(),
                        x: w.frame.x,
                        y: w.frame.y,
                        width: w.frame.width,
                        height: w.frame.height,
                        is_focused: state.focused == Some(w.id),
                        is_floating: w.is_floating,
                        is_fullscreen: w.is_fullscreen,
                        status: None,
                        ax_id: if *debug { w.ax_id.clone() } else { None },
                        subrole: if *debug { w.subrole.clone() } else { None },
                        window_level: if *debug { Some(w.window_level) } else { None },
                        close_button: if *debug {
                            Some(w.close_button.clone())
                        } else {
                            None
                        },
                        fullscreen_button: if *debug {
                            Some(w.fullscreen_button.clone())
                        } else {
                            None
                        },
                        minimize_button: if *debug {
                            Some(w.minimize_button.clone())
                        } else {
                            None
                        },
                        zoom_button: if *debug {
                            Some(w.zoom_button.clone())
                        } else {
                            None
                        },
                    })
                    .collect();
                CommandResult::with_response(Response::Windows { windows })
            }
        }
        Command::ListOutputs => {
            let outputs: Vec<OutputInfo> = state
                .displays
                .values()
                .map(|d| OutputInfo {
                    id: d.id,
                    name: d.name.clone(),
                    x: d.frame.x,
                    y: d.frame.y,
                    width: d.frame.width,
                    height: d.frame.height,
                    is_main: d.is_main,
                    visible_tags: d.visible_tags.mask(),
                    is_focused: state.focused_display == d.id,
                })
                .collect();
            CommandResult::with_response(Response::Outputs { outputs })
        }
        Command::GetState => CommandResult::with_response(Response::State {
            state: StateInfo {
                visible_tags: state.visible_tags().mask(),
                focused_window_id: state.focused,
                window_count: state.windows.len(),
                default_layout: state.default_layout.clone(),
                current_layout: state
                    .displays
                    .get(&state.focused_display)
                    .and_then(|d| d.current_layout.clone()),
            },
        }),
        Command::FocusedWindow => {
            CommandResult::with_response(Response::WindowId { id: state.focused })
        }
        Command::ListBindings => {
            let bindings: Vec<BindingInfo> = hotkey_manager
                .list_bindings()
                .into_iter()
                .map(|(key, cmd)| BindingInfo {
                    key,
                    action: format!("{:?}", cmd),
                })
                .collect();
            CommandResult::with_response(Response::Bindings { bindings })
        }

        // Tag operations - mutate state, return effects
        Command::TagView { tags, output } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => id,
                Err(e) => return CommandResult::error(e),
            };
            let moves = state.view_tags_on_display(*tags, display_id);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::RetileDisplays(vec![display_id]),
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::TagToggle { tags, output } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => id,
                Err(e) => return CommandResult::error(e),
            };
            let moves = state.toggle_tags_on_display(*tags, display_id);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::RetileDisplays(vec![display_id]),
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::TagViewLast => {
            let moves = state.view_tags_last();
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::Retile,
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::WindowMoveToTag { tags } => {
            let moves = state.move_focused_to_tags(*tags);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::Retile,
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }
        Command::WindowToggleTag { tags } => {
            let moves = state.toggle_focused_window_tags(*tags);
            CommandResult::ok_with_effects(vec![
                Effect::ApplyWindowMoves(moves),
                Effect::Retile,
                Effect::FocusVisibleWindowIfNeeded,
            ])
        }

        // Hotkey operations
        Command::Bind { key, action } => match hotkey_manager.bind(key, *action.clone()) {
            Ok(()) => CommandResult::ok(),
            Err(e) => CommandResult::error(e),
        },
        Command::Unbind { key } => match hotkey_manager.unbind(key) {
            Ok(()) => CommandResult::ok(),
            Err(e) => CommandResult::error(e),
        },

        // Focus operations
        Command::WindowFocus { direction } => {
            if let Some((window_id, pid)) = state.focus_window(*direction) {
                tracing::info!("Focusing window {} (pid {})", window_id, pid);
                CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                    window_id,
                    pid,
                    is_output_change: false,
                }])
            } else {
                CommandResult::ok()
            }
        }
        Command::WindowSwap { direction } => {
            if let Some(display_id) = state.swap_window(*direction) {
                CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
            } else {
                CommandResult::ok()
            }
        }
        Command::OutputFocus { direction } => match state.focus_output(*direction) {
            Some(FocusOutputResult::Window { window_id, pid }) => {
                tracing::info!("Focusing output - window {} (pid {})", window_id, pid);
                CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                    window_id,
                    pid,
                    is_output_change: true,
                }])
            }
            Some(FocusOutputResult::EmptyDisplay { display_id }) => {
                tracing::info!("Focusing output - empty display {}", display_id);
                CommandResult::ok_with_effects(vec![Effect::WarpCursorToDisplay { display_id }])
            }
            None => CommandResult::ok(),
        },

        // Fullscreen toggle
        Command::WindowToggleFullscreen => {
            if let Some((display_id, is_fullscreen, window_id, pid)) =
                state.toggle_focused_fullscreen()
            {
                if is_fullscreen {
                    // Going fullscreen - apply fullscreen geometry
                    CommandResult::ok_with_effects(vec![
                        Effect::ApplyFullscreen {
                            window_id,
                            pid,
                            display_id,
                        },
                        Effect::RetileDisplays(vec![display_id]),
                    ])
                } else {
                    // Exiting fullscreen - just retile (layout will recompute position)
                    CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
                }
            } else {
                CommandResult::ok()
            }
        }

        // Float toggle
        Command::WindowToggleFloat => {
            if let Some((display_id, _is_floating, _window_id, _pid)) = state.toggle_focused_float()
            {
                // Just retile - window maintains current position when floating,
                // or gets positioned by layout when unfloating
                CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
            } else {
                CommandResult::ok()
            }
        }

        // Window close
        Command::WindowClose => {
            if let Some(focused_id) = state.focused {
                if let Some(window) = state.windows.get(&focused_id) {
                    CommandResult::ok_with_effects(vec![Effect::CloseWindow {
                        window_id: focused_id,
                        pid: window.pid,
                    }])
                } else {
                    CommandResult::error("Focused window not found")
                }
            } else {
                CommandResult::error("No focused window")
            }
        }

        // Send to output - returns displays that need retiling
        Command::OutputSend { direction } => {
            let displays_to_retile = state.send_to_output(*direction);
            if let Some((source_display, target_display)) = displays_to_retile {
                // Get the window info for moving
                let mut effects = Vec::new();
                if let Some(focused_id) = state.focused {
                    if let Some(window) = state.windows.get(&focused_id) {
                        effects.push(Effect::MoveWindowToPosition {
                            window_id: focused_id,
                            pid: window.pid,
                            x: window.frame.x,
                            y: window.frame.y,
                        });
                    }
                }
                effects.push(Effect::RetileDisplays(vec![source_display, target_display]));
                CommandResult::ok_with_effects(effects)
            } else {
                CommandResult::ok()
            }
        }

        // Layout configuration
        Command::LayoutSetDefault { layout } => {
            state.set_default_layout(layout.clone());
            CommandResult::ok()
        }
        Command::LayoutSet {
            tags,
            output,
            layout,
        } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => Some(id),
                Err(e) => return CommandResult::error(e),
            };
            state.set_layout_on_display(*tags, display_id, layout.clone());
            // Only retile if setting for current tag (no tags specified)
            if tags.is_none() {
                if let Some(id) = display_id {
                    CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![id])])
                } else {
                    CommandResult::ok_with_effects(vec![Effect::Retile])
                }
            } else {
                CommandResult::ok()
            }
        }
        Command::LayoutGet { tags, output } => {
            let display_id = match state.get_target_display(output.as_ref()) {
                Ok(id) => Some(id),
                Err(e) => return CommandResult::error(e),
            };
            let layout = state.get_layout_on_display(*tags, display_id).to_string();
            CommandResult::with_response(Response::Layout { layout })
        }

        // Layout commands - need layout engine interaction (handled as effects)
        Command::LayoutCommand { layout, cmd, args } => {
            let mut effects = vec![Effect::SendLayoutCommand {
                layout: layout.clone(),
                cmd: cmd.clone(),
                args: args.clone(),
            }];
            // Only retile if targeting current layout (layout is None)
            if layout.is_none() {
                effects.push(Effect::Retile);
            }
            CommandResult::ok_with_effects(effects)
        }
        Command::Retile { output } => {
            if let Some(ref spec) = output {
                let display_id = match state.get_target_display(Some(spec)) {
                    Ok(id) => id,
                    Err(e) => return CommandResult::error(e),
                };
                CommandResult::ok_with_effects(vec![Effect::RetileDisplays(vec![display_id])])
            } else {
                CommandResult::ok_with_effects(vec![Effect::Retile])
            }
        }

        // Exec path commands
        Command::GetExecPath => CommandResult::with_response(Response::ExecPath {
            path: state.exec_path.clone(),
        }),
        Command::SetExecPath { path } => {
            tracing::info!("Set exec path: {}", path);
            state.exec_path = path.clone();
            CommandResult::ok_with_effects(vec![Effect::UpdateLayoutExecPath {
                path: path.clone(),
            }])
        }
        Command::AddExecPath { path, append } => {
            let new_exec_path = if *append {
                if state.exec_path.is_empty() {
                    path.clone()
                } else {
                    format!("{}:{}", state.exec_path, path)
                }
            } else if state.exec_path.is_empty() {
                path.clone()
            } else {
                format!("{}:{}", path, state.exec_path)
            };
            tracing::info!("Add exec path: {} (append={})", path, append);
            state.exec_path = new_exec_path.clone();
            CommandResult::ok_with_effects(vec![Effect::UpdateLayoutExecPath {
                path: new_exec_path,
            }])
        }

        // Exec commands
        Command::Exec { command, track } => {
            if *track {
                CommandResult::ok_with_effects(vec![Effect::ExecCommandTracked {
                    command: command.clone(),
                    path: state.exec_path.clone(),
                }])
            } else {
                CommandResult::ok_with_effects(vec![Effect::ExecCommand {
                    command: command.clone(),
                    path: state.exec_path.clone(),
                }])
            }
        }
        Command::ExecOrFocus { app_name, command } => {
            // Check if a window with the given app_name exists
            let existing_window = state
                .windows
                .values()
                .find(|w| w.app_name == *app_name)
                .map(|w| (w.id, w.pid, w.tags, w.display_id, w.is_hidden()));

            if let Some((window_id, pid, window_tags, window_display_id, is_hidden)) =
                existing_window
            {
                // Check if window is visible on its display
                let is_visible = state
                    .displays
                    .get(&window_display_id)
                    .map(|display| window_tags.intersects(display.visible_tags) && !is_hidden)
                    .unwrap_or(false);

                if is_visible {
                    tracing::info!(
                        "Focusing visible window for app '{}' (window_id={}, pid={})",
                        app_name,
                        window_id,
                        pid
                    );
                    CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                        window_id,
                        pid,
                        is_output_change: false,
                    }])
                } else {
                    // Window is hidden, switch to its tag first
                    if let Some(tag) = window_tags.first_tag() {
                        tracing::info!(
                            "Switching to tag {} and focusing window for app '{}' (window_id={}, pid={})",
                            tag,
                            app_name,
                            window_id,
                            pid
                        );
                        let moves = state.view_tags(1 << (tag - 1));
                        CommandResult::ok_with_effects(vec![
                            Effect::ApplyWindowMoves(moves),
                            Effect::Retile,
                            Effect::FocusWindow {
                                window_id,
                                pid,
                                is_output_change: false,
                            },
                        ])
                    } else {
                        CommandResult::ok_with_effects(vec![Effect::FocusWindow {
                            window_id,
                            pid,
                            is_output_change: false,
                        }])
                    }
                }
            } else {
                tracing::info!(
                    "No existing window for app '{}', executing command",
                    app_name
                );
                CommandResult::ok_with_effects(vec![Effect::ExecCommand {
                    command: command.clone(),
                    path: state.exec_path.clone(),
                }])
            }
        }

        // Rules
        Command::RuleAdd { rule } => {
            state.add_rule(rule.clone());

            if state.init_completed {
                CommandResult::ok_with_effects(apply_rules_effects(state))
            } else {
                CommandResult::ok()
            }
        }
        Command::RuleDel { matcher, action } => {
            if state.remove_rule(matcher, action) {
                CommandResult::ok()
            } else {
                CommandResult::error("Rule not found")
            }
        }
        Command::ListRules => {
            let rules: Vec<RuleInfo> = state
                .rules
                .iter()
                .map(|r| {
                    let action_str = match &r.action {
                        yashiki_ipc::RuleAction::Ignore => "ignore".to_string(),
                        yashiki_ipc::RuleAction::Float => "float".to_string(),
                        yashiki_ipc::RuleAction::NoFloat => "no-float".to_string(),
                        yashiki_ipc::RuleAction::Tags { tags } => format!("tags {}", tags),
                        yashiki_ipc::RuleAction::Output { output } => match output {
                            yashiki_ipc::OutputSpecifier::Id(id) => format!("output {}", id),
                            yashiki_ipc::OutputSpecifier::Name(name) => {
                                format!("output {}", name)
                            }
                        },
                        yashiki_ipc::RuleAction::Position { x, y } => {
                            format!("position {} {}", x, y)
                        }
                        yashiki_ipc::RuleAction::Dimensions { width, height } => {
                            format!("dimensions {} {}", width, height)
                        }
                    };
                    RuleInfo {
                        app_name: r.matcher.app_name.as_ref().map(|p| p.pattern().to_string()),
                        app_id: r.matcher.app_id.as_ref().map(|p| p.pattern().to_string()),
                        title: r.matcher.title.as_ref().map(|p| p.pattern().to_string()),
                        ax_id: r.matcher.ax_id.as_ref().map(|p| p.pattern().to_string()),
                        subrole: r.matcher.subrole.as_ref().map(|p| p.pattern().to_string()),
                        window_level: r.matcher.window_level.as_ref().map(format_window_level),
                        close_button: r.matcher.close_button.map(format_button_state),
                        fullscreen_button: r.matcher.fullscreen_button.map(format_button_state),
                        minimize_button: r.matcher.minimize_button.map(format_button_state),
                        zoom_button: r.matcher.zoom_button.map(format_button_state),
                        action: action_str,
                    }
                })
                .collect();
            CommandResult::with_response(Response::Rules { rules })
        }
        Command::ApplyRules => {
            state.init_completed = true;
            tracing::info!("Applied rules to all existing windows");
            CommandResult::ok_with_effects(apply_rules_effects(state))
        }

        // Cursor warp
        Command::SetCursorWarp { mode } => {
            tracing::info!("Set cursor warp mode: {:?}", mode);
            state.cursor_warp = *mode;
            CommandResult::ok()
        }
        Command::GetCursorWarp => CommandResult::with_response(Response::CursorWarp {
            mode: state.cursor_warp,
        }),

        // Outer gap
        Command::SetOuterGap { values } => match OuterGap::from_args(values) {
            Some(gap) => {
                tracing::info!("Set outer gap: {}", gap);
                state.outer_gap = gap;
                CommandResult::ok_with_effects(vec![Effect::Retile])
            }
            None => CommandResult::error("usage: set-outer-gap <all> | <v h> | <t r b l>"),
        },
        Command::GetOuterGap => CommandResult::with_response(Response::OuterGap {
            outer_gap: state.outer_gap,
        }),

        // Control
        Command::Quit => {
            tracing::info!("Quit command received");
            CommandResult::ok()
        }
    }
}

/// List all system windows (managed and ignored) for --all option
pub fn list_all_windows<S: WindowSystem>(
    state: &RefCell<State>,
    window_system: &S,
    debug: bool,
) -> Response {
    let state = state.borrow();
    let system_windows = window_system.get_on_screen_windows();

    let mut windows: Vec<WindowInfo> = Vec::new();

    for sys_win in &system_windows {
        // Check if this window is managed (in state)
        if let Some(w) = state.windows.get(&sys_win.window_id) {
            // Managed window - use state data
            windows.push(WindowInfo {
                id: w.id,
                pid: w.pid,
                title: w.title.clone(),
                app_name: w.app_name.clone(),
                app_id: w.app_id.clone(),
                tags: w.tags.mask(),
                x: w.frame.x,
                y: w.frame.y,
                width: w.frame.width,
                height: w.frame.height,
                is_focused: state.focused == Some(w.id),
                is_floating: w.is_floating,
                is_fullscreen: w.is_fullscreen,
                status: Some(WindowStatus::Managed),
                ax_id: if debug { w.ax_id.clone() } else { None },
                subrole: if debug { w.subrole.clone() } else { None },
                window_level: if debug { Some(w.window_level) } else { None },
                close_button: if debug {
                    Some(w.close_button.clone())
                } else {
                    None
                },
                fullscreen_button: if debug {
                    Some(w.fullscreen_button.clone())
                } else {
                    None
                },
                minimize_button: if debug {
                    Some(w.minimize_button.clone())
                } else {
                    None
                },
                zoom_button: if debug {
                    Some(w.zoom_button.clone())
                } else {
                    None
                },
            });
        } else {
            // Ignored window - use system window info, query extended attrs if debug
            let ext_attrs = if debug {
                Some(window_system.get_extended_attributes(
                    sys_win.window_id,
                    sys_win.pid,
                    sys_win.layer,
                ))
            } else {
                None
            };

            windows.push(WindowInfo {
                id: sys_win.window_id,
                pid: sys_win.pid,
                title: sys_win.name.clone().unwrap_or_default(),
                app_name: sys_win.owner_name.clone(),
                app_id: sys_win.bundle_id.clone(),
                tags: 0,
                x: sys_win.bounds.x as i32,
                y: sys_win.bounds.y as i32,
                width: sys_win.bounds.width as u32,
                height: sys_win.bounds.height as u32,
                is_focused: false,
                is_floating: false,
                is_fullscreen: false,
                status: Some(WindowStatus::Ignored),
                ax_id: ext_attrs.as_ref().and_then(|a| a.ax_id.clone()),
                subrole: ext_attrs.as_ref().and_then(|a| a.subrole.clone()),
                window_level: ext_attrs.as_ref().map(|a| a.window_level),
                close_button: ext_attrs.as_ref().map(|a| a.close_button.clone()),
                fullscreen_button: ext_attrs.as_ref().map(|a| a.fullscreen_button.clone()),
                minimize_button: ext_attrs.as_ref().map(|a| a.minimize_button.clone()),
                zoom_button: ext_attrs.as_ref().map(|a| a.zoom_button.clone()),
            });
        }
    }

    Response::Windows { windows }
}

/// Format window level for display
fn format_window_level(level: &WindowLevel) -> String {
    match level {
        WindowLevel::Named(name) => match name {
            WindowLevelName::Normal => "normal".to_string(),
            WindowLevelName::Floating => "floating".to_string(),
            WindowLevelName::Modal => "modal".to_string(),
            WindowLevelName::Utility => "utility".to_string(),
            WindowLevelName::Popup => "popup".to_string(),
        },
        WindowLevel::Other(WindowLevelOther::Other) => "other".to_string(),
        WindowLevel::Numeric(n) => n.to_string(),
    }
}

/// Format button state for display
fn format_button_state(state: ButtonState) -> String {
    match state {
        ButtonState::Exists => "exists".to_string(),
        ButtonState::None => "none".to_string(),
        ButtonState::Enabled => "enabled".to_string(),
        ButtonState::Disabled => "disabled".to_string(),
    }
}
