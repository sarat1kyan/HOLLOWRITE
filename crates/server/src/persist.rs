use std::sync::Arc;
use std::time::{Duration, Instant};

use hollowrite_game_core::world::SaveRequest;
use hollowrite_game_core::CharacterData;
use hollowrite_persistence::{Db, DbResult};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::metrics::Metrics;

pub enum PersistCmd {
    Save(Box<SaveRequest>),
    Load { account: Uuid, character: String, reply: oneshot::Sender<DbResult<Option<CharacterData>>> },
    Flush { reply: oneshot::Sender<()> },
}

/// Single FIFO writer for character state. Loads go through the same queue
/// so a character can never be read back before its last save landed.
#[derive(Clone)]
pub struct Persist {
    tx: mpsc::UnboundedSender<PersistCmd>,
}

impl Persist {
    pub fn spawn(db: Db, metrics: Arc<Metrics>) -> (Persist, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(run(db, rx, metrics));
        (Persist { tx }, handle)
    }

    pub fn save(&self, req: Box<SaveRequest>) {
        if self.tx.send(PersistCmd::Save(req)).is_err() {
            tracing::error!("persistence queue closed; save dropped");
        }
    }

    pub async fn load(&self, account: Uuid, character: String) -> DbResult<Option<CharacterData>> {
        let (reply, rx) = oneshot::channel();
        let _ = self.tx.send(PersistCmd::Load { account, character, reply });
        rx.await.unwrap_or(Ok(None))
    }

    pub async fn flush(&self) {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(PersistCmd::Flush { reply }).is_ok() {
            let _ = rx.await;
        }
    }
}

async fn run(db: Db, mut rx: mpsc::UnboundedReceiver<PersistCmd>, metrics: Arc<Metrics>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            PersistCmd::Save(req) => save_with_retry(&db, &req, &metrics).await,
            PersistCmd::Load { account, character, reply } => {
                let _ = reply.send(db.load_character(account, &character).await);
            }
            PersistCmd::Flush { reply } => {
                let _ = reply.send(());
            }
        }
    }
}

async fn save_with_retry(db: &Db, req: &SaveRequest, metrics: &Metrics) {
    let mut delay = Duration::from_millis(100);
    for attempt in 1..=6 {
        let start = Instant::now();
        match db.save_character(req).await {
            Ok(applied) => {
                metrics.db_us_last.store(start.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
                if applied {
                    Metrics::inc(&metrics.saves_ok);
                } else {
                    Metrics::inc(&metrics.saves_stale);
                }
                tracing::debug!(character = %req.data.id, version = req.data.version, reason = req.reason, applied, "saved");
                return;
            }
            Err(e) => {
                Metrics::inc(&metrics.saves_failed);
                tracing::warn!(character = %req.data.id, attempt, error = %e, "save failed");
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
        }
    }
    tracing::error!(character = %req.data.id, version = req.data.version, "giving up on save; the next save will supersede it");
}
