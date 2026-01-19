mod app;
mod core;
mod effect;
mod event;
mod event_emitter;
mod ipc;
mod layout;
mod macos;
mod pid;
mod platform;

use anyhow::{bail, Result};
use argh::FromArgs;
use tracing_subscriber::EnvFilter;

use ipc::IpcClient;
use yashiki_ipc::{
    ButtonInfo, ButtonState, Command, CursorWarpMode, Direction, EventFilter, GlobPattern,
    OutputDirection, OutputSpecifier, Response, RuleAction, RuleMatcher, WindowLevel,
    WindowLevelName, WindowLevelOther, WindowRule, WindowStatus,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Yashiki - macOS tiling window manager
#[derive(FromArgs)]
struct Cli {
    #[argh(subcommand)]
    command: Option<SubCommand>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum SubCommand {
    Start(StartCmd),
    Version(VersionCmd),
    Bind(BindCmd),
    Unbind(UnbindCmd),
    ListBindings(ListBindingsCmd),
    TagView(TagViewCmd),
    TagToggle(TagToggleCmd),
    TagViewLast(TagViewLastCmd),
    WindowMoveToTag(WindowMoveToTagCmd),
    WindowToggleTag(WindowToggleTagCmd),
    WindowFocus(WindowFocusCmd),
    WindowSwap(WindowSwapCmd),
    WindowToggleFullscreen(WindowToggleFullscreenCmd),
    WindowToggleFloat(WindowToggleFloatCmd),
    WindowClose(WindowCloseCmd),
    OutputFocus(OutputFocusCmd),
    OutputSend(OutputSendCmd),
    Retile(RetileCmd),
    LayoutSetDefault(LayoutSetDefaultCmd),
    LayoutSet(LayoutSetCmd),
    LayoutGet(LayoutGetCmd),
    LayoutCmd(LayoutCmdCmd),
    ListWindows(ListWindowsCmd),
    ListOutputs(ListOutputsCmd),
    GetState(GetStateCmd),
    FocusedWindow(FocusedWindowCmd),
    Exec(ExecCmd),
    ExecOrFocus(ExecOrFocusCmd),
    ExecPath(ExecPathCmd),
    SetExecPath(SetExecPathCmd),
    AddExecPath(AddExecPathCmd),
    RuleAdd(RuleAddCmd),
    RuleDel(RuleDelCmd),
    ListRules(ListRulesCmd),
    SetCursorWarp(SetCursorWarpCmd),
    GetCursorWarp(GetCursorWarpCmd),
    SetOuterGap(SetOuterGapCmd),
    GetOuterGap(GetOuterGapCmd),
    Subscribe(SubscribeCmd),
    Quit(QuitCmd),
}

/// Start the yashiki daemon
#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
struct StartCmd {}

/// Show version information
#[derive(FromArgs)]
#[argh(subcommand, name = "version")]
struct VersionCmd {}

/// Bind a hotkey to a command
#[derive(FromArgs)]
#[argh(subcommand, name = "bind")]
struct BindCmd {
    /// hotkey (e.g., alt-1, cmd-shift-h)
    #[argh(positional)]
    key: String,
    /// command and arguments to bind
    #[argh(positional, greedy)]
    action: Vec<String>,
}

/// Unbind a hotkey
#[derive(FromArgs)]
#[argh(subcommand, name = "unbind")]
struct UnbindCmd {
    /// hotkey to unbind
    #[argh(positional)]
    key: String,
}

/// List all hotkey bindings
#[derive(FromArgs)]
#[argh(subcommand, name = "list-bindings")]
struct ListBindingsCmd {}

/// Switch to specific tags (bitmask)
#[derive(FromArgs)]
#[argh(subcommand, name = "tag-view")]
struct TagViewCmd {
    /// output (display) ID or name
    #[argh(option)]
    output: Option<String>,
    /// tags bitmask (e.g., 1 for tag 1, 2 for tag 2, 3 for tags 1+2)
    #[argh(positional)]
    tags: u32,
}

/// Toggle visibility of tags (bitmask)
#[derive(FromArgs)]
#[argh(subcommand, name = "tag-toggle")]
struct TagToggleCmd {
    /// output (display) ID or name
    #[argh(option)]
    output: Option<String>,
    /// tags bitmask to toggle
    #[argh(positional)]
    tags: u32,
}

/// Switch to the previously viewed tags
#[derive(FromArgs)]
#[argh(subcommand, name = "tag-view-last")]
struct TagViewLastCmd {}

/// Move focused window to tags (bitmask)
#[derive(FromArgs)]
#[argh(subcommand, name = "window-move-to-tag")]
struct WindowMoveToTagCmd {
    /// tags bitmask
    #[argh(positional)]
    tags: u32,
}

/// Toggle tags on the focused window (bitmask)
#[derive(FromArgs)]
#[argh(subcommand, name = "window-toggle-tag")]
struct WindowToggleTagCmd {
    /// tags bitmask to toggle
    #[argh(positional)]
    tags: u32,
}

/// Focus a window in the specified direction
#[derive(FromArgs)]
#[argh(subcommand, name = "window-focus")]
struct WindowFocusCmd {
    /// direction: left, right, up, down, next, prev
    #[argh(positional)]
    direction: String,
}

/// Swap focused window with window in the specified direction
#[derive(FromArgs)]
#[argh(subcommand, name = "window-swap")]
struct WindowSwapCmd {
    /// direction: left, right, up, down, next, prev
    #[argh(positional)]
    direction: String,
}

/// Toggle fullscreen for focused window (AeroSpace-style, not macOS native)
#[derive(FromArgs)]
#[argh(subcommand, name = "window-toggle-fullscreen")]
struct WindowToggleFullscreenCmd {}

/// Toggle floating state for focused window
#[derive(FromArgs)]
#[argh(subcommand, name = "window-toggle-float")]
struct WindowToggleFloatCmd {}

/// Close the focused window
#[derive(FromArgs)]
#[argh(subcommand, name = "window-close")]
struct WindowCloseCmd {}

/// Focus the next or previous display
#[derive(FromArgs)]
#[argh(subcommand, name = "output-focus")]
struct OutputFocusCmd {
    /// direction: next, prev
    #[argh(positional)]
    direction: String,
}

/// Send focused window to the next or previous display
#[derive(FromArgs)]
#[argh(subcommand, name = "output-send")]
struct OutputSendCmd {
    /// direction: next, prev
    #[argh(positional)]
    direction: String,
}

/// Re-apply the current layout
#[derive(FromArgs)]
#[argh(subcommand, name = "retile")]
struct RetileCmd {
    /// output (display) ID or name
    #[argh(option)]
    output: Option<String>,
}

/// Set the default layout engine
#[derive(FromArgs)]
#[argh(subcommand, name = "layout-set-default")]
struct LayoutSetDefaultCmd {
    /// layout engine name (e.g., tatami, byobu)
    #[argh(positional)]
    layout: String,
}

/// Set the layout engine for tags (current tag by default)
#[derive(FromArgs)]
#[argh(subcommand, name = "layout-set")]
struct LayoutSetCmd {
    /// tags bitmask, defaults to current tag
    #[argh(option)]
    tags: Option<u32>,
    /// output (display) ID or name
    #[argh(option)]
    output: Option<String>,
    /// layout engine name
    #[argh(positional)]
    layout: String,
}

/// Get the current layout engine
#[derive(FromArgs)]
#[argh(subcommand, name = "layout-get")]
struct LayoutGetCmd {
    /// tags bitmask, defaults to current layout
    #[argh(option)]
    tags: Option<u32>,
    /// output (display) ID or name
    #[argh(option)]
    output: Option<String>,
}

/// Send a command to the layout engine
#[derive(FromArgs)]
#[argh(subcommand, name = "layout-cmd")]
struct LayoutCmdCmd {
    /// target layout engine (defaults to current active layout)
    #[argh(option)]
    layout: Option<String>,
    /// layout command
    #[argh(positional)]
    cmd: String,
    /// command arguments
    #[argh(positional, greedy)]
    args: Vec<String>,
}

/// List all managed windows
#[derive(FromArgs)]
#[argh(subcommand, name = "list-windows")]
struct ListWindowsCmd {
    /// include ignored windows (popups, tooltips, etc.)
    #[argh(switch)]
    all: bool,
    /// show debug info (ax_id, subrole, window_level, buttons)
    #[argh(switch)]
    debug: bool,
}

/// List all displays/outputs
#[derive(FromArgs)]
#[argh(subcommand, name = "list-outputs")]
struct ListOutputsCmd {}

/// Get current window manager state
#[derive(FromArgs)]
#[argh(subcommand, name = "get-state")]
struct GetStateCmd {}

/// Get the focused window ID
#[derive(FromArgs)]
#[argh(subcommand, name = "focused-window")]
struct FocusedWindowCmd {}

/// Execute a shell command
#[derive(FromArgs)]
#[argh(subcommand, name = "exec")]
struct ExecCmd {
    /// shell command to execute
    #[argh(positional)]
    command: String,
}

/// Focus an app if running, otherwise execute a command to launch it
#[derive(FromArgs)]
#[argh(subcommand, name = "exec-or-focus")]
struct ExecOrFocusCmd {
    /// application name to focus
    #[argh(option)]
    app_name: String,
    /// shell command to execute if app is not running
    #[argh(positional)]
    command: String,
}

/// Get the current exec path
#[derive(FromArgs)]
#[argh(subcommand, name = "exec-path")]
struct ExecPathCmd {}

/// Set the exec path
#[derive(FromArgs)]
#[argh(subcommand, name = "set-exec-path")]
struct SetExecPathCmd {
    /// the path to set
    #[argh(positional)]
    path: String,
}

/// Add a path to exec path
#[derive(FromArgs)]
#[argh(subcommand, name = "add-exec-path")]
struct AddExecPathCmd {
    /// append to end instead of prepending to start
    #[argh(switch)]
    append: bool,
    /// the path to add
    #[argh(positional)]
    path: String,
}

/// Add a window rule
#[derive(FromArgs)]
#[argh(subcommand, name = "rule-add")]
struct RuleAddCmd {
    /// application name pattern (glob, e.g., "Safari", "*Chrome*")
    #[argh(option)]
    app_name: Option<String>,
    /// bundle identifier pattern (glob, e.g., "com.apple.Safari", "com.google.*")
    #[argh(option)]
    app_id: Option<String>,
    /// window title pattern (glob)
    #[argh(option)]
    title: Option<String>,
    /// AXIdentifier pattern (glob, "none" matches absent)
    #[argh(option)]
    ax_id: Option<String>,
    /// AXSubrole pattern (glob, AX prefix optional, "none" matches absent)
    #[argh(option)]
    subrole: Option<String>,
    /// window level (normal, floating, modal, utility, popup, other, or numeric)
    #[argh(option)]
    window_level: Option<String>,
    /// close button state (exists, none, enabled, disabled)
    #[argh(option)]
    close_button: Option<String>,
    /// fullscreen button state (exists, none, enabled, disabled)
    #[argh(option)]
    fullscreen_button: Option<String>,
    /// minimize button state (exists, none, enabled, disabled)
    #[argh(option)]
    minimize_button: Option<String>,
    /// zoom button state (exists, none, enabled, disabled)
    #[argh(option)]
    zoom_button: Option<String>,
    /// action and arguments (e.g., "float", "tags 2", "dimensions 800 600")
    #[argh(positional, greedy)]
    action: Vec<String>,
}

/// Remove a window rule
#[derive(FromArgs)]
#[argh(subcommand, name = "rule-del")]
struct RuleDelCmd {
    /// application name pattern (glob)
    #[argh(option)]
    app_name: Option<String>,
    /// bundle identifier pattern (glob)
    #[argh(option)]
    app_id: Option<String>,
    /// window title pattern (glob)
    #[argh(option)]
    title: Option<String>,
    /// AXIdentifier pattern (glob, "none" matches absent)
    #[argh(option)]
    ax_id: Option<String>,
    /// AXSubrole pattern (glob, AX prefix optional, "none" matches absent)
    #[argh(option)]
    subrole: Option<String>,
    /// window level (normal, floating, modal, utility, popup, other, or numeric)
    #[argh(option)]
    window_level: Option<String>,
    /// close button state (exists, none, enabled, disabled)
    #[argh(option)]
    close_button: Option<String>,
    /// fullscreen button state (exists, none, enabled, disabled)
    #[argh(option)]
    fullscreen_button: Option<String>,
    /// minimize button state (exists, none, enabled, disabled)
    #[argh(option)]
    minimize_button: Option<String>,
    /// zoom button state (exists, none, enabled, disabled)
    #[argh(option)]
    zoom_button: Option<String>,
    /// action to remove (e.g., "float", "tags")
    #[argh(positional, greedy)]
    action: Vec<String>,
}

/// List all window rules
#[derive(FromArgs)]
#[argh(subcommand, name = "list-rules")]
struct ListRulesCmd {}

/// Set cursor warp mode (mouse follows focus)
#[derive(FromArgs)]
#[argh(subcommand, name = "set-cursor-warp")]
struct SetCursorWarpCmd {
    /// mode: disabled, on-output-change, on-focus-change
    #[argh(positional)]
    mode: String,
}

/// Get current cursor warp mode
#[derive(FromArgs)]
#[argh(subcommand, name = "get-cursor-warp")]
struct GetCursorWarpCmd {}

/// Set the outer gap (gap between windows and screen edges)
#[derive(FromArgs)]
#[argh(subcommand, name = "set-outer-gap")]
struct SetOuterGapCmd {
    /// gap values: <all> | <v h> | <t r b l> (CSS-style: 1, 2, or 4 values)
    #[argh(positional, greedy)]
    values: Vec<String>,
}

/// Get current outer gap
#[derive(FromArgs)]
#[argh(subcommand, name = "get-outer-gap")]
struct GetOuterGapCmd {}

/// Subscribe to state change events
#[derive(FromArgs)]
#[argh(subcommand, name = "subscribe")]
struct SubscribeCmd {
    /// request a snapshot on connection
    #[argh(switch)]
    snapshot: bool,
    /// filter events (comma-separated: window,focus,display,tags,layout)
    #[argh(option)]
    filter: Option<String>,
}

/// Quit the yashiki daemon
#[derive(FromArgs)]
#[argh(subcommand, name = "quit")]
struct QuitCmd {}

fn main() -> Result<()> {
    let cli: Cli = argh::from_env();

    match cli.command {
        None => {
            // No subcommand - show help (simulate --help)
            let args: Vec<&str> = vec!["yashiki", "--help"];
            match Cli::from_args(&args[..1], &args[1..]) {
                Ok(_) => {}
                Err(e) => {
                    println!("{}", e.output);
                }
            }
            Ok(())
        }
        Some(SubCommand::Start(_)) => {
            // Start daemon
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .init();

            tracing::info!("yashiki starting");
            app::App::run()
        }
        Some(SubCommand::Version(_)) => {
            println!("yashiki {}", VERSION);
            Ok(())
        }
        Some(SubCommand::Subscribe(cmd)) => {
            // Subscribe to events (separate from normal IPC)
            let filter = cmd.filter.map(|f| parse_event_filter(&f));
            ipc::subscribe_and_print(cmd.snapshot, filter)
        }
        Some(subcmd) => run_cli(subcmd),
    }
}

fn run_cli(subcmd: SubCommand) -> Result<()> {
    let cmd = to_command(subcmd)?;
    let mut client = IpcClient::connect()?;
    let response = client.send(&cmd)?;

    match response {
        Response::Ok => {}
        Response::Error { message } => {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
        Response::Windows { windows } => {
            for w in windows {
                let mut flags = Vec::new();
                // Status (ignored/managed) if present
                if let Some(status) = &w.status {
                    match status {
                        WindowStatus::Ignored => flags.push("ignored".to_string()),
                        WindowStatus::Managed => {}
                    }
                }
                if w.is_focused {
                    flags.push("*".to_string());
                }
                if w.is_floating {
                    flags.push("float".to_string());
                }
                if w.is_fullscreen {
                    flags.push("full".to_string());
                }
                let flag_str = if flags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", flags.join(","))
                };
                println!(
                    "{}: {} ({}) [tags={}, {}x{} @ ({},{})]{}",
                    w.id,
                    w.app_name,
                    w.app_id.as_deref().unwrap_or("-"),
                    w.tags,
                    w.width,
                    w.height,
                    w.x,
                    w.y,
                    flag_str
                );
                // Debug info if present
                if w.ax_id.is_some()
                    || w.subrole.is_some()
                    || w.window_level.is_some()
                    || w.close_button.is_some()
                {
                    let mut debug_parts = Vec::new();
                    debug_parts.push(format!("title={:?}", w.title));
                    if let Some(ax_id) = &w.ax_id {
                        debug_parts.push(format!("ax_id={}", ax_id));
                    }
                    if let Some(subrole) = &w.subrole {
                        debug_parts.push(format!("subrole={}", subrole));
                    }
                    if let Some(level) = &w.window_level {
                        let level_name = match *level {
                            0 => "normal".to_string(),
                            3 => "floating".to_string(),
                            8 => "modal".to_string(),
                            19 => "utility".to_string(),
                            101 => "popup".to_string(),
                            n => n.to_string(),
                        };
                        debug_parts.push(format!("level={}", level_name));
                    }
                    if let Some(btn) = &w.close_button {
                        let state = format_button_state(btn);
                        debug_parts.push(format!("close={}", state));
                    }
                    if let Some(btn) = &w.fullscreen_button {
                        let state = format_button_state(btn);
                        debug_parts.push(format!("fullscreen={}", state));
                    }
                    if let Some(btn) = &w.minimize_button {
                        let state = format_button_state(btn);
                        debug_parts.push(format!("minimize={}", state));
                    }
                    if let Some(btn) = &w.zoom_button {
                        let state = format_button_state(btn);
                        debug_parts.push(format!("zoom={}", state));
                    }
                    println!("  {}", debug_parts.join(", "));
                }
            }
        }
        Response::Outputs { outputs } => {
            let mut sorted_outputs = outputs;
            sorted_outputs.sort_by_key(|o| o.id);
            for o in sorted_outputs {
                let main_marker = if o.is_main { " (main)" } else { "" };
                let focused_marker = if o.is_focused { " *" } else { "" };
                println!(
                    "{}: {} [{}x{} @ ({},{})]{}{}",
                    o.id, o.name, o.width, o.height, o.x, o.y, main_marker, focused_marker
                );
                println!("  visible_tags: {}", o.visible_tags);
            }
        }
        Response::State { state } => {
            println!("Visible tags: {}", state.visible_tags);
            println!("Focused window: {:?}", state.focused_window_id);
            println!("Window count: {}", state.window_count);
            println!("Default layout: {}", state.default_layout);
            println!(
                "Current layout: {}",
                state.current_layout.as_deref().unwrap_or("(default)")
            );
        }
        Response::Bindings { bindings } => {
            for b in bindings {
                println!("{} -> {}", b.key, b.action);
            }
        }
        Response::WindowId { id } => {
            if let Some(id) = id {
                println!("{}", id);
            } else {
                std::process::exit(1);
            }
        }
        Response::Layout { layout } => {
            println!("{}", layout);
        }
        Response::ExecPath { path } => {
            println!("{}", path);
        }
        Response::Rules { rules } => {
            for r in rules {
                let mut matchers = Vec::new();
                if let Some(app) = &r.app_name {
                    matchers.push(format!("--app-name {}", app));
                }
                if let Some(app_id) = &r.app_id {
                    matchers.push(format!("--app-id {}", app_id));
                }
                if let Some(title) = &r.title {
                    matchers.push(format!("--title {}", title));
                }
                if let Some(ax_id) = &r.ax_id {
                    matchers.push(format!("--ax-id {}", ax_id));
                }
                if let Some(subrole) = &r.subrole {
                    matchers.push(format!("--subrole {}", subrole));
                }
                if matchers.is_empty() {
                    matchers.push("*".to_string());
                }
                println!("{} -> {}", matchers.join(" "), r.action);
            }
        }
        Response::CursorWarp { mode } => {
            let mode_str = match mode {
                CursorWarpMode::Disabled => "disabled",
                CursorWarpMode::OnOutputChange => "on-output-change",
                CursorWarpMode::OnFocusChange => "on-focus-change",
            };
            println!("{}", mode_str);
        }
        Response::OuterGap { outer_gap } => {
            println!("{}", outer_gap);
        }
    }

    Ok(())
}

fn to_command(subcmd: SubCommand) -> Result<Command> {
    match subcmd {
        SubCommand::Start(_) | SubCommand::Version(_) | SubCommand::Subscribe(_) => {
            unreachable!("handled in main")
        }
        SubCommand::Bind(cmd) => {
            if cmd.action.is_empty() {
                bail!("bind requires a command to bind");
            }
            let action = parse_command(&cmd.action)?;
            Ok(Command::Bind {
                key: cmd.key,
                action: Box::new(action),
            })
        }
        SubCommand::Unbind(cmd) => Ok(Command::Unbind { key: cmd.key }),
        SubCommand::ListBindings(_) => Ok(Command::ListBindings),
        SubCommand::TagView(cmd) => Ok(Command::TagView {
            tags: cmd.tags,
            output: parse_output_specifier(cmd.output),
        }),
        SubCommand::TagToggle(cmd) => Ok(Command::TagToggle {
            tags: cmd.tags,
            output: parse_output_specifier(cmd.output),
        }),
        SubCommand::TagViewLast(_) => Ok(Command::TagViewLast),
        SubCommand::WindowMoveToTag(cmd) => Ok(Command::WindowMoveToTag { tags: cmd.tags }),
        SubCommand::WindowToggleTag(cmd) => Ok(Command::WindowToggleTag { tags: cmd.tags }),
        SubCommand::WindowFocus(cmd) => Ok(Command::WindowFocus {
            direction: parse_direction(&cmd.direction)?,
        }),
        SubCommand::WindowSwap(cmd) => Ok(Command::WindowSwap {
            direction: parse_direction(&cmd.direction)?,
        }),
        SubCommand::WindowToggleFullscreen(_) => Ok(Command::WindowToggleFullscreen),
        SubCommand::WindowToggleFloat(_) => Ok(Command::WindowToggleFloat),
        SubCommand::WindowClose(_) => Ok(Command::WindowClose),
        SubCommand::OutputFocus(cmd) => Ok(Command::OutputFocus {
            direction: parse_output_direction(&cmd.direction)?,
        }),
        SubCommand::OutputSend(cmd) => Ok(Command::OutputSend {
            direction: parse_output_direction(&cmd.direction)?,
        }),
        SubCommand::Retile(cmd) => Ok(Command::Retile {
            output: parse_output_specifier(cmd.output),
        }),
        SubCommand::LayoutSetDefault(cmd) => Ok(Command::LayoutSetDefault { layout: cmd.layout }),
        SubCommand::LayoutSet(cmd) => Ok(Command::LayoutSet {
            tags: cmd.tags,
            output: parse_output_specifier(cmd.output),
            layout: cmd.layout,
        }),
        SubCommand::LayoutGet(cmd) => Ok(Command::LayoutGet {
            tags: cmd.tags,
            output: parse_output_specifier(cmd.output),
        }),
        SubCommand::LayoutCmd(cmd) => Ok(Command::LayoutCommand {
            layout: cmd.layout,
            cmd: cmd.cmd,
            args: cmd.args,
        }),
        SubCommand::ListWindows(cmd) => Ok(Command::ListWindows {
            all: cmd.all,
            debug: cmd.debug,
        }),
        SubCommand::ListOutputs(_) => Ok(Command::ListOutputs),
        SubCommand::GetState(_) => Ok(Command::GetState),
        SubCommand::FocusedWindow(_) => Ok(Command::FocusedWindow),
        SubCommand::Exec(cmd) => Ok(Command::Exec {
            command: cmd.command,
        }),
        SubCommand::ExecOrFocus(cmd) => Ok(Command::ExecOrFocus {
            app_name: cmd.app_name,
            command: cmd.command,
        }),
        SubCommand::ExecPath(_) => Ok(Command::GetExecPath),
        SubCommand::SetExecPath(cmd) => Ok(Command::SetExecPath { path: cmd.path }),
        SubCommand::AddExecPath(cmd) => Ok(Command::AddExecPath {
            path: cmd.path,
            append: cmd.append,
        }),
        SubCommand::RuleAdd(cmd) => {
            if cmd.app_name.is_none()
                && cmd.app_id.is_none()
                && cmd.title.is_none()
                && cmd.ax_id.is_none()
                && cmd.subrole.is_none()
                && cmd.window_level.is_none()
                && cmd.close_button.is_none()
                && cmd.fullscreen_button.is_none()
                && cmd.minimize_button.is_none()
                && cmd.zoom_button.is_none()
            {
                bail!("rule-add requires at least one matcher (--app-name, --app-id, --title, --ax-id, --subrole, --window-level, or button options)");
            }
            if cmd.action.is_empty() {
                bail!("rule-add requires an action");
            }
            let window_level = cmd
                .window_level
                .map(|s| parse_window_level(&s))
                .transpose()?;
            let close_button = cmd
                .close_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let fullscreen_button = cmd
                .fullscreen_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let minimize_button = cmd
                .minimize_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let zoom_button = cmd
                .zoom_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let matcher = RuleMatcher::with_extended(
                cmd.app_name.map(GlobPattern::new),
                cmd.app_id.map(GlobPattern::new),
                cmd.title.map(GlobPattern::new),
                cmd.ax_id.map(GlobPattern::new),
                cmd.subrole.map(GlobPattern::new),
                window_level,
                close_button,
                fullscreen_button,
                minimize_button,
                zoom_button,
            );
            let action = parse_rule_action(&cmd.action)?;
            Ok(Command::RuleAdd {
                rule: WindowRule::new(matcher, action),
            })
        }
        SubCommand::RuleDel(cmd) => {
            if cmd.app_name.is_none()
                && cmd.app_id.is_none()
                && cmd.title.is_none()
                && cmd.ax_id.is_none()
                && cmd.subrole.is_none()
                && cmd.window_level.is_none()
                && cmd.close_button.is_none()
                && cmd.fullscreen_button.is_none()
                && cmd.minimize_button.is_none()
                && cmd.zoom_button.is_none()
            {
                bail!("rule-del requires at least one matcher (--app-name, --app-id, --title, --ax-id, --subrole, --window-level, or button options)");
            }
            if cmd.action.is_empty() {
                bail!("rule-del requires an action");
            }
            let window_level = cmd
                .window_level
                .map(|s| parse_window_level(&s))
                .transpose()?;
            let close_button = cmd
                .close_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let fullscreen_button = cmd
                .fullscreen_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let minimize_button = cmd
                .minimize_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let zoom_button = cmd
                .zoom_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let matcher = RuleMatcher::with_extended(
                cmd.app_name.map(GlobPattern::new),
                cmd.app_id.map(GlobPattern::new),
                cmd.title.map(GlobPattern::new),
                cmd.ax_id.map(GlobPattern::new),
                cmd.subrole.map(GlobPattern::new),
                window_level,
                close_button,
                fullscreen_button,
                minimize_button,
                zoom_button,
            );
            let action = parse_rule_action(&cmd.action)?;
            Ok(Command::RuleDel { matcher, action })
        }
        SubCommand::ListRules(_) => Ok(Command::ListRules),
        SubCommand::SetCursorWarp(cmd) => {
            let mode = parse_cursor_warp_mode(&cmd.mode)?;
            Ok(Command::SetCursorWarp { mode })
        }
        SubCommand::GetCursorWarp(_) => Ok(Command::GetCursorWarp),
        SubCommand::SetOuterGap(cmd) => {
            if cmd.values.is_empty() {
                bail!("set-outer-gap requires at least one value");
            }
            if cmd.values.len() != 1 && cmd.values.len() != 2 && cmd.values.len() != 4 {
                bail!(
                    "set-outer-gap: expected 1, 2, or 4 values (got {})",
                    cmd.values.len()
                );
            }
            Ok(Command::SetOuterGap { values: cmd.values })
        }
        SubCommand::GetOuterGap(_) => Ok(Command::GetOuterGap),
        SubCommand::Quit(_) => Ok(Command::Quit),
    }
}

fn parse_command(args: &[String]) -> Result<Command> {
    if args.is_empty() {
        bail!("No command provided");
    }

    let cmd_name = &args[0];
    let cmd_args: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();

    fn from_argh<T: argh::FromArgs>(name: &str, args: &[&str]) -> Result<T> {
        T::from_args(&[name], args).map_err(|e| anyhow::anyhow!("{}", e.output))
    }

    match cmd_name.as_str() {
        "bind" => {
            let cmd: BindCmd = from_argh(cmd_name, &cmd_args)?;
            if cmd.action.is_empty() {
                bail!("bind requires a command to bind");
            }
            let action = parse_command(&cmd.action)?;
            Ok(Command::Bind {
                key: cmd.key,
                action: Box::new(action),
            })
        }
        "unbind" => {
            let cmd: UnbindCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::Unbind { key: cmd.key })
        }
        "list-bindings" => Ok(Command::ListBindings),
        "tag-view" => {
            let cmd: TagViewCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::TagView {
                tags: cmd.tags,
                output: parse_output_specifier(cmd.output),
            })
        }
        "tag-toggle" => {
            let cmd: TagToggleCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::TagToggle {
                tags: cmd.tags,
                output: parse_output_specifier(cmd.output),
            })
        }
        "tag-view-last" => Ok(Command::TagViewLast),
        "window-move-to-tag" => {
            let cmd: WindowMoveToTagCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::WindowMoveToTag { tags: cmd.tags })
        }
        "window-toggle-tag" => {
            let cmd: WindowToggleTagCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::WindowToggleTag { tags: cmd.tags })
        }
        "window-focus" => {
            let cmd: WindowFocusCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::WindowFocus {
                direction: parse_direction(&cmd.direction)?,
            })
        }
        "window-swap" => {
            let cmd: WindowSwapCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::WindowSwap {
                direction: parse_direction(&cmd.direction)?,
            })
        }
        "window-toggle-fullscreen" => Ok(Command::WindowToggleFullscreen),
        "window-toggle-float" => Ok(Command::WindowToggleFloat),
        "window-close" => Ok(Command::WindowClose),
        "output-focus" => {
            let cmd: OutputFocusCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::OutputFocus {
                direction: parse_output_direction(&cmd.direction)?,
            })
        }
        "output-send" => {
            let cmd: OutputSendCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::OutputSend {
                direction: parse_output_direction(&cmd.direction)?,
            })
        }
        "retile" => {
            let cmd: RetileCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::Retile {
                output: parse_output_specifier(cmd.output),
            })
        }
        "layout-set-default" => {
            let cmd: LayoutSetDefaultCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::LayoutSetDefault { layout: cmd.layout })
        }
        "layout-set" => {
            let cmd: LayoutSetCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::LayoutSet {
                tags: cmd.tags,
                output: parse_output_specifier(cmd.output),
                layout: cmd.layout,
            })
        }
        "layout-get" => {
            let cmd: LayoutGetCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::LayoutGet {
                tags: cmd.tags,
                output: parse_output_specifier(cmd.output),
            })
        }
        "layout-cmd" => {
            let cmd: LayoutCmdCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::LayoutCommand {
                layout: cmd.layout,
                cmd: cmd.cmd,
                args: cmd.args,
            })
        }
        "list-windows" => {
            let cmd: ListWindowsCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::ListWindows {
                all: cmd.all,
                debug: cmd.debug,
            })
        }
        "list-outputs" => Ok(Command::ListOutputs),
        "get-state" => Ok(Command::GetState),
        "focused-window" => Ok(Command::FocusedWindow),
        "exec" => {
            let cmd: ExecCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::Exec {
                command: cmd.command,
            })
        }
        "exec-or-focus" => {
            let cmd: ExecOrFocusCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::ExecOrFocus {
                app_name: cmd.app_name,
                command: cmd.command,
            })
        }
        "exec-path" => Ok(Command::GetExecPath),
        "set-exec-path" => {
            let cmd: SetExecPathCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::SetExecPath { path: cmd.path })
        }
        "add-exec-path" => {
            let cmd: AddExecPathCmd = from_argh(cmd_name, &cmd_args)?;
            Ok(Command::AddExecPath {
                path: cmd.path,
                append: cmd.append,
            })
        }
        "rule-add" => {
            let cmd: RuleAddCmd = from_argh(cmd_name, &cmd_args)?;
            if cmd.app_name.is_none()
                && cmd.app_id.is_none()
                && cmd.title.is_none()
                && cmd.ax_id.is_none()
                && cmd.subrole.is_none()
                && cmd.window_level.is_none()
                && cmd.close_button.is_none()
                && cmd.fullscreen_button.is_none()
                && cmd.minimize_button.is_none()
                && cmd.zoom_button.is_none()
            {
                bail!("rule-add requires at least one matcher (--app-name, --app-id, --title, --ax-id, --subrole, --window-level, or button options)");
            }
            if cmd.action.is_empty() {
                bail!("rule-add requires an action");
            }
            let window_level = cmd
                .window_level
                .map(|s| parse_window_level(&s))
                .transpose()?;
            let close_button = cmd
                .close_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let fullscreen_button = cmd
                .fullscreen_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let minimize_button = cmd
                .minimize_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let zoom_button = cmd
                .zoom_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let matcher = RuleMatcher::with_extended(
                cmd.app_name.map(GlobPattern::new),
                cmd.app_id.map(GlobPattern::new),
                cmd.title.map(GlobPattern::new),
                cmd.ax_id.map(GlobPattern::new),
                cmd.subrole.map(GlobPattern::new),
                window_level,
                close_button,
                fullscreen_button,
                minimize_button,
                zoom_button,
            );
            let action = parse_rule_action(&cmd.action)?;
            Ok(Command::RuleAdd {
                rule: WindowRule::new(matcher, action),
            })
        }
        "rule-del" => {
            let cmd: RuleDelCmd = from_argh(cmd_name, &cmd_args)?;
            if cmd.app_name.is_none()
                && cmd.app_id.is_none()
                && cmd.title.is_none()
                && cmd.ax_id.is_none()
                && cmd.subrole.is_none()
                && cmd.window_level.is_none()
                && cmd.close_button.is_none()
                && cmd.fullscreen_button.is_none()
                && cmd.minimize_button.is_none()
                && cmd.zoom_button.is_none()
            {
                bail!("rule-del requires at least one matcher (--app-name, --app-id, --title, --ax-id, --subrole, --window-level, or button options)");
            }
            if cmd.action.is_empty() {
                bail!("rule-del requires an action");
            }
            let window_level = cmd
                .window_level
                .map(|s| parse_window_level(&s))
                .transpose()?;
            let close_button = cmd
                .close_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let fullscreen_button = cmd
                .fullscreen_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let minimize_button = cmd
                .minimize_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let zoom_button = cmd
                .zoom_button
                .map(|s| parse_button_state(&s))
                .transpose()?;
            let matcher = RuleMatcher::with_extended(
                cmd.app_name.map(GlobPattern::new),
                cmd.app_id.map(GlobPattern::new),
                cmd.title.map(GlobPattern::new),
                cmd.ax_id.map(GlobPattern::new),
                cmd.subrole.map(GlobPattern::new),
                window_level,
                close_button,
                fullscreen_button,
                minimize_button,
                zoom_button,
            );
            let action = parse_rule_action(&cmd.action)?;
            Ok(Command::RuleDel { matcher, action })
        }
        "list-rules" => Ok(Command::ListRules),
        "set-cursor-warp" => {
            let cmd: SetCursorWarpCmd = from_argh(cmd_name, &cmd_args)?;
            let mode = parse_cursor_warp_mode(&cmd.mode)?;
            Ok(Command::SetCursorWarp { mode })
        }
        "get-cursor-warp" => Ok(Command::GetCursorWarp),
        "set-outer-gap" => {
            let cmd: SetOuterGapCmd = from_argh(cmd_name, &cmd_args)?;
            if cmd.values.is_empty() {
                bail!("set-outer-gap requires at least one value");
            }
            if cmd.values.len() != 1 && cmd.values.len() != 2 && cmd.values.len() != 4 {
                bail!(
                    "set-outer-gap: expected 1, 2, or 4 values (got {})",
                    cmd.values.len()
                );
            }
            Ok(Command::SetOuterGap { values: cmd.values })
        }
        "get-outer-gap" => Ok(Command::GetOuterGap),
        "quit" => Ok(Command::Quit),
        _ => bail!("Unknown command: {}", cmd_name),
    }
}

