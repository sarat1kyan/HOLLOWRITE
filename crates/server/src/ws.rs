use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use hollowrite_game_core::CharacterData;
use hollowrite_persistence::{DbError, SessionInfo};
use hollowrite_protocol::{validate_name, ClientMsg, ErrorCode, ServerMsg, PROTOCOL_VERSION};
use tokio::sync::{mpsc, oneshot};

use crate::game::{outbound_channel, ConnId, Outbound, ToWorld};
use crate::metrics::Metrics;
use crate::ratelimit::Bucket;
use crate::AppState;

static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const PING_EVERY: Duration = Duration::from_secs(10);
const MAX_STRIKES: u32 = 50;

#[derive(PartialEq, Eq)]
enum Phase {
    Hello,
    Lobby,
    World,
}

fn encode(msg: &ServerMsg) -> String {
    serde_json::to_string(msg).expect("server messages always serialize")
}

struct Session {
    conn: ConnId,
    state: AppState,
    auth: SessionInfo,
    tx: mpsc::Sender<Outbound>,
    phase: Phase,
    strikes: u32,
}

impl Session {
    async fn reply(&self, msg: ServerMsg) {
        let _ = self.tx.send(Outbound::Frame(encode(&msg))).await;
    }

    async fn error(&self, code: ErrorCode, message: impl Into<String>) {
        self.reply(ServerMsg::Error { code, message: message.into() }).await;
    }

    async fn characters(&self) {
        match self.state.db.list_characters(&self.state.content, self.auth.account_id).await {
            Ok(list) => self.reply(ServerMsg::Characters { list }).await,
            Err(e) => {
                tracing::error!(error = %e, "listing characters failed");
                self.error(ErrorCode::Internal, "could not list characters").await;
            }
        }
    }

    async fn on_message(&mut self, msg: ClientMsg) -> bool {
        match (&self.phase, msg) {
            (Phase::Hello, ClientMsg::Hello { protocol, client, build }) => {
                if protocol != PROTOCOL_VERSION {
                    self.error(
                        ErrorCode::ProtocolMismatch,
                        format!("server speaks protocol {PROTOCOL_VERSION}, client speaks {protocol}; please update"),
                    )
                    .await;
                    return false;
                }
                tracing::info!(conn = self.conn, account = %self.auth.account_id, ?client, build = %build.chars().take(32).collect::<String>(), "hello");
                self.phase = Phase::Lobby;
                self.reply(ServerMsg::Welcome {
                    protocol: PROTOCOL_VERSION,
                    server: format!("hollowrite {}", env!("CARGO_PKG_VERSION")),
                    tick_hz: self.state.content.balance.tick_hz,
                    snapshot_hz: self.state.content.balance.tick_hz / self.state.content.balance.snapshot_every,
                    account: self.auth.username.clone(),
                })
                .await;
            }
            (Phase::Hello, _) => {
                self.error(ErrorCode::BadRequest, "say hello first").await;
                return false;
            }
            (_, ClientMsg::Hello { .. }) => {
                self.strikes += 1;
            }
            (_, ClientMsg::Ping { nonce }) if self.phase == Phase::Lobby => {
                self.reply(ServerMsg::Pong { nonce, tick: 0 }).await;
            }
            (Phase::Lobby, ClientMsg::ListCharacters) => self.characters().await,
            (Phase::Lobby, ClientMsg::CreateCharacter { name, archetype, look }) => {
                if let Err(e) = validate_name(&name) {
                    self.error(ErrorCode::InvalidName, e).await;
                    return true;
                }
                if !look.is_valid() {
                    self.error(ErrorCode::BadRequest, "invalid appearance").await;
                    return true;
                }
                let data = CharacterData::new(
                    &self.state.content,
                    uuid::Uuid::new_v4().to_string(),
                    self.auth.account_id.to_string(),
                    name,
                    archetype,
                    look,
                );
                match self.state.db.insert_character(&data).await {
                    Ok(()) => {
                        tracing::info!(account = %self.auth.account_id, character = %data.name, "character created");
                        self.characters().await;
                    }
                    Err(DbError::Conflict) => {
                        self.error(ErrorCode::NameTaken, "that name is already carried by another").await
                    }
                    Err(DbError::Limit) => {
                        self.error(ErrorCode::TooManyCharacters, "an account may keep four characters").await
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "creating character failed");
                        self.error(ErrorCode::Internal, "could not create the character").await;
                    }
                }
            }
            (Phase::Lobby, ClientMsg::SelectCharacter { id }) => {
                let loaded = self.state.persist.load(self.auth.account_id, id).await;
                let data = match loaded {
                    Ok(Some(d)) => d,
                    Ok(None) => {
                        self.error(ErrorCode::NotFound, "no such character on this account").await;
                        return true;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "loading character failed");
                        self.error(ErrorCode::Internal, "could not load the character").await;
                        return true;
                    }
                };
                let mutes = self.state.db.mutes(self.auth.account_id).await.unwrap_or_default();
                let (reply, done) = oneshot::channel();
                let join = ToWorld::Join {
                    conn: self.conn,
                    account: self.auth.account_id,
                    data: Box::new(data),
                    tx: self.tx.clone(),
                    mutes,
                    reply,
                };
                if !self.state.world.send(join).await || done.await.is_err() {
                    self.error(ErrorCode::Busy, "the world is not accepting players").await;
                    return false;
                }
                self.phase = Phase::World;
            }
            (Phase::Lobby, _) => {
                self.strikes += 1;
                self.error(ErrorCode::BadRequest, "choose a character first").await;
            }
            (Phase::World, ClientMsg::Input { seq, mv, act, face }) => {
                if !self.state.world.try_send(ToWorld::Input { conn: self.conn, seq, mv, act, face }) {
                    Metrics::inc(&self.state.metrics.frames_rejected);
                }
            }
            (Phase::World, ClientMsg::Leave) => {
                self.state.world.send(ToWorld::Leave { conn: self.conn }).await;
                self.state.persist.flush().await;
                self.phase = Phase::Lobby;
                self.characters().await;
            }
            (
                Phase::World,
                ClientMsg::ListCharacters | ClientMsg::CreateCharacter { .. } | ClientMsg::SelectCharacter { .. },
            ) => {
                self.strikes += 1;
                self.error(ErrorCode::BadRequest, "leave the world first").await;
            }
            (Phase::World, msg) => {
                self.state.world.send(ToWorld::Msg { conn: self.conn, msg }).await;
            }
        }
        true
    }
}

