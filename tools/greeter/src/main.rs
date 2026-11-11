mod auth;
mod config;
mod session;
mod transition;
mod ui;

use clap::Parser;
use gtk4::{gio, prelude::*};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Native Wayland greeter for the desktop shell")]
struct Args {
    #[arg(long, default_value = "/etc/shell-greeter/config.toml")]
    config: PathBuf,
    /// Validate configuration and discover sessions without opening a display/socket.
    #[arg(long)]
    check_config: bool,
    /// Isolated demo. Never authenticates, starts sessions, writes state, or powers off.
    #[arg(long)]
    preview: bool,
    /// Cover Hyprland startup, then reveal its ready wallpaper and bar. No authentication.
    #[arg(long, conflicts_with_all = ["preview", "check_config"])]
    desktop_reveal: bool,
}
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.desktop_reveal {
        let app = gtk4::Application::builder()
            .application_id("me.robindecker.SessionReveal")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(transition::desktop);
        app.run_with_args::<&str>(&[]);
        return Ok(());
    }
    let config = config::Config::load(&args.config)?;
    let sessions = session::discover(&config.session_dirs);
    if args.check_config {
        anyhow::ensure!(!sessions.is_empty(), "No usable desktop sessions found");
        for s in &sessions {
            println!(
                "{}: {} ({})",
                s.id,
                s.name,
                if s.x11 { "X11" } else { "Wayland" }
            );
        }
        return Ok(());
    }
    let app = gtk4::Application::builder()
        .application_id("me.robindecker.ShellGreeter")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| ui::start(app, config.clone(), sessions.clone(), args.preview));
    app.run_with_args::<&str>(&[]);
    Ok(())
}
