use clap::{Parser, Subcommand};
use hollowrite_persistence::Db;
use hollowrite_server::{Config, Limits, Server};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "hollowrite-server", version, about = "Authoritative HOLLOWRITE game server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the game server (default).
    Serve,
    /// Apply database migrations and exit.
    Migrate,
    /// Validate the content directory and exit.
    CheckContent,
    /// Moderation and account administration.
    Admin {
        #[command(subcommand)]
        action: Admin,
    },
}

#[derive(Subcommand)]
enum Admin {
    /// Suspend an account and revoke its sessions.
    Suspend {
        username: String,
        #[arg(long, default_value_t = 24)]
        hours: i64,
        #[arg(long, default_value = "terms of play")]
        reason: String,
    },
    /// Lift a suspension.
    Unsuspend { username: String },
    /// Grant or revoke the admin flag.
    Promote {
        username: String,
        #[arg(long)]
        revoke: bool,
    },
    /// List unresolved player reports.
    Reports,
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=warn"));
    let json = std::env::var("HOLLOWRITE_LOG_FORMAT").is_ok_and(|v| v == "json");
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_target(true);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env()?;
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
            let limits = Limits::from_config(&config);
            let server = Server::start(config, limits).await?;
            shutdown_signal().await;
            server.shutdown().await;
        }
        Command::Migrate => {
            let db = Db::connect(&config.database_url, 2).await?;
            db.migrate().await?;
            println!("migrations applied");
        }
        Command::CheckContent => {
            let content = hollowrite_game_core::Content::load(&config.content_dir)?;
            println!(
                "content ok: {} items, {} enemies, {} npcs, map {}x{} with {} spawns",
                content.items.len(),
                content.enemies.len(),
                content.npcs.len(),
                content.map.width,
                content.map.height,
                content.map.spawns.len()
            );
        }
        Command::Admin { action } => {
            let db = Db::connect(&config.database_url, 2).await?;
            match action {
                Admin::Suspend { username, hours, reason } => {
                    let until = chrono::Utc::now() + chrono::Duration::hours(hours);
                    db.suspend(&username, Some(until), &reason, None).await?;
                    println!("{username} suspended until {until}");
                }
                Admin::Unsuspend { username } => {
                    db.suspend(&username, None, "lifted", None).await?;
                    println!("{username} unsuspended");
                }
                Admin::Promote { username, revoke } => {
                    db.set_admin(&username, !revoke).await?;
                    println!("{username} admin = {}", !revoke);
                }
                Admin::Reports => {
                    for (id, from, against, reason, at) in db.open_reports().await? {
                        println!("#{id} {at} {from} -> {against}: {reason}");
                    }
                }
            }
        }
    }
    Ok(())
}
