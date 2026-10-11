use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use hollowrite_protocol::{ClientKind, ClientMsg, ServerMsg, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stored {
    pub server: String,
    pub username: String,
    pub token: String,
}

fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("hollowrite").join("session.json"))
}

pub fn load_session(server: &str) -> Option<Stored> {
    let text = std::fs::read_to_string(store_path()?).ok()?;
    let s: Stored = serde_json::from_str(&text).ok()?;
    (s.server == server).then_some(s)
}

/// Persists the session token with owner-only permissions. The password is
/// never written anywhere.
pub fn save_session(s: &Stored) -> Result<()> {
    let path = store_path().ok_or_else(|| anyhow!("no config directory"))?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let text = serde_json::to_string(s)?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
        f.write_all(text.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, text)?;
    Ok(())
}

pub fn clear_session() {
    if let Some(p) = store_path() {
        let _ = std::fs::remove_file(p);
    }
}

pub async fn authenticate(server: &str, username: &str, password: &str, register: bool) -> Result<String> {
    let path = if register { "/api/register" } else { "/api/login" };
    let res = reqwest::Client::new()
        .post(format!("{server}{path}"))
        .header("x-hollowrite", "1")
        .json(&serde_json::json!({ "username": username, "password": password, "client": "terminal" }))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .with_context(|| format!("could not reach {server}"))?;
    let status = res.status();
    let body: serde_json::Value = res.json().await.unwrap_or_default();
    if !status.is_success() {
        bail!("{}", body["error"].as_str().unwrap_or("authentication failed"));
    }
    body["token"].as_str().map(str::to_string).ok_or_else(|| anyhow!("server did not return a token"))
}

pub async fn logout(server: &str, token: &str) {
    let _ = reqwest::Client::new()
        .post(format!("{server}/api/logout"))
        .header("x-hollowrite", "1")
        .bearer_auth(token)
        .timeout(Duration::from_secs(5))
        .send()
        .await;
}

#[derive(Debug)]
pub enum NetEvent {
    Msg(Box<ServerMsg>),
    Rtt(Duration),
    Closed(String),
}

pub fn ws_url(server: &str) -> String {
    let base = server.trim_end_matches('/');
    if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}/ws")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}/ws")
    } else {
        format!("ws://{base}/ws")
    }
}

/// Connects and runs the socket on a background task. Outbound messages go
/// through the returned sender; everything inbound arrives on `events`.
pub fn spawn(server: &str, token: &str, events: mpsc::UnboundedSender<NetEvent>) -> mpsc::UnboundedSender<ClientMsg> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ClientMsg>();
    let url = ws_url(server);
    let token = token.to_string();
    tokio::spawn(async move {
        let mut req = match url.as_str().into_client_request() {
            Ok(r) => r,
            Err(e) => {
                let _ = events.send(NetEvent::Closed(format!("bad server address: {e}")));
                return;
            }
        };
        req.headers_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
        let ws = match tokio_tungstenite::connect_async(req).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                let _ = events.send(NetEvent::Closed(format!("could not connect: {e}")));
                return;
            }
        };
        let (mut sink, mut stream) = ws.split();
        let hello = ClientMsg::Hello {
            protocol: PROTOCOL_VERSION,
            client: ClientKind::Terminal,
            build: env!("CARGO_PKG_VERSION").into(),
        };
        if sink.send(Message::Text(serde_json::to_string(&hello).unwrap().into())).await.is_err() {
            let _ = events.send(NetEvent::Closed("connection dropped".into()));
            return;
        }
        let mut ping = tokio::time::interval(Duration::from_secs(2));
        let mut pending_ping: Option<(u32, Instant)> = None;
        let mut nonce = 0u32;
        let reason = loop {
            tokio::select! {
                out = rx.recv() => {
                    let Some(msg) = out else { break "closed".to_string() };
                    if sink.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() {
                        break "connection dropped".into();
                    }
                }
                _ = ping.tick() => {
                    nonce = nonce.wrapping_add(1);
                    pending_ping = Some((nonce, Instant::now()));
                    let msg = ClientMsg::Ping { nonce };
                    if sink.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() {
                        break "connection dropped".into();
                    }
                }
                frame = stream.next() => match frame {
                    Some(Ok(Message::Text(t))) => match serde_json::from_str::<ServerMsg>(&t) {
                        Ok(ServerMsg::Pong { nonce: n, .. }) => {
                            if let Some((p, at)) = pending_ping {
                                if p == n {
                                    let _ = events.send(NetEvent::Rtt(at.elapsed()));
                                }
                            }
                        }
                        Ok(msg) => {
                            let _ = events.send(NetEvent::Msg(Box::new(msg)));
                        }
                        Err(_) => {}
                    },
                    Some(Ok(Message::Close(frame))) => {
                        break frame.map(|f| f.reason.to_string()).filter(|r| !r.is_empty()).unwrap_or_else(|| "server closed the connection".into());
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => break format!("connection error: {e}"),
                    None => break "connection lost".into(),
                }
            }
        };
        let _ = events.send(NetEvent::Closed(reason));
    });
    tx
}
