mod app;
mod core;
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
use yashiki_ipc::{Command, Direction, OutputDirection, Response};

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
    ViewTag(ViewTagCmd),
    ToggleViewTag(ToggleViewTagCmd),
    ViewTagLast(ViewTagLastCmd),
    MoveToTag(MoveToTagCmd),
    ToggleWindowTag(ToggleWindowTagCmd),
    FocusWindow(FocusWindowCmd),
    SwapWindow(SwapWindowCmd),
    Zoom(ZoomCmd),
    FocusOutput(FocusOutputCmd),
    SendToOutput(SendToOutputCmd),
    Retile(RetileCmd),
    LayoutCmd(LayoutCmdCmd),
    ListWindows(ListWindowsCmd),
    GetState(GetStateCmd),
    FocusedWindow(FocusedWindowCmd),
    Exec(ExecCmd),
    ExecOrFocus(ExecOrFocusCmd),
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

/// Switch to a specific tag
#[derive(FromArgs)]
#[argh(subcommand, name = "view-tag")]
struct ViewTagCmd {
    /// tag number (1-32)
    #[argh(positional)]
    tag: u32,
}

/// Toggle visibility of a tag
#[derive(FromArgs)]
#[argh(subcommand, name = "toggle-view-tag")]
struct ToggleViewTagCmd {
    /// tag number (1-32)
    #[argh(positional)]
    tag: u32,
}

/// Switch to the previously viewed tag
#[derive(FromArgs)]
#[argh(subcommand, name = "view-tag-last")]
struct ViewTagLastCmd {}

/// Move focused window to a tag
#[derive(FromArgs)]
#[argh(subcommand, name = "move-to-tag")]
struct MoveToTagCmd {
    /// tag number (1-32)
    #[argh(positional)]
    tag: u32,
}

/// Toggle a tag on the focused window
#[derive(FromArgs)]
#[argh(subcommand, name = "toggle-window-tag")]
struct ToggleWindowTagCmd {
    /// tag number (1-32)
    #[argh(positional)]
    tag: u32,
}

/// Focus a window in the specified direction
#[derive(FromArgs)]
#[argh(subcommand, name = "focus-window")]
struct FocusWindowCmd {
    /// direction: left, right, up, down, next, prev
    #[argh(positional)]
    direction: String,
}

/// Swap focused window with window in the specified direction
#[derive(FromArgs)]
#[argh(subcommand, name = "swap-window")]
struct SwapWindowCmd {
    /// direction: left, right, up, down, next, prev
    #[argh(positional)]
    direction: String,
}

/// Toggle zoom on focused window
#[derive(FromArgs)]
#[argh(subcommand, name = "zoom")]
struct ZoomCmd {}

/// Focus the next or previous display
#[derive(FromArgs)]
#[argh(subcommand, name = "focus-output")]
struct FocusOutputCmd {
    /// direction: next, prev
    #[argh(positional)]
    direction: String,
}

/// Send focused window to the next or previous display
#[derive(FromArgs)]
#[argh(subcommand, name = "send-to-output")]
struct SendToOutputCmd {
    /// direction: next, prev
    #[argh(positional)]
    direction: String,
}

/// Re-apply the current layout
#[derive(FromArgs)]
#[argh(subcommand, name = "retile")]
struct RetileCmd {}

/// Send a command to the layout engine
#[derive(FromArgs)]
#[argh(subcommand, name = "layout-cmd")]
struct LayoutCmdCmd {
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
        Response::State { state } => {
            println!("Visible tags: {}", state.visible_tags);
            println!("Focused window: {:?}", state.focused_window_id);
            println!("Window count: {}", state.window_count);
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
        SubCommand::ViewTag(cmd) => Ok(Command::ViewTag { tag: cmd.tag }),
        SubCommand::ToggleViewTag(cmd) => Ok(Command::ToggleViewTag { tag: cmd.tag }),
        SubCommand::ViewTagLast(_) => Ok(Command::ViewTagLast),
        SubCommand::MoveToTag(cmd) => Ok(Command::MoveToTag { tag: cmd.tag }),
        SubCommand::ToggleWindowTag(cmd) => Ok(Command::ToggleWindowTag { tag: cmd.tag }),
        SubCommand::FocusWindow(cmd) => Ok(Command::FocusWindow {
            direction: parse_direction(&cmd.direction)?,
        }),
        SubCommand::SwapWindow(cmd) => Ok(Command::SwapWindow {
            direction: parse_direction(&cmd.direction)?,
        }),
        SubCommand::Zoom(_) => Ok(Command::Zoom),
        SubCommand::FocusOutput(cmd) => Ok(Command::FocusOutput {
            direction: parse_output_direction(&cmd.direction)?,
        }),
        SubCommand::SendToOutput(cmd) => Ok(Command::SendToOutput {
            direction: parse_output_direction(&cmd.direction)?,
        }),
        SubCommand::Retile(_) => Ok(Command::Retile),
        SubCommand::LayoutCmd(cmd) => Ok(Command::LayoutCommand {
            cmd: cmd.cmd,
            args: cmd.args,
        }),
        SubCommand::ListWindows(_) => Ok(Command::ListWindows),
        SubCommand::GetState(_) => Ok(Command::GetState),
        SubCommand::FocusedWindow(_) => Ok(Command::FocusedWindow),
        SubCommand::Exec(cmd) => Ok(Command::Exec {
            command: cmd.command,
        }),
        SubCommand::ExecOrFocus(cmd) => Ok(Command::ExecOrFocus {
            app_name: cmd.app_name,
            command: cmd.command,
        }),
        SubCommand::Quit(_) => Ok(Command::Quit),
    }
}

