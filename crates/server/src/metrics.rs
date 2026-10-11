use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    pub connections: AtomicU64,
    pub players_online: AtomicU64,
    pub ticks: AtomicU64,
    pub tick_us_last: AtomicU64,
    pub tick_us_max: AtomicU64,
    pub tick_overruns: AtomicU64,
    pub snapshot_bytes: AtomicU64,
    pub snapshots_sent: AtomicU64,
    pub snapshots_dropped: AtomicU64,
    pub frames_in: AtomicU64,
    pub frames_rejected: AtomicU64,
    pub slow_consumer_kicks: AtomicU64,
    pub saves_ok: AtomicU64,
    pub saves_stale: AtomicU64,
    pub saves_failed: AtomicU64,
    pub db_us_last: AtomicU64,
    pub world_heartbeat_ms: AtomicU64,
}

impl Metrics {
    pub fn inc(c: &AtomicU64) {
        c.fetch_add(1, Ordering::Relaxed);
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        let mut g = |name: &str, help: &str, kind: &str, v: &AtomicU64| {
            let _ = writeln!(
                s,
                "# HELP hollowrite_{name} {help}\n# TYPE hollowrite_{name} {kind}\nhollowrite_{name} {}",
                v.load(Ordering::Relaxed)
            );
        };
        g("connections", "Open WebSocket connections", "gauge", &self.connections);
        g("players_online", "Characters in the world", "gauge", &self.players_online);
        g("ticks_total", "Simulation ticks executed", "counter", &self.ticks);
        g("tick_duration_us", "Duration of the last tick in microseconds", "gauge", &self.tick_us_last);
        g("tick_duration_max_us", "Longest tick since start in microseconds", "gauge", &self.tick_us_max);
        g("tick_overruns_total", "Ticks that exceeded their time budget", "counter", &self.tick_overruns);
        g("snapshot_bytes_total", "Bytes of snapshot JSON queued", "counter", &self.snapshot_bytes);
        g("snapshots_sent_total", "Snapshots queued to clients", "counter", &self.snapshots_sent);
        g("snapshots_dropped_total", "Snapshots dropped for slow clients", "counter", &self.snapshots_dropped);
        g("frames_in_total", "Client frames received", "counter", &self.frames_in);
        g("frames_rejected_total", "Client frames rejected", "counter", &self.frames_rejected);
        g("slow_consumer_kicks_total", "Connections dropped for not reading", "counter", &self.slow_consumer_kicks);
        g("saves_ok_total", "Character saves committed", "counter", &self.saves_ok);
        g("saves_stale_total", "Character saves skipped as stale", "counter", &self.saves_stale);
        g("saves_failed_total", "Character save attempts that errored", "counter", &self.saves_failed);
        g("db_latency_us", "Latency of the last database save in microseconds", "gauge", &self.db_us_last);
        s
    }
}
