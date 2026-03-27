use std::time::Instant;

use clap::Parser;
use serde_json::json;

use actionbook_cli::action::Action;
use actionbook_cli::action_result::ActionResult;
use actionbook_cli::cli::{BrowserCommands, Cli, Commands};
use actionbook_cli::output::{self, JsonEnvelope, ResponseContext};
use actionbook_cli::utils::client::DaemonClient;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Internal: daemon auto-start passes a hidden arg before clap parsing
    if std::env::args().nth(1).as_deref() == Some("__serve") {
        if let Err(e) = actionbook_cli::commands::daemon::server::run_daemon().await {
            eprintln!("daemon error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let cli = Cli::parse();
    let json_output = cli.json;

    // Handle --version before subcommand dispatch
    if cli.version {
        handle_version(json_output);
        return;
    }

    if cli.command.is_none() {
        eprintln!("error: no subcommand provided. Run `actionbook --help` for usage.");
        std::process::exit(1);
    }

    let result = run(cli).await;

    match result {
        Ok(()) => {}
        Err(e) => {
            if json_output {
                let envelope = JsonEnvelope::error(
                    "unknown",
                    None,
                    "INTERNAL_ERROR",
                    &e.to_string(),
                    false,
                    serde_json::Value::Null,
                    "",
                    std::time::Duration::ZERO,
                );
                println!(
                    "{}",
                    serde_json::to_string(&envelope).unwrap_or_default()
                );
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(1);
        }
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let json_mode = cli.json;

    match cli.command.unwrap() {
        Commands::Browser { command } => {
            handle_browser(command, json_mode).await?;
        }
        Commands::Help => {
            handle_help(json_mode);
        }
    }
    Ok(())
}

async fn handle_browser(
    command: BrowserCommands,
    json_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    // Build action from CLI args
    let action = build_action(&command);
    let command_name = action.command_name().to_string();

    // Connect to daemon and execute
    let mut client = DaemonClient::connect().await?;
    let result = client.send_action(&action).await?;

    let duration = start.elapsed();

    // Build context based on command type and result
    let context = build_context(&command, &result);

    if json_mode {
        let envelope = JsonEnvelope::from_result(&command_name, context.clone(), &result, duration);
        println!("{}", serde_json::to_string(&envelope)?);
    } else {
        let text = output::format_text(&command_name, &context, &result);
        println!("{text}");
    }

    if !result.is_ok() {
        std::process::exit(1);
    }

    Ok(())
}

fn build_action(command: &BrowserCommands) -> Action {
    match command {
        BrowserCommands::Start {
            mode,
            headless,
            profile,
            open_url,
            cdp_endpoint,
            set_session_id,
            ..
        } => Action::StartSession {
            mode: mode.clone().into(),
            headless: *headless,
            profile: profile.clone(),
            open_url: open_url.clone(),
            cdp_endpoint: cdp_endpoint.clone(),
            set_session_id: set_session_id.clone(),
        },
        BrowserCommands::ListSessions => Action::ListSessions,
        BrowserCommands::Status { session } => Action::SessionStatus {
            session_id: session.clone(),
        },
        BrowserCommands::Close { session } => Action::Close {
            session_id: session.clone(),
        },
        BrowserCommands::Restart { session } => Action::Restart {
            session_id: session.clone(),
        },
        BrowserCommands::Goto {
            url,
            session,
            tab,
        } => Action::Goto {
            session_id: session.clone(),
            tab_id: tab.clone(),
            url: url.clone(),
        },
        BrowserCommands::NewTab {
            url,
            session,
            new_window,
        }
        | BrowserCommands::Open {
            url,
            session,
            new_window,
        } => Action::NewTab {
            session_id: session.clone(),
            url: url.clone(),
            new_window: *new_window,
        },
        BrowserCommands::CloseTab { session, tab } => Action::CloseTab {
            session_id: session.clone(),
            tab_id: tab.clone(),
        },
        BrowserCommands::ListTabs { session } => Action::ListTabs {
            session_id: session.clone(),
        },
        BrowserCommands::Snapshot { session, tab } => Action::Snapshot {
            session_id: session.clone(),
            tab_id: tab.clone(),
        },
        BrowserCommands::Eval {
            expression,
            session,
            tab,
        } => Action::Eval {
            session_id: session.clone(),
            tab_id: tab.clone(),
            expression: expression.clone(),
        },
    }
}

fn build_context(
    command: &BrowserCommands,
    result: &ActionResult,
) -> Option<ResponseContext> {
    match command {
        // Global commands that create a session return context
        BrowserCommands::Start { .. } => {
            if let ActionResult::Ok { data } = result {
                Some(ResponseContext {
                    session_id: data["session"]["session_id"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    tab_id: Some(
                        data["tab"]["tab_id"]
                            .as_str()
                            .unwrap_or("t1")
                            .to_string(),
                    ),
                    window_id: None,
                    url: data["tab"]["url"].as_str().map(|s| s.to_string()),
                    title: data["tab"]["title"].as_str().map(|s| s.to_string()),
                })
            } else {
                None
            }
        }
        // Global commands with no session
        BrowserCommands::ListSessions => None,
        // Session-level commands
        BrowserCommands::Status { session }
        | BrowserCommands::Close { session }
        | BrowserCommands::Restart { session } => {
            let mut ctx = ResponseContext {
                session_id: session.clone(),
                tab_id: None,
                window_id: None,
                url: None,
                title: None,
            };
            // restart returns tab info per §7.5
            if matches!(command, BrowserCommands::Restart { .. }) {
                if let ActionResult::Ok { data } = result {
                    if let Some(tab_id) = data
                        .pointer("/session/tab_id")
                        .or_else(|| data.pointer("/tab/tab_id"))
                        .and_then(|v| v.as_str())
                    {
                        ctx.tab_id = Some(tab_id.to_string());
                    } else {
                        ctx.tab_id = Some("t1".to_string());
                    }
                }
            }
            Some(ctx)
        }
        // Tab-level commands
        BrowserCommands::Goto { session, tab, .. }
        | BrowserCommands::Snapshot { session, tab }
        | BrowserCommands::Eval {
            session, tab, ..
        }
        | BrowserCommands::CloseTab { session, tab } => Some(ResponseContext {
            session_id: session.clone(),
            tab_id: Some(tab.clone()),
            window_id: None,
            url: None,
            title: None,
        }),
        BrowserCommands::NewTab { session, .. }
        | BrowserCommands::Open { session, .. }
        | BrowserCommands::ListTabs { session } => Some(ResponseContext {
            session_id: session.clone(),
            tab_id: None,
            window_id: None,
            url: None,
            title: None,
        }),
    }
}

fn handle_version(json_mode: bool) {
    let version = env!("CARGO_PKG_VERSION");
    if json_mode {
        let envelope = JsonEnvelope::success("version", None, json!(version), std::time::Duration::ZERO);
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    } else {
        println!("{version}");
    }
}

fn handle_help(json_mode: bool) {
    let help_text = "actionbook browser <subcommand>\n\nstart         Start or attach a browser session\nlist-sessions List all active sessions\nstatus        Show session status\nclose         Close a session\nrestart       Restart a session\nlist-tabs     List tabs in a session\nnew-tab       Open a new tab\ngoto          Navigate to URL\nsnapshot      Capture accessibility snapshot\neval          Evaluate JavaScript";

    if json_mode {
        let envelope = JsonEnvelope::success("help", None, json!(help_text), std::time::Duration::ZERO);
        println!(
            "{}",
            serde_json::to_string(&envelope).unwrap_or_default()
        );
    } else {
        println!("{help_text}");
    }
}

