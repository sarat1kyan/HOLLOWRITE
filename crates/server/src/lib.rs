pub mod config;
pub mod game;
pub mod http;
pub mod metrics;
pub mod persist;
pub mod ratelimit;
pub mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use hollowrite_game_core::Content;
use hollowrite_persistence::Db;
use tokio::sync::oneshot;

pub use config::Config;
use game::{ToWorld, WorldHandle};
use metrics::Metrics;
use persist::Persist;
use ratelimit::Limiter;

pub struct Limits {
    pub register: Limiter,
    pub login_ip: Limiter,
    pub login_user: Limiter,
    pub ws_connect: Limiter,
}

impl Limits {
    pub fn from_config(config: &Config) -> Self {
        let per_hour = config.registrations_per_hour.max(1.0);
        Limits {
            register: Limiter::new(per_hour, per_hour / 3600.0),
            login_ip: Limiter::new(20.0, 20.0 / 60.0),
            login_user: Limiter::new(8.0, 8.0 / 60.0),
            ws_connect: Limiter::new(config.connect_burst.max(1.0), 1.0),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub content: Arc<Content>,
    pub world: WorldHandle,
    pub persist: Persist,
    pub metrics: Arc<Metrics>,
    pub limits: Arc<Limits>,
    pub dummy_hash: String,
}

/// A running server. Dropping it does not stop it; call [`Server::shutdown`].
pub struct Server {
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    done: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(config: Config, limits: Limits) -> anyhow::Result<Server> {
        let content = Arc::new(Content::load(&config.content_dir)?);
        let db = Db::connect(&config.database_url, config.db_max_connections).await?;
        db.migrate().await?;
        let metrics = Arc::new(Metrics::default());
        let (persist, persist_task) = Persist::spawn(db.clone(), metrics.clone());
        let (world, world_task) =
            game::spawn(content.clone(), config.seed, persist.clone(), db.clone(), metrics.clone());
        let dummy_hash = http::hash_password("not-a-real-password-0".into()).await?;
        let state = AppState {
            config: Arc::new(config.clone()),
            db: db.clone(),
            content,
            world: world.clone(),
            persist: persist.clone(),
            metrics,
            limits: Arc::new(limits),
            dummy_hash,
        };
        let listener = tokio::net::TcpListener::bind(config.bind).await?;
        let addr = listener.local_addr()?;
        let app = http::router(state).into_make_service_with_connect_info::<SocketAddr>();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();

        let watcher = {
            let db = db.clone();
            let world = world.clone();
            tokio::spawn(async move {
                let mut every = tokio::time::interval(Duration::from_secs(15));
                loop {
                    every.tick().await;
                    match db.suspended_accounts().await {
                        Ok(ids) if !ids.is_empty() => {
                            world.send(ToWorld::KickAccounts(ids)).await;
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!(error = %e, "suspension poll failed"),
                    }
                    let _ = db.purge_expired_sessions().await;
                }
            })
        };

        let done = tokio::spawn(async move {
            let shutdown_world = world.clone();
            // Players are kicked and saved before the listener drains, otherwise
            // open WebSockets would keep the graceful shutdown waiting forever.
            let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = stop_rx.await;
                tracing::info!("shutdown requested; saving players");
                let (reply, saved) = oneshot::channel();
                if shutdown_world.send(ToWorld::Shutdown { reply }).await {
                    let _ = saved.await;
                }
            });
            if let Err(e) = serve.await {
                tracing::error!(error = %e, "http server failed");
            }
            watcher.abort();
            drop(world);
            let _ = world_task.await;
            persist.flush().await;
            drop(persist);
            let _ = tokio::time::timeout(Duration::from_secs(10), persist_task).await;
            tracing::info!("shutdown complete");
        });
        tracing::info!(%addr, "listening");
        Ok(Server { addr, stop: Some(stop_tx), done })
    }

    /// Stops accepting connections, saves every online character and waits
    /// for the persistence queue to drain.
    pub async fn shutdown(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = self.done.await;
    }

    pub async fn wait(self) {
        let _ = self.done.await;
    }
}
