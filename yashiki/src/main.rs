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

    println!("\n=== Move focused window test ===\n");
    if let Ok(win) = macos::get_focused_window() {
        let title = win.title().unwrap_or_default();
        let Ok(pos) = win.position() else {
            println!("Could not get position");
            return Ok(());
        };
        println!(
            "Focused: {:?} @ ({}, {})",
            title, pos.x as i32, pos.y as i32
        );

        let new_pos = core_graphics::geometry::CGPoint::new(pos.x + 100.0, pos.y);
        println!("Moving to ({}, {})...", new_pos.x as i32, new_pos.y as i32);
        if win.set_position(new_pos).is_err() {
            println!("Failed to set position");
            return Ok(());
        }

        std::thread::sleep(std::time::Duration::from_secs(1));

        let Ok(final_pos) = win.position() else {
            println!("Could not get final position");
            return Ok(());
        };
        println!(
            "Final position: ({}, {})",
            final_pos.x as i32, final_pos.y as i32
        );

        if (final_pos.x - new_pos.x).abs() < 1.0 {
            println!("Success! Window moved.");
        } else {
            println!("Window was moved back (probably by AeroSpace)");
        }
    } else {
        println!("Could not get focused window");
    }

    Ok(())
}
