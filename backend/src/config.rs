use std::env;
use std::net::SocketAddr;

pub struct Config {
    pub music_dir: String,
    pub source_mirrors: Vec<String>,
    pub max_concurrent: usize,
    pub bind_addr: SocketAddr,
    pub preamp_scan_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let source_api = env::var("SOURCE_API").expect("SOURCE_API must be set");
        let mirrors: Vec<String> = env::var("SOURCE_MIRRORS")
            .unwrap_or_else(|_| source_api.clone())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let bind_addr: SocketAddr = env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .expect("invalid BIND_ADDR");

        Self {
            music_dir: env::var("MUSIC_DIR").unwrap_or_else(|_| "/music".into()),
            source_mirrors: mirrors,
            max_concurrent: env::var("MAX_CONCURRENT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
            bind_addr,
            preamp_scan_url: env::var("PREAMP_SCAN_URL").ok(),
        }
    }
}
