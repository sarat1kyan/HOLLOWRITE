use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub content_dir: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub allowed_origins: Vec<String>,
    pub secure_cookies: bool,
    pub seed: u64,
    pub session_days: u64,
    pub db_max_connections: u32,
    pub public_metrics: bool,
    pub registrations_per_hour: f64,
    pub connect_burst: f64,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let bind: SocketAddr = env("HOLLOWRITE_BIND").unwrap_or_else(|| "127.0.0.1:7700".into()).parse()?;
        let database_url = env("DATABASE_URL").ok_or_else(|| anyhow::anyhow!("DATABASE_URL must be set"))?;
        let content_dir = env("HOLLOWRITE_CONTENT_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("content"));
        let web_dir = env("HOLLOWRITE_WEB_DIR").map(PathBuf::from).or_else(|| {
            let p = PathBuf::from("apps/web/dist");
            p.join("index.html").exists().then_some(p)
        });
        let port = bind.port();
        let mut allowed_origins: Vec<String> = env("HOLLOWRITE_ALLOWED_ORIGINS")
            .map(|v| {
                v.split(',').map(|s| s.trim().trim_end_matches('/').to_string()).filter(|s| !s.is_empty()).collect()
            })
            .unwrap_or_default();
        if allowed_origins.is_empty() {
            for host in ["localhost", "127.0.0.1"] {
                allowed_origins.push(format!("http://{host}:{port}"));
                allowed_origins.push(format!("http://{host}:5173"));
            }
        }
        Ok(Config {
            bind,
            database_url,
            content_dir,
            web_dir,
            allowed_origins,
            secure_cookies: env("HOLLOWRITE_SECURE_COOKIES").is_some_and(|v| v == "1" || v == "true"),
            seed: env("HOLLOWRITE_SEED").and_then(|v| v.parse().ok()).unwrap_or(0x484f_4c4c_4f57),
            session_days: env("HOLLOWRITE_SESSION_DAYS").and_then(|v| v.parse().ok()).unwrap_or(14),
            db_max_connections: env("HOLLOWRITE_DB_MAX_CONNECTIONS").and_then(|v| v.parse().ok()).unwrap_or(10),
            connect_burst: env("HOLLOWRITE_CONNECT_BURST").and_then(|v| v.parse().ok()).unwrap_or(20.0),
            registrations_per_hour: env("HOLLOWRITE_REGISTRATIONS_PER_HOUR")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10.0),
            public_metrics: env("HOLLOWRITE_PUBLIC_METRICS").is_some_and(|v| v == "1" || v == "true"),
        })
    }
}
