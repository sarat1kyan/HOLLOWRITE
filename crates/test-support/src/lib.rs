//! Real protocol clients for tests and operational checks. These talk to a
//! running server over HTTP and WebSocket exactly like the shipped clients.

pub mod proxy;

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use hollowrite_protocol::*;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Registers (or logs in, if the name exists) and returns a bearer token.
pub async fn obtain_token(http_base: &str, username: &str, password: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "username": username, "password": password, "client": "bot" });
    let post =
        |path: &'static str| client.post(format!("{http_base}{path}")).header("x-hollowrite", "1").json(&body).send();
    let mut res = post("/api/register").await.context("POST /api/register")?;
    if res.status() == reqwest::StatusCode::CONFLICT {
        res = post("/api/login").await.context("POST /api/login")?;
    }
    if !res.status().is_success() {
        bail!("authentication failed: {} {}", res.status(), res.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = res.json().await?;
    v["token"].as_str().map(str::to_string).ok_or_else(|| anyhow!("no token in response"))
}

pub struct Bot {
    pub ws: Ws,
    pub seq: u32,
    pub me: EntityId,
    pub map: Option<MapData>,
    pub sheet: Option<CharacterSheet>,
    pub last: Option<Snapshot>,
    pub events: Vec<GameEvent>,
    pub chat: Vec<(ChatChannel, String, String)>,
    pub party: Vec<PartyMember>,
    pub invites: Vec<String>,
    pub errors: Vec<(ErrorCode, String)>,
    pub dialogue: Option<(EntityId, Vec<String>, Vec<String>)>,
    pub kicked: Option<String>,
}

impl Bot {
    pub async fn connect(ws_url: &str, token: &str) -> Result<Bot> {
        let mut req = ws_url.into_client_request()?;
        req.headers_mut().insert("authorization", format!("Bearer {token}").parse()?);
        let (ws, _) = tokio_tungstenite::connect_async(req).await.context("websocket connect")?;
        let mut bot = Bot {
            ws,
            seq: 0,
            me: 0,
            map: None,
            sheet: None,
            last: None,
            events: Vec::new(),
            chat: Vec::new(),
            party: Vec::new(),
            invites: Vec::new(),
            errors: Vec::new(),
            dialogue: None,
            kicked: None,
        };
        bot.send(&ClientMsg::Hello { protocol: PROTOCOL_VERSION, client: ClientKind::Bot, build: "bot".into() })
            .await?;
        bot.wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::Welcome { .. })).await?;
        Ok(bot)
    }

    pub async fn send(&mut self, msg: &ClientMsg) -> Result<()> {
        self.ws.send(Message::Text(serde_json::to_string(msg)?.into())).await?;
        Ok(())
    }

    pub async fn send_raw(&mut self, text: &str) -> Result<()> {
        self.ws.send(Message::Text(text.to_string().into())).await?;
        Ok(())
    }

    fn absorb(&mut self, msg: &ServerMsg) {
        match msg {
            ServerMsg::EnterWorld { you, map, sheet, .. } => {
                self.me = *you;
                self.map = Some(map.clone());
                self.sheet = Some(sheet.clone());
            }
            ServerMsg::Snapshot(s) => self.last = Some(s.clone()),
            ServerMsg::Sheet { sheet } => self.sheet = Some(sheet.clone()),
            ServerMsg::Events { events, .. } => self.events.extend(events.iter().cloned()),
            ServerMsg::Chat { channel, from, text } => self.chat.push((*channel, from.clone(), text.clone())),
            ServerMsg::Party { members, invites } => {
                self.party = members.clone();
                self.invites = invites.clone();
            }
            ServerMsg::Error { code, message } => self.errors.push((*code, message.clone())),
            ServerMsg::Dialogue { npc, lines, choices, .. } => {
                self.dialogue = Some((*npc, lines.clone(), choices.clone()))
            }
            ServerMsg::Kicked { reason } => self.kicked = Some(reason.clone()),
            _ => {}
        }
    }

    /// Reads frames until `pred` matches one or the timeout elapses.
    pub async fn wait(&mut self, timeout: Duration, mut pred: impl FnMut(&ServerMsg) -> bool) -> Result<ServerMsg> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("timed out waiting for message");
            }
            let frame = tokio::time::timeout(left, self.ws.next())
                .await
                .map_err(|_| anyhow!("timed out waiting for message"))?;
            match frame {
                Some(Ok(Message::Text(t))) => {
                    let msg: ServerMsg = serde_json::from_str(&t).with_context(|| format!("decoding {t}"))?;
                    self.absorb(&msg);
                    if pred(&msg) {
                        return Ok(msg);
                    }
                }
                Some(Ok(Message::Close(c))) => bail!("closed: {:?}", c.map(|c| c.reason.to_string())),
                Some(Ok(_)) => {}
                Some(Err(e)) => bail!("socket error: {e}"),
                None => bail!("socket ended"),
            }
        }
    }

    /// Drains whatever arrives within `dur`.
    pub async fn pump(&mut self, dur: Duration) -> Result<()> {
        let deadline = Instant::now() + dur;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            match tokio::time::timeout(left, self.ws.next()).await {
                Err(_) => return Ok(()),
                Ok(Some(Ok(Message::Text(t)))) => {
                    let msg: ServerMsg = serde_json::from_str(&t)?;
                    self.absorb(&msg);
                }
                Ok(Some(Ok(Message::Close(c)))) => bail!("closed: {:?}", c.map(|c| c.reason.to_string())),
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(e))) => bail!("socket error: {e}"),
                Ok(None) => bail!("socket ended"),
            }
        }
    }

    pub async fn characters(&mut self) -> Result<Vec<CharacterSummary>> {
        self.send(&ClientMsg::ListCharacters).await?;
        match self.wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::Characters { .. })).await? {
            ServerMsg::Characters { list } => Ok(list),
            _ => unreachable!(),
        }
    }

    pub async fn create(&mut self, name: &str, archetype: Archetype) -> Result<Vec<CharacterSummary>> {
        self.send(&ClientMsg::CreateCharacter { name: name.into(), archetype, look: Appearance { sigil: 1, hue: 2 } })
            .await?;
        match self
            .wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::Characters { .. } | ServerMsg::Error { .. }))
            .await?
        {
            ServerMsg::Characters { list } => Ok(list),
            ServerMsg::Error { message, .. } => bail!("create failed: {message}"),
            _ => unreachable!(),
        }
    }

    /// Creates the character if needed and enters the world with it.
    pub async fn enter(&mut self, name: &str, archetype: Archetype) -> Result<()> {
        let mut list = self.characters().await?;
        if !list.iter().any(|c| c.name == name) {
            list = self.create(name, archetype).await?;
        }
        let id =
            list.iter().find(|c| c.name == name).map(|c| c.id.clone()).ok_or_else(|| anyhow!("character missing"))?;
        self.send(&ClientMsg::SelectCharacter { id }).await?;
        self.wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::EnterWorld { .. })).await?;
        self.wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::Snapshot(_))).await?;
        Ok(())
    }

    pub async fn input(&mut self, mv: Option<Dir8>, act: Option<Action>) -> Result<u32> {
        self.seq += 1;
        let seq = self.seq;
        self.send(&ClientMsg::Input { seq, mv, act, face: None }).await?;
        Ok(seq)
    }

    pub fn my_view(&self) -> Option<&EntityView> {
        self.last.as_ref()?.entities.iter().find(|e| e.id == self.me)
    }

    pub fn my_cell(&self) -> Option<(i32, i32)> {
        self.my_view().map(|e| (e.x.div_euclid(CELL), e.y.div_euclid(CELL)))
    }

    pub fn sees(&self, id: EntityId) -> Option<&EntityView> {
        self.last.as_ref()?.entities.iter().find(|e| e.id == id)
    }

    pub fn sees_named(&self, name: &str) -> Option<&EntityView> {
        self.last.as_ref()?.entities.iter().find(|e| e.name.as_deref() == Some(name))
    }

    /// Walks greedily toward a cell, re-reading the authoritative position.
    pub async fn walk_to(&mut self, target: (i32, i32), timeout: Duration) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let Some((x, y)) = self.my_cell() else {
                self.pump(Duration::from_millis(50)).await?;
                continue;
            };
            if (x, y) == target {
                self.input(None, None).await?;
                return Ok(true);
            }
            let d = Dir8::from_offset(target.0 - x, target.1 - y).unwrap();
            self.input(Some(d), None).await?;
            self.pump(Duration::from_millis(60)).await?;
        }
        Ok(false)
    }
}
