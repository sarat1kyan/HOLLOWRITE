use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use hollowrite_protocol::*;
use hollowrite_test_support::proxy::LatencyProxy;
use hollowrite_test_support::{obtain_token, Bot};
use rand::Rng;

#[derive(Parser)]
#[command(name = "hollowrite-bot", about = "Protocol bots for smoke, load and latency checks")]
struct Cli {
    /// Base HTTP URL of the server.
    #[arg(long, default_value = "http://127.0.0.1:7700", global = true)]
    server: String,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Two players meet, chat, party up and move; exits non-zero on failure.
    Smoke,
    /// N wandering bots for a while, then report server and client metrics.
    Load {
        #[arg(long, default_value_t = 20)]
        bots: usize,
        #[arg(long, default_value_t = 30)]
        seconds: u64,
    },
    /// Measure input-to-acknowledgement latency through a delaying proxy.
    Latency {
        #[arg(long, default_value_t = 150)]
        rtt_ms: u64,
        #[arg(long, default_value_t = 40)]
        jitter_ms: u64,
        #[arg(long, default_value_t = 200)]
        samples: usize,
    },
}

const PASSWORD: &str = "smoke-test-password";

fn ws_url(http: &str) -> String {
    format!("{}/ws", http.replacen("http", "ws", 1))
}

fn suffix() -> String {
    format!("{:04x}", rand::thread_rng().gen::<u16>())
}

async fn player(server: &str, name: &str, archetype: Archetype) -> Result<Bot> {
    let token = obtain_token(server, name, PASSWORD).await?;
    let mut bot = Bot::connect(&ws_url(server), &token).await?;
    bot.enter(name, archetype).await?;
    Ok(bot)
}

fn check(ok: bool, what: &str) -> Result<()> {
    if ok {
        println!("  ok    {what}");
        Ok(())
    } else {
        println!("  FAIL  {what}");
        bail!("smoke failed: {what}")
    }
}

async fn smoke(server: &str) -> Result<()> {
    let s = suffix();
    let (na, nb) = (format!("SmokeA{s}"), format!("SmokeB{s}"));
    println!("smoke against {server}");
    let mut a = player(server, &na, Archetype::AshWarden).await.context("player A")?;
    let mut b = player(server, &nb, Archetype::GraveRunner).await.context("player B")?;
    check(true, "two accounts registered and entered the world")?;
    a.pump(Duration::from_millis(400)).await?;
    b.pump(Duration::from_millis(400)).await?;
    check(a.sees_named(&nb).is_some() && b.sees_named(&na).is_some(), "players see each other")?;
    let before = b.sees(a.me).map(|e| e.x).unwrap_or_default();
    for _ in 0..10 {
        a.input(Some(Dir8::E), None).await?;
        a.pump(Duration::from_millis(50)).await?;
    }
    a.input(None, None).await?;
    b.pump(Duration::from_millis(400)).await?;
    let after = b.sees(a.me).map(|e| e.x).unwrap_or_default();
    check(after > before, "movement is replicated to the other client")?;
    a.send(&ClientMsg::Chat { channel: ChatChannel::Local, text: "the lantern holds".into() }).await?;
    let got = b
        .wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Chat { text, .. } if text == "the lantern holds"))
        .await;
    check(got.is_ok(), "local chat delivered")?;
    a.send(&ClientMsg::PartyInvite { name: nb.clone() }).await?;
    b.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Party { invites, .. } if !invites.is_empty())).await?;
    b.send(&ClientMsg::PartyAccept { from: na.clone() }).await?;
    let party =
        a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Party { members, .. } if members.len() == 2)).await;
    check(party.is_ok(), "party formed")?;
    a.send(&ClientMsg::Ping { nonce: 1 }).await?;
    check(
        a.wait(Duration::from_secs(3), |m| matches!(m, ServerMsg::Pong { nonce: 1, .. })).await.is_ok(),
        "ping answered",
    )?;
    println!("smoke passed");
    Ok(())
}

async fn wander(mut bot: Bot, until: Instant) -> Result<(usize, Vec<Duration>)> {
    let mut snaps = 0usize;
    let mut gaps = Vec::new();
    let mut last = Instant::now();
    let mut dir = Dir8::E;
    while Instant::now() < until {
        if rand::thread_rng().gen_ratio(1, 15) {
            dir = Dir8::ALL[rand::thread_rng().gen_range(0..8)];
        }
        let act = if rand::thread_rng().gen_ratio(1, 30) { Some(Action::Light) } else { None };
        bot.input(Some(dir), act).await?;
        let before = bot.last.as_ref().map(|s| s.tick);
        bot.pump(Duration::from_millis(100)).await?;
        if bot.last.as_ref().map(|s| s.tick) != before {
            snaps += 1;
            gaps.push(last.elapsed());
            last = Instant::now();
        }
    }
    Ok((snaps, gaps))
}

