use colored::Colorize;

use crate::cli::{Cli, DaemonCommands};
use crate::commands::browser::effective_profile_name;
use crate::config::Config;
use crate::error::Result;

pub async fn run(cli: &Cli, command: &DaemonCommands) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = (cli, command);
        return Err(crate::error::ActionbookError::FeatureNotSupported(
            "Daemon mode is only supported on Unix (macOS/Linux)".to_string(),
        ));
    }

    #[cfg(unix)]
    {
        run_unix(cli, command).await
    }
}

#[cfg(unix)]
async fn run_unix(cli: &Cli, command: &DaemonCommands) -> Result<()> {
    use crate::daemon::{lifecycle, server};

    let config = Config::load()?;
    let profile = effective_profile_name(cli, &config).to_string();

    match command {
        DaemonCommands::Serve { profile: prof_override } => {
            let profile = prof_override
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&profile);
            server::run(profile).await
        }
        DaemonCommands::Status => {
            let alive = lifecycle::is_daemon_alive(&profile).await;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "profile": profile,
                        "running": alive,
                        "socket": lifecycle::socket_path(&profile).display().to_string(),
                        "pid_file": lifecycle::pid_path(&profile).display().to_string(),
                    })
                );
            } else if alive {
                println!(
                    "{} Daemon for profile '{}' is {}",
                    "●".green(),
                    profile,
                    "running".green()
                );
                println!(
                    "  Socket: {}",
                    lifecycle::socket_path(&profile).display()
                );
            } else {
                println!(
                    "{} Daemon for profile '{}' is {}",
                    "○".dimmed(),
                    profile,
                    "not running".dimmed()
                );
            }
            Ok(())
        }
        DaemonCommands::Stop => {
            lifecycle::stop_daemon(&profile).await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "profile": profile,
                        "stopped": true,
                    })
                );
            } else {
                println!("Daemon for profile '{}' stopped", profile);
            }
            Ok(())
        }
    }
}
