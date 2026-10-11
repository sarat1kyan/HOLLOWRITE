//! End-to-end tests over real HTTP and WebSocket against a real database.
//! Requires `TEST_DATABASE_URL`; each test is skipped when it is unset.

use std::path::PathBuf;
use std::time::Duration;

use hollowrite_protocol::*;
use hollowrite_server::ratelimit::Limiter;
use hollowrite_server::{Config, Limits, Server};
use hollowrite_test_support::{obtain_token, Bot};

const PASSWORD: &str = "correct horse battery";

fn config() -> Option<Config> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set; skipping server test");
        return None;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: url,
        content_dir: root.join("content"),
        web_dir: None,
        allowed_origins: vec!["http://localhost:5173".into()],
        secure_cookies: false,
        seed: 7,
        session_days: 1,
        db_max_connections: 5,
        public_metrics: false,
        registrations_per_hour: 1000.0,
        connect_burst: 1000.0,
    })
}

fn limits() -> Limits {
    Limits {
        register: Limiter::new(1000.0, 1000.0),
        login_ip: Limiter::new(1000.0, 1000.0),
        login_user: Limiter::new(1000.0, 1000.0),
        ws_connect: Limiter::new(1000.0, 1000.0),
    }
}

fn uniq(prefix: &str) -> String {
    format!("{prefix}{}", rand_suffix())
}

fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
    format!("{:x}{}", t % 0xfffff, N.fetch_add(1, Ordering::Relaxed))
}

struct Ctx {
    server: Server,
}

impl Ctx {
    fn http(&self) -> String {
        format!("http://{}", self.server.addr)
    }
    fn ws(&self) -> String {
        format!("ws://{}/ws", self.server.addr)
    }
    async fn bot(&self, name: &str) -> Bot {
        let token = obtain_token(&self.http(), name, PASSWORD).await.unwrap();
        Bot::connect(&self.ws(), &token).await.unwrap()
    }
}

fn init_tracing() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    }
}

async fn start() -> Option<Ctx> {
    init_tracing();
    let cfg = config()?;
    Some(Ctx { server: Server::start(cfg, limits()).await.unwrap() })
}