fn parse_direction(s: &str) -> Result<Direction> {
    match s.to_lowercase().as_str() {
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        "up" => Ok(Direction::Up),
        "down" => Ok(Direction::Down),
        "next" => Ok(Direction::Next),
        "prev" => Ok(Direction::Prev),
        _ => bail!(
            "Unknown direction: {} (use left, right, up, down, next, prev)",
            s
        ),
    }
}

fn parse_output_direction(s: &str) -> Result<OutputDirection> {
    match s.to_lowercase().as_str() {
        "next" => Ok(OutputDirection::Next),
        "prev" => Ok(OutputDirection::Prev),
        _ => bail!("Unknown output direction: {} (use next or prev)", s),
    }
}

fn parse_cursor_warp_mode(s: &str) -> Result<CursorWarpMode> {
    match s.to_lowercase().as_str() {
        "disabled" => Ok(CursorWarpMode::Disabled),
        "on-output-change" => Ok(CursorWarpMode::OnOutputChange),
        "on-focus-change" => Ok(CursorWarpMode::OnFocusChange),
        _ => bail!(
            "Unknown cursor warp mode: {} (use disabled, on-output-change, on-focus-change)",
            s
        ),
    }
}

fn parse_window_level(s: &str) -> Result<WindowLevel> {
    match s.to_lowercase().as_str() {
        "normal" => Ok(WindowLevel::Named(WindowLevelName::Normal)),
        "floating" => Ok(WindowLevel::Named(WindowLevelName::Floating)),
        "modal" => Ok(WindowLevel::Named(WindowLevelName::Modal)),
        "utility" => Ok(WindowLevel::Named(WindowLevelName::Utility)),
        "popup" => Ok(WindowLevel::Named(WindowLevelName::Popup)),
        "other" => Ok(WindowLevel::Other(WindowLevelOther::Other)),
        _ => {
            // Try parsing as a number
            if let Ok(n) = s.parse::<i32>() {
                Ok(WindowLevel::Numeric(n))
            } else {
                bail!(
                    "Unknown window level: {} (use normal, floating, modal, utility, popup, other, or a number)",
                    s
                )
            }
        }
    }
}