pub async fn serve(socket: WebSocket, state: AppState, auth: SessionInfo, addr: SocketAddr) {
    let conn = NEXT_CONN.fetch_add(1, Ordering::Relaxed);
    let metrics = state.metrics.clone();
    metrics.connections.fetch_add(1, Ordering::Relaxed);
    tracing::info!(conn, account = %auth.account_id, peer = %addr, "connected");

    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = outbound_channel();
    let (closed_tx, mut closed_rx) = tokio::sync::watch::channel(false);

    let writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(PING_EVERY);
        ping.tick().await;
        loop {
            tokio::select! {
                out = rx.recv() => match out {
                    Some(Outbound::Frame(text)) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Outbound::Close(reason)) => {
                        let reason: String = reason.chars().take(100).collect();
                        let _ = sink.send(Message::Close(Some(CloseFrame { code: 4000, reason: reason.into() }))).await;
                        break;
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = sink.close().await;
        let _ = closed_tx.send(true);
    });

    let mut session = Session { conn, state: state.clone(), auth, tx: tx.clone(), phase: Phase::Hello, strikes: 0 };
    let mut rate = Bucket::new(90.0, 60.0);
    loop {
        let wait = if session.phase == Phase::Hello { HELLO_TIMEOUT } else { IDLE_TIMEOUT };
        let read = tokio::select! {
            r = tokio::time::timeout(wait, stream.next()) => r,
            _ = closed_rx.changed() => break,
        };
        let next = match read {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(e))) => {
                tracing::debug!(conn, error = %e, "socket error");
                break;
            }
            Ok(None) => break,
            Err(_) => {
                tracing::info!(conn, "idle timeout");
                let _ = tx.send(Outbound::Close("idle timeout".into())).await;
                break;
            }
        };
        let text = match next {
            Message::Text(t) => t,
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Binary(_) => {
                session.strikes += 1;
                continue;
            }
        };
        Metrics::inc(&metrics.frames_in);
        if !rate.take() {
            Metrics::inc(&metrics.frames_rejected);
            session.strikes += 1;
            if session.strikes > MAX_STRIKES {
                tracing::warn!(conn, "flooding; disconnecting");
                break;
            }
            continue;
        }
        let msg = match serde_json::from_str::<ClientMsg>(&text) {
            Ok(m) => m,
            Err(_) => {
                Metrics::inc(&metrics.frames_rejected);
                session.strikes += 1;
                tracing::debug!(conn, "malformed frame");
                if session.strikes > MAX_STRIKES {
                    break;
                }
                session.error(ErrorCode::BadRequest, "malformed message").await;
                continue;
            }
        };
        if !session.on_message(msg).await {
            break;
        }
        if session.strikes > MAX_STRIKES {
            tracing::warn!(conn, "too many protocol violations; disconnecting");
            break;
        }
    }

    if session.phase == Phase::World {
        state.world.send(ToWorld::Leave { conn }).await;
    }
    drop(session);
    drop(tx);
    let _ = writer.await;
    metrics.connections.fetch_sub(1, Ordering::Relaxed);
    tracing::info!(conn, "disconnected");
}
