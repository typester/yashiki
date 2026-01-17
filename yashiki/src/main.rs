mod app;
mod core;
mod event;
mod ipc;
mod layout;
mod macos;

use anyhow::{bail, Result};
use ipc::IpcClient;
use std::env;
use tracing_subscriber::EnvFilter;
use yashiki_ipc::{Command, Direction, OutputDirection, Response};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        // No arguments - run daemon
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();

        tracing::info!("yashiki starting");
        app::App::run()
    } else {
        // CLI mode - send command to daemon
        run_cli(&args[1..])
    }
}

fn run_cli(args: &[String]) -> Result<()> {
    let cmd = parse_command(args)?;
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

fn parse_command(args: &[String]) -> Result<Command> {
    if args.is_empty() {
        bail!("No command provided");
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];

    match cmd {
        "bind" => {
            if rest.len() < 2 {
                bail!("Usage: yashiki bind <key> <command> [args...]");
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
                bail!("Usage: yashiki unbind <key>");
            }
            Ok(Command::Unbind {
                key: rest[0].clone(),
            })
        }
        "list-bindings" => Ok(Command::ListBindings),
        "view-tag" => {
            if rest.is_empty() {
                bail!("Usage: yashiki view-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::ViewTag { tag })
        }
        "toggle-view-tag" => {
            if rest.is_empty() {
                bail!("Usage: yashiki toggle-view-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::ToggleViewTag { tag })
        }
        "view-tag-last" => Ok(Command::ViewTagLast),
        "move-to-tag" => {
            if rest.is_empty() {
                bail!("Usage: yashiki move-to-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::MoveToTag { tag })
        }
        "toggle-window-tag" => {
            if rest.is_empty() {
                bail!("Usage: yashiki toggle-window-tag <tag>");
            }
            let tag: u32 = rest[0].parse()?;
            Ok(Command::ToggleWindowTag { tag })
        }
        "focus-window" => {
            if rest.is_empty() {
                bail!("Usage: yashiki focus-window <direction>");
            }
            let direction = parse_direction(&rest[0])?;
            Ok(Command::FocusWindow { direction })
        }
        "swap-window" => {
            if rest.is_empty() {
                bail!("Usage: yashiki swap-window <direction>");
            }
            let direction = parse_direction(&rest[0])?;
            Ok(Command::SwapWindow { direction })
        }
        "zoom" => Ok(Command::Zoom),
        "focus-output" => {
            if rest.is_empty() {
                bail!("Usage: yashiki focus-output <next|prev>");
            }
            let direction = parse_output_direction(&rest[0])?;
            Ok(Command::FocusOutput { direction })
        }
        "send-to-output" => {
            if rest.is_empty() {
                bail!("Usage: yashiki send-to-output <next|prev>");
            }
            let direction = parse_output_direction(&rest[0])?;
            Ok(Command::SendToOutput { direction })
        }
        "retile" => Ok(Command::Retile),
        "layout-cmd" => {
            if rest.is_empty() {
                bail!("Usage: yashiki layout-cmd <cmd> [args...]");
            }
            Ok(Command::LayoutCommand {
                cmd: rest[0].clone(),
                args: rest[1..].to_vec(),
            })
        }
        "list-windows" => Ok(Command::ListWindows),
        "get-state" => Ok(Command::GetState),
        "focused-window" => Ok(Command::FocusedWindow),
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
        _ => bail!("Unknown direction: {}", s),
    }
}

fn parse_output_direction(s: &str) -> Result<OutputDirection> {
    match s.to_lowercase().as_str() {
        "next" => Ok(OutputDirection::Next),
        "prev" => Ok(OutputDirection::Prev),
        _ => bail!("Unknown output direction: {} (use next or prev)", s),
    }
}
