use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;

const EVENT_RING_CAP: usize = 500;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<RwLock<AppStateInner>>,
    pub event_counter: Arc<AtomicU64>,
    pub config: Arc<Config>,
    pub http_client: reqwest::Client,
}

pub struct AppStateInner {
    pub jobs: Vec<Job>,
    pub event_ring: VecDeque<(u64, String, Value)>,
}

impl AppStateInner {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            event_ring: VecDeque::with_capacity(EVENT_RING_CAP),
        }
    }
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            inner: Arc::new(RwLock::new(AppStateInner::new())),
            event_counter: Arc::new(AtomicU64::new(0)),
            config: Arc::new(config),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn log(&self, msg: impl Into<String>) {
        let message = msg.into();
        self.emit("log", serde_json::json!({"message": message})).await;
    }

    pub async fn emit(&self, event_type: &str, data: Value) {
        let seq = self.event_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let mut state = self.inner.write().await;
        if state.event_ring.len() >= EVENT_RING_CAP {
            state.event_ring.pop_front();
        }
        state
            .event_ring
            .push_back((seq, event_type.to_string(), data));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub artist: String,
    pub album: String,
    pub tracks: Vec<Track>,
    pub total_tracks: Option<u32>,
    pub resolved: bool,
    pub status: JobStatus,
    pub current_track: Option<u32>,
    pub tracks_done: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Active,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub index: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub cover: String,
    pub tracks: u32,
    pub year: String,
}