fn parse_button_state(s: &str) -> Result<ButtonState> {
    match s.to_lowercase().as_str() {
        "exists" => Ok(ButtonState::Exists),
        "none" => Ok(ButtonState::None),
        "enabled" => Ok(ButtonState::Enabled),
        "disabled" => Ok(ButtonState::Disabled),
        _ => bail!(
            "Unknown button state: {} (use exists, none, enabled, disabled)",
            s
        ),
    }
}

fn parse_output_specifier(s: Option<String>) -> Option<OutputSpecifier> {
    s.map(|s| {
        if let Ok(id) = s.parse::<u32>() {
            OutputSpecifier::Id(id)
        } else {
            OutputSpecifier::Name(s)
        }
    })
}

fn parse_rule_action(args: &[String]) -> Result<RuleAction> {
    if args.is_empty() {
        bail!("No action provided");
    }

    let action_name = args[0].to_lowercase();
    let action_args = &args[1..];

    match action_name.as_str() {
        "ignore" => Ok(RuleAction::Ignore),
        "float" => Ok(RuleAction::Float),
        "no-float" => Ok(RuleAction::NoFloat),
        "tags" => {
            if action_args.is_empty() {
                bail!("tags action requires a bitmask argument");
            }
            let tags = action_args[0]
                .parse::<u32>()
                .map_err(|_| anyhow::anyhow!("Invalid tags bitmask: {}", action_args[0]))?;
            Ok(RuleAction::Tags { tags })
        }
        "output" => {
            if action_args.is_empty() {
                bail!("output action requires an output ID or name");
            }
            let output = if let Ok(id) = action_args[0].parse::<u32>() {
                OutputSpecifier::Id(id)
            } else {
                OutputSpecifier::Name(action_args[0].clone())
            };
            Ok(RuleAction::Output { output })
        }
        "position" => {
            if action_args.len() < 2 {
                bail!("position action requires x and y arguments");
            }
            let x = action_args[0]
                .parse::<i32>()
                .map_err(|_| anyhow::anyhow!("Invalid x position: {}", action_args[0]))?;
            let y = action_args[1]
                .parse::<i32>()
                .map_err(|_| anyhow::anyhow!("Invalid y position: {}", action_args[1]))?;
            Ok(RuleAction::Position { x, y })
        }
        "dimensions" => {
            if action_args.len() < 2 {
                bail!("dimensions action requires width and height arguments");
            }
            let width = action_args[0]
                .parse::<u32>()
                .map_err(|_| anyhow::anyhow!("Invalid width: {}", action_args[0]))?;
            let height = action_args[1]
                .parse::<u32>()
                .map_err(|_| anyhow::anyhow!("Invalid height: {}", action_args[1]))?;
            Ok(RuleAction::Dimensions { width, height })
        }
        _ => bail!(
            "Unknown rule action: {} (use ignore, float, no-float, tags, output, position, dimensions)",
            action_name
        ),
    }
}

fn parse_event_filter(s: &str) -> EventFilter {
    let mut filter = EventFilter::default();
    for part in s.split(',') {
        match part.trim().to_lowercase().as_str() {
            "window" => filter.window = true,
            "focus" => filter.focus = true,
            "display" => filter.display = true,
            "tags" => filter.tags = true,
            "layout" => filter.layout = true,
            _ => {}
        }
    }
    filter
}

fn format_button_state(btn: &ButtonInfo) -> &'static str {
    if !btn.exists {
        "none"
    } else if btn.enabled == Some(true) {
        "enabled"
    } else if btn.enabled == Some(false) {
        "disabled"
    } else {
        "exists"
    }
}