#[tokio::test(flavor = "multi_thread")]
async fn two_players_share_one_world() {
    let Some(ctx) = start().await else { return };
    let (na, nb) = (uniq("Al"), uniq("Bo"));
    let mut a = ctx.bot(&na).await;
    let mut b = ctx.bot(&nb).await;
    a.enter(&na, Archetype::AshWarden).await.unwrap();
    b.enter(&nb, Archetype::GraveRunner).await.unwrap();
    a.pump(Duration::from_millis(300)).await.unwrap();
    b.pump(Duration::from_millis(300)).await.unwrap();
    let b_in_a = a.sees_named(&nb).expect("A sees B").clone();
    assert_eq!(b_in_a.kind, EntityKind::Player);
    assert_eq!(b_in_a.rel, Relation::Stranger);
    let before = b.sees(a.me).expect("B sees A").x;
    for _ in 0..12 {
        a.input(Some(Dir8::E), None).await.unwrap();
        a.pump(Duration::from_millis(40)).await.unwrap();
    }
    a.input(None, None).await.unwrap();
    b.pump(Duration::from_millis(400)).await.unwrap();
    a.pump(Duration::from_millis(100)).await.unwrap();
    b.pump(Duration::from_millis(100)).await.unwrap();
    let after = b.sees(a.me).expect("B still sees A").x;
    assert!(after > before + 1000, "B saw A move from {before} to {after}");
    let my = a.my_view().unwrap();
    assert_eq!(my.x, after, "both clients agree on A's authoritative position");
    ctx.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_and_party_routing() {
    let Some(ctx) = start().await else { return };
    let (na, nb) = (uniq("Cy"), uniq("Di"));
    let mut a = ctx.bot(&na).await;
    let mut b = ctx.bot(&nb).await;
    a.enter(&na, Archetype::VeilSeer).await.unwrap();
    b.enter(&nb, Archetype::AshWarden).await.unwrap();

    a.send(&ClientMsg::Chat { channel: ChatChannel::Local, text: "hello \x1b[2J<b>there</b>".into() }).await.unwrap();
    b.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Chat { channel: ChatChannel::Local, .. })).await.unwrap();
    let (_, from, text) = b.chat.iter().find(|c| c.0 == ChatChannel::Local).unwrap().clone();
    assert_eq!(from, na);
    assert!(!text.contains('\x1b'), "escape sequence leaked: {text:?}");

    b.send(&ClientMsg::Chat { channel: ChatChannel::Party, text: "anyone?".into() }).await.unwrap();
    b.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Error { .. })).await.unwrap();

    a.send(&ClientMsg::PartyInvite { name: nb.clone() }).await.unwrap();
    b.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Party { invites, .. } if !invites.is_empty()))
        .await
        .unwrap();
    b.send(&ClientMsg::PartyAccept { from: na.clone() }).await.unwrap();
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Party { members, .. } if members.len() == 2))
        .await
        .unwrap();
    b.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Party { members, .. } if members.len() == 2))
        .await
        .unwrap();
    a.pump(Duration::from_millis(200)).await.unwrap();
    assert_eq!(a.sees_named(&nb).unwrap().rel, Relation::Party);

    b.send(&ClientMsg::Chat { channel: ChatChannel::Party, text: "together".into() }).await.unwrap();
    a.wait(
        Duration::from_secs(3),
        |m| matches!(m, ServerMsg::Chat { channel: ChatChannel::Party, text, .. } if text == "together"),
    )
    .await
    .unwrap();

    // Chat flooding is throttled server side.
    for i in 0..12 {
        a.send(&ClientMsg::Chat { channel: ChatChannel::Local, text: format!("spam {i}") }).await.unwrap();
    }
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Error { code: ErrorCode::RateLimited, .. }))
        .await
        .unwrap();

    b.send(&ClientMsg::PartyLeave).await.unwrap();
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Party { members, .. } if members.is_empty()))
        .await
        .unwrap();
    ctx.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_and_malformed_traffic_is_rejected() {
    let Some(ctx) = start().await else { return };
    let (na, nb) = (uniq("Ev"), uniq("Fa"));
    let mut a = ctx.bot(&na).await;
    a.enter(&na, Archetype::AshWarden).await.unwrap();
    let mut b = ctx.bot(&nb).await;
    let b_chars = b.create(&nb, Archetype::AshWarden).await.unwrap();

    // Selecting another account's character is refused.
    a.send(&ClientMsg::Leave).await.unwrap();
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Characters { .. })).await.unwrap();
    a.send(&ClientMsg::SelectCharacter { id: b_chars[0].id.clone() }).await.unwrap();
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Error { code: ErrorCode::NotFound, .. })).await.unwrap();
    a.enter(&na, Archetype::AshWarden).await.unwrap();

    // Messages that do not exist in the protocol are rejected, not executed.
    a.send_raw(r#"{"t":"grant_echoes","amount":999999}"#).await.unwrap();
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Error { code: ErrorCode::BadRequest, .. }))
        .await
        .unwrap();
    a.send_raw(r#"{"t":"input","seq":1,"mv":"e","teleport":[1,1]}"#).await.unwrap();

    // Duplicate and stale sequence numbers never move the ack backwards.
    let s = a.input(Some(Dir8::E), None).await.unwrap();
    a.send(&ClientMsg::Input { seq: s, mv: Some(Dir8::W), act: Some(Action::Heavy), face: None }).await.unwrap();
    a.send(&ClientMsg::Input { seq: 1, mv: Some(Dir8::W), act: None, face: None }).await.unwrap();
    a.pump(Duration::from_millis(300)).await.unwrap();
    assert_eq!(a.last.as_ref().unwrap().ack, s);
    assert_ne!(a.my_view().unwrap().state, ActorState::Windup);

    // Using an item the character does not carry is refused.
    a.send(&ClientMsg::UseItem { item: "ashen_moth".into() }).await.unwrap();
    a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Error { code: ErrorCode::NotAllowed, .. }))
        .await
        .unwrap();
    assert_eq!(a.sheet.as_ref().unwrap().echoes, 0);

    // Oversized frames close the connection.
    let huge = format!(r#"{{"t":"chat","channel":"local","text":"{}"}}"#, "x".repeat(MAX_CLIENT_FRAME_BYTES * 2));
    let _ = a.send_raw(&huge).await;
    let closed = a.pump(Duration::from_secs(2)).await;
    assert!(closed.is_err(), "oversized frame should drop the connection");
    ctx.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn second_login_takes_over_the_character() {
    let Some(ctx) = start().await else { return };
    let name = uniq("Gi");
    let mut first = ctx.bot(&name).await;
    first.enter(&name, Archetype::GraveRunner).await.unwrap();
    let mut second = ctx.bot(&name).await;
    second.enter(&name, Archetype::GraveRunner).await.unwrap();
    let r = first.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Kicked { .. })).await;
    assert!(r.is_ok() || first.kicked.is_some(), "the first client should be told it was replaced");
    second.pump(Duration::from_millis(300)).await.unwrap();
    let mine =
        second.last.as_ref().unwrap().entities.iter().filter(|e| e.name.as_deref() == Some(name.as_str())).count();
    assert_eq!(mine, 1, "exactly one body for the character");
    ctx.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_survives_disconnect_and_restart() {
    let Some(cfg) = config() else { return };
    let name = uniq("Hu");
    let server = Server::start(cfg.clone(), limits()).await.unwrap();
    let http = format!("http://{}", server.addr);
    let token = obtain_token(&http, &name, PASSWORD).await.unwrap();
    let mut bot = Bot::connect(&format!("ws://{}/ws", server.addr), &token).await.unwrap();
    bot.enter(&name, Archetype::GraveRunner).await.unwrap();
    assert_eq!(bot.sheet.as_ref().unwrap().inventory.iter().find(|i| i.item == "grave_salt").map(|i| i.qty), Some(1));
    bot.send(&ClientMsg::UseItem { item: "grave_salt".into() }).await.unwrap();
    bot.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Sheet { .. })).await.unwrap();
    for _ in 0..10 {
        bot.input(Some(Dir8::E), None).await.unwrap();
        bot.pump(Duration::from_millis(40)).await.unwrap();
    }
    bot.input(None, None).await.unwrap();
    bot.pump(Duration::from_millis(200)).await.unwrap();
    let pos = (bot.my_view().unwrap().x, bot.my_view().unwrap().y);
    drop(bot);
    tokio::time::sleep(Duration::from_millis(300)).await;
    server.shutdown().await;

    let server = Server::start(cfg, limits()).await.unwrap();
    let mut bot = Bot::connect(&format!("ws://{}/ws", server.addr), &token).await.unwrap();
    bot.enter(&name, Archetype::GraveRunner).await.unwrap();
    let sheet = bot.sheet.clone().unwrap();
    assert!(sheet.inventory.iter().all(|i| i.item != "grave_salt"), "consumed item came back after restart");
    let me = bot.my_view().unwrap();
    assert_eq!((me.x, me.y), pos, "position was not restored");
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn http_auth_surface() {
    let Some(ctx) = start().await else { return };
    let http = ctx.http();
    let c = reqwest::Client::new();
    let name = uniq("Iv");
    let body = serde_json::json!({ "username": name, "password": PASSWORD });

    let r = c.post(format!("{http}/api/register")).json(&body).send().await.unwrap();
    assert_eq!(r.status(), 403, "missing CSRF header must be refused");
    let r = c
        .post(format!("{http}/api/register"))
        .header("x-hollowrite", "1")
        .header("origin", "https://evil.example")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "foreign origin must be refused");
    let r = c
        .post(format!("{http}/api/register"))
        .header("x-hollowrite", "1")
        .json(&serde_json::json!({ "username": "x<script>", "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let r = c
        .post(format!("{http}/api/register"))
        .header("x-hollowrite", "1")
        .json(&serde_json::json!({ "username": name, "password": "short" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    let r = c.post(format!("{http}/api/register")).header("x-hollowrite", "1").json(&body).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let cookie = r.headers().get("set-cookie").unwrap().to_str().unwrap().to_string();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    let v: serde_json::Value = r.json().await.unwrap();
    assert!(v.get("token").is_none(), "browser logins must not expose the token to script");

    let r = c
        .post(format!("{http}/api/login"))
        .header("x-hollowrite", "1")
        .json(&serde_json::json!({ "username": name, "password": "wrong password!!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    // WebSocket without credentials, and with a foreign origin, is refused.
    let r = c
        .get(format!("{http}/ws"))
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let token_cookie = cookie.split(';').next().unwrap().to_string();
    let r = c
        .get(format!("{http}/ws"))
        .header("cookie", &token_cookie)
        .header("origin", "https://evil.example")
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    // A cookie alone, without an Origin, is not accepted either.
    let r = c
        .get(format!("{http}/ws"))
        .header("cookie", &token_cookie)
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    let r = c.get(format!("{http}/api/me")).header("cookie", &token_cookie).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = c
        .post(format!("{http}/api/logout"))
        .header("x-hollowrite", "1")
        .header("cookie", &token_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let r = c.get(format!("{http}/api/me")).header("cookie", &token_cookie).send().await.unwrap();
    assert_eq!(r.status(), 401);

    let r = c.get(format!("{http}/healthz")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let r = c.get(format!("{http}/readyz")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = c.get(format!("{http}/metrics")).send().await.unwrap();
    assert!(r.text().await.unwrap().contains("hollowrite_ticks_total"));
    ctx.server.shutdown().await;
}