fn parse_command(args: &[String]) -> Result<Command> {
    if args.is_empty() {
        bail!("No command provided");
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];

    match cmd {
        "bind" => {
            if rest.len() < 2 {
                bail!("Usage: bind <key> <command> [args...]");
            }
            let key = rest[0].clone();
            let action = parse_command(&rest[1..].to_vec())?;
            Ok(Command::Bind {
                key,
                action: Box::new(action),
            })
        }
        "unbind" => {
            if rest.is_empty() {
                bail!("Usage: unbind <key>");
            }
            Ok(Command::Unbind {
                key: rest[0].clone(),
            })
        }
        "list-bindings" => Ok(Command::ListBindings),
        "view-tag" => {
            if rest.is_empty() {
                bail!("Usage: view-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::ViewTag { tag })
        }
        "toggle-view-tag" => {
            if rest.is_empty() {
                bail!("Usage: toggle-view-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::ToggleViewTag { tag })
        }
        "view-tag-last" => Ok(Command::ViewTagLast),
        "move-to-tag" => {
            if rest.is_empty() {
                bail!("Usage: move-to-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::MoveToTag { tag })
        }
        "toggle-window-tag" => {
            if rest.is_empty() {
                bail!("Usage: toggle-window-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::ToggleWindowTag { tag })
        }
        "focus-window" => {
            if rest.is_empty() {
                bail!("Usage: focus-window <direction>");
            }
            let direction = parse_direction(&rest[0])?;
            Ok(Command::FocusWindow { direction })
        }
        "swap-window" => {
            if rest.is_empty() {
                bail!("Usage: swap-window <direction>");
            }
            let direction = parse_direction(&rest[0])?;
            Ok(Command::SwapWindow { direction })
        }
        "zoom" => Ok(Command::Zoom),
        "focus-output" => {
            if rest.is_empty() {
                bail!("Usage: focus-output <next|prev>");
            }
            let direction = parse_output_direction(&rest[0])?;
            Ok(Command::FocusOutput { direction })
        }
        "send-to-output" => {
            if rest.is_empty() {
                bail!("Usage: send-to-output <next|prev>");
            }
            let direction = parse_output_direction(&rest[0])?;
            Ok(Command::SendToOutput { direction })
        }
        "retile" => Ok(Command::Retile),
        "layout-cmd" => {
            if rest.is_empty() {
                bail!("Usage: layout-cmd <cmd> [args...]");
            }
            Ok(Command::LayoutCommand {
                cmd: rest[0].clone(),
                args: rest[1..].to_vec(),
            })
        }
        "list-windows" => Ok(Command::ListWindows),
        "get-state" => Ok(Command::GetState),
        "focused-window" => Ok(Command::FocusedWindow),
        "exec" => {
            if rest.is_empty() {
                bail!("Usage: exec <command>");
            }
            Ok(Command::Exec {
                command: rest[0].clone(),
            })
        }
        "exec-or-focus" => {
            if rest.len() < 3 || rest[0] != "--app-name" {
                bail!("Usage: exec-or-focus --app-name <name> <command>");
            }
            let app_name = rest[1].clone();
            let command = rest[2].clone();
            Ok(Command::ExecOrFocus { app_name, command })
        }
        "quit" => Ok(Command::Quit),
        _ => bail!("Unknown command: {}", cmd),
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
