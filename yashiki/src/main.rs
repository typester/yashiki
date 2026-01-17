mod core;
mod macos;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("yashiki starting");

    if !macos::is_trusted() {
        tracing::warn!("Accessibility permission not granted, requesting...");
        macos::is_trusted_with_prompt();
        anyhow::bail!("Please grant Accessibility permission and restart");
    }

    tracing::info!("Accessibility permission granted");

    println!("\n=== On-screen windows (CGWindowList) ===\n");
    let windows = macos::get_on_screen_windows();
    for w in &windows {
        println!(
            "[{}] {} - {:?} @ ({:.0}, {:.0}) {}x{}",
            w.pid,
            w.owner_name,
            w.name,
            w.bounds.x,
            w.bounds.y,
            w.bounds.width as u32,
            w.bounds.height as u32
        );
    }

    println!("\n=== Windows via Accessibility API ===\n");
    let pids = macos::get_running_app_pids();
    for pid in pids {
        let app = macos::AXUIElement::application(pid);
        let Ok(app_windows) = app.windows() else {
            continue;
        };

        for win in app_windows {
            let title = win.title().unwrap_or_default();
            let pos = win.position().ok();
            let size = win.size().ok();
            let minimized = win.is_minimized().unwrap_or(false);

            if minimized {
                continue;
            }

            println!(
                "[{}] {:?} @ {:?} {:?}",
                pid,
                title,
                pos.map(|p| (p.x as i32, p.y as i32)),
                size.map(|s| (s.width as u32, s.height as u32)),
            );
        }
    }

    Ok(())
}
