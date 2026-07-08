use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "cockpit-bar")]
#[command(about = "Wayland command cockpit bar")]
struct Cli {
    #[arg(long, value_name = "PATH", default_value_os_t = default_config_path())]
    config: PathBuf,
}

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cockpit-bar")
        .join("config.toml")
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let cli = Cli::parse();
    match cockpit_bar::run(&cli.config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}
