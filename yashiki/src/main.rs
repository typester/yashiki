mod app;
mod core;
mod event;
mod ipc;
mod layout;
mod macos;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("yashiki starting");
    app::App::run()
}