async fn load(server: &str, bots: usize, seconds: u64) -> Result<()> {
    println!("load: {bots} bots for {seconds}s against {server}");
    let s = suffix();
    let mut handles = Vec::new();
    let until = Instant::now() + Duration::from_secs(seconds) + Duration::from_secs(5);
    for i in 0..bots {
        let server = server.to_string();
        let name = format!("Load{s}x{i}");
        handles.push(tokio::spawn(async move {
            let bot = player(&server, &name, Archetype::ALL[i % 3]).await?;
            wander(bot, until).await
        }));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut total = 0;
    let mut failures = 0;
    for h in handles {
        match h.await? {
            Ok((n, _)) => total += n,
            Err(e) => {
                failures += 1;
                eprintln!("bot failed: {e:#}");
            }
        }
    }
    let metrics = reqwest::get(format!("{server}/metrics")).await?.text().await.unwrap_or_default();
    let read = |k: &str| -> f64 {
        metrics
            .lines()
            .find(|l| l.starts_with(&format!("hollowrite_{k} ")))
            .and_then(|l| l.split(' ').nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    };
    println!("bots ok: {}, failed: {failures}", bots - failures);
    println!("client snapshot batches observed: {total}");
    println!(
        "server tick last: {} us, max: {} us, overruns: {}",
        read("tick_duration_us"),
        read("tick_duration_max_us"),
        read("tick_overruns_total")
    );
    let sent = read("snapshots_sent_total");
    if sent > 0.0 {
        println!(
            "snapshots sent: {sent}, mean size: {:.0} bytes, dropped: {}",
            read("snapshot_bytes_total") / sent,
            read("snapshots_dropped_total")
        );
    } else {
        println!("server metrics unavailable (metrics are loopback-only by default)");
    }
    if failures > 0 {
        bail!("{failures} bots failed");
    }
    Ok(())
}

async fn latency(server: &str, rtt_ms: u64, jitter_ms: u64, samples: usize) -> Result<()> {
    let upstream: std::net::SocketAddr =
        server.trim_start_matches("http://").parse().context("latency needs http://ip:port")?;
    let proxy =
        LatencyProxy::start(upstream, Duration::from_millis(rtt_ms / 2), Duration::from_millis(jitter_ms / 2)).await?;
    let via = format!("http://{}", proxy.addr);
    println!("latency: rtt {rtt_ms} ms, jitter up to {jitter_ms} ms, via {via}");
    let name = format!("Lag{}", suffix());
    let mut bot = player(&via, &name, Archetype::GraveRunner).await?;
    let mut acks = Vec::new();
    let mut pings = Vec::new();
    for i in 0..samples {
        let dir = if (i / 20) % 2 == 0 { Dir8::E } else { Dir8::W };
        let sent = Instant::now();
        let seq = bot.input(Some(dir), None).await?;
        bot.wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::Snapshot(s) if s.ack >= seq)).await?;
        acks.push(sent.elapsed());
        if i % 10 == 0 {
            let t = Instant::now();
            bot.send(&ClientMsg::Ping { nonce: i as u32 }).await?;
            bot.wait(Duration::from_secs(5), |m| matches!(m, ServerMsg::Pong { nonce, .. } if *nonce == i as u32))
                .await?;
            pings.push(t.elapsed());
        }
    }
    let report = |label: &str, v: &mut Vec<Duration>| {
        v.sort();
        let pct = |p: f64| v[((v.len() - 1) as f64 * p) as usize].as_millis();
        println!(
            "{label}: n={} p50={} ms p95={} ms max={} ms",
            v.len(),
            pct(0.5),
            pct(0.95),
            v.last().unwrap().as_millis()
        );
    };
    report("ping round trip", &mut pings);
    report("input to authoritative ack", &mut acks);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let server = cli.server.trim_end_matches('/').to_string();
    match cli.command {
        Cmd::Smoke => smoke(&server).await,
        Cmd::Load { bots, seconds } => load(&server, bots, seconds).await,
        Cmd::Latency { rtt_ms, jitter_ms, samples } => latency(&server, rtt_ms, jitter_ms, samples).await,
    }
}
