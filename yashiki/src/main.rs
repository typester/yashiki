mod app;
mod core;
mod effect;
mod event;
mod ipc;
mod layout;
mod macos;
mod pid;
mod platform;

use anyhow::{bail, Result};
use argh::FromArgs;
use ipc::IpcClient;
use tracing_subscriber::EnvFilter;
use yashiki_ipc::{Command, Direction, OutputDirection, OutputSpecifier, Response};

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
struct ListWindowsCmd {}

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
                println!(
                    "{}: {} - {} [tags={}, {}x{} @ ({},{})]{}",
                    w.id,
                    w.app_name,
                    w.title,
                    w.tags,
                    w.width,
                    w.height,
                    w.x,
                    w.y,
                    if w.is_focused { " *" } else { "" }
                );
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
    }

    Ok(())
}

fn to_command(subcmd: SubCommand) -> Result<Command> {
    match subcmd {
        SubCommand::Start(_) | SubCommand::Version(_) => {
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
        SubCommand::ListWindows(_) => Ok(Command::ListWindows),
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
        "list-windows" => Ok(Command::ListWindows),
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

fn parse_output_specifier(s: Option<String>) -> Option<OutputSpecifier> {
    s.map(|s| {
        if let Ok(id) = s.parse::<u32>() {
            OutputSpecifier::Id(id)
        } else {
            OutputSpecifier::Name(s)
        }
    })
}
