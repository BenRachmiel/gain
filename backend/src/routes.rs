use axum::{
    Json,
    extract::{Path, Query, State},
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use tokio_stream::Stream;

use crate::source;
use crate::state::{AppState, Job, JobStatus, Track};

// --- Search ---

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
}

pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Json<Value> {
    let q = query.q.trim().to_string();
    if q.is_empty() {
        return Json(json!({"error": "No query"}));
    }
    match source::search_albums(&state, &q).await {
        Ok(albums) => Json(json!({"albums": albums})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

// --- Start job ---

#[derive(Deserialize)]
pub struct StartRequest {
    artist: String,
    album: String,
    tracks: Vec<Track>,
    #[serde(default = "default_true")]
    resolved: bool,
    total_tracks: Option<u32>,
    cover_url: Option<String>,
}

fn default_true() -> bool {
    true
}

pub async fn start_job(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> Json<Value> {
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let track_count = req.total_tracks.unwrap_or(req.tracks.len() as u32);

    let job = Job {
        id: id.clone(),
        artist: req.artist.clone(),
        album: req.album.clone(),
        tracks: req.tracks,
        total_tracks: req.total_tracks,
        resolved: req.resolved,
        cover_url: req.cover_url,
        status: JobStatus::Queued,
        current_track: None,
        tracks_done: 0,
    };

    state.inner.write().await.jobs.push(job);

    state
        .emit(
            "job_update",
            json!({
                "job_id": id,
                "status": "queued",
                "artist": req.artist,
                "album": req.album,
                "track_count": track_count,
            }),
        )
        .await;

    Json(json!({"job_id": id}))
}

// --- Append tracks ---

#[derive(Deserialize)]
pub struct AppendTracksRequest {
    tracks: Vec<Track>,
}

pub async fn append_tracks(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Json(req): Json<AppendTracksRequest>,
) -> (axum::http::StatusCode, Json<Value>) {
    let mut inner = state.inner.write().await;
    if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
        job.tracks.extend(req.tracks);
        (axum::http::StatusCode::OK, Json(json!({"ok": true})))
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({"error": "Job not found"})),
        )
    }
}

// --- Mark resolved ---

pub async fn mark_resolved(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> (axum::http::StatusCode, Json<Value>) {
    let (status, final_count) = {
        let mut inner = state.inner.write().await;
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
            job.resolved = true;
            let count = job.tracks.len() as u32;
            job.total_tracks = Some(count);
            let status = job.status;
            (Some(status), count)
        } else {
            (None, 0)
        }
    };

    match status {
        Some(status) => {
            state
                .emit(
                    "job_update",
                    json!({
                        "job_id": job_id,
                        "status": status,
                        "track_count": final_count,
                    }),
                )
                .await;
            (axum::http::StatusCode::OK, Json(json!({"ok": true})))
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({"error": "Job not found"})),
        ),
    }
}

// --- Get jobs ---

#[derive(Serialize)]
pub struct JobSummary {
    id: String,
    artist: String,
    album: String,
    status: JobStatus,
    current_track: Option<u32>,
    track_count: u32,
    tracks_done: u32,
}

pub async fn get_jobs(State(state): State<AppState>) -> Json<Vec<JobSummary>> {
    let inner = state.inner.read().await;
    let summaries: Vec<_> = inner
        .jobs
        .iter()
        .map(|j| JobSummary {
            id: j.id.clone(),
            artist: j.artist.clone(),
            album: j.album.clone(),
            status: j.status,
            current_track: j.current_track,
            track_count: j.total_tracks.unwrap_or(j.tracks.len() as u32),
            tracks_done: j.tracks_done,
        })
        .collect();
    Json(summaries)
}

// --- Clear jobs ---

pub async fn clear_jobs(State(state): State<AppState>) -> Json<Value> {
    let mut inner = state.inner.write().await;
    inner
        .jobs
        .retain(|j| !matches!(j.status, JobStatus::Done | JobStatus::Error));
    Json(json!({"ok": true}))
}

// --- SSE status stream ---

#[derive(Deserialize)]
pub struct StatusQuery {
    #[serde(default)]
    last_id: u64,
}

pub async fn status_stream(
    State(state): State<AppState>,
    Query(query): Query<StatusQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut last_id = query.last_id;

    let stream = async_stream::stream! {
        loop {
            let events: Vec<_> = {
                let inner = state.inner.read().await;
                inner.event_ring.iter()
                    .filter(|(seq, _, _)| *seq > last_id)
                    .cloned()
                    .collect()
            };

            for (seq, event_type, data) in events {
                last_id = seq;
                let event = Event::default()
                    .id(seq.to_string())
                    .event(&event_type)
                    .json_data(&data)
                    .unwrap();
                yield Ok(event);
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// --- Resolve album SSE ---

pub async fn resolve_album(
    State(state): State<AppState>,
    Path(album_id): Path<u64>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        match source::resolve_album_stream(&state, album_id).await {
            Ok(mut rx) => {
                while let Some(event) = rx.recv().await {
                    yield Ok::<_, Infallible>(event);
                }
            }
            Err(e) => {
                let event = Event::default()
                    .event("error")
                    .json_data(&json!({"error": e.to_string()}))
                    .unwrap();
                yield Ok::<_, Infallible>(event);
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
