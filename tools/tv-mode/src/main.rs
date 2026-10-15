mod config;
mod control;
mod library;
mod oblecto;
mod platform;
mod player;
mod session;
mod ui;

use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use config::{Config, default_config_path};
use control::ControlCommand;

#[derive(Debug, Parser)]
#[command(name = "tv-mode")]
#[command(about = "Controller-first TV session for Hyprland")]
struct Cli {
    #[arg(long, value_name = "PATH", default_value_os_t = default_config_path())]
    config: PathBuf,
    #[command(subcommand)]
    command: CommandName,
}

#[derive(Debug, Subcommand)]
enum CommandName {
    /// Start TV mode or bring its home screen forward
    Start,
    /// Bring the active TV home screen forward
    Home,
    /// Report whether TV mode has an active state snapshot
    Status,
    /// Exit TV mode after the foreground entertainment app closes
    Exit,
    /// Restore a stale or crashed TV mode session
    Recover,
    /// Run the TV shell process (used by tv-mode.service)
    #[command(hide = true)]
    Run,
    /// Preview the TV interface without changing displays, audio, or power
    Preview {
        /// Disable physical controllers when running an isolated UI test
        #[arg(long)]
        no_controller: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        CommandName::Start => start_service(),
        CommandName::Home => control::send(ControlCommand::Show, Duration::from_secs(1)),
        CommandName::Status => {
            println!(
                "{}",
                if session::is_active()? {
                    "active"
                } else {
                    "inactive"
                }
            );
            Ok(())
        }
        CommandName::Exit => control::send(ControlCommand::Exit, Duration::from_secs(1)),
        CommandName::Recover => {
            let config = Config::load(&cli.config)?;
            if session::recover(&config)? {
                println!("restored TV mode session");
            }
            Ok(())
        }
        CommandName::Run => {
            let config = Config::load(&cli.config)?;
            if session::is_active()? {
                session::recover(&config).context("failed to recover stale TV mode state")?;
            }
            let session = session::Session::enter(config)?;
            ui::run(session)
        }
        CommandName::Preview { no_controller } => {
            ui::preview(Config::load(&cli.config)?, !no_controller)
        }
    }
}

fn start_service() -> Result<()> {
    if session::is_active()? {
        return control::send(ControlCommand::Show, Duration::from_secs(1));
    }
    let status = Command::new("systemctl")
        .args(["--user", "start", "tv-mode.service"])
        .status()
        .context("failed to start tv-mode.service")?;
    if !status.success() {
        anyhow::bail!("systemctl failed to start tv-mode.service");
    }
    control::send(ControlCommand::Show, Duration::from_secs(4))
}
