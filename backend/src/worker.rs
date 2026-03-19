use serde_json::json;
use std::path::PathBuf;
use tokio::sync::Semaphore;

use crate::state::{AppState, JobStatus, Track};
use crate::transcode;
use crate::util;

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        loop {
            let job_id = {
                let inner = state.inner.read().await;
                inner
                    .jobs
                    .iter()
                    .find(|j| j.status == JobStatus::Queued)
                    .map(|j| j.id.clone())
            };

            if let Some(job_id) = job_id {
                process_job(&state, &job_id).await;
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
}

async fn process_job(state: &AppState, job_id: &str) {
    // Mark active
    {
        let mut inner = state.inner.write().await;
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = JobStatus::Active;
            job.tracks_done = 0;
        }
    }

    let (artist, album) = {
        let inner = state.inner.read().await;
        let job = inner.jobs.iter().find(|j| j.id == job_id).unwrap();
        (job.artist.clone(), job.album.clone())
    };

    tracing::info!(job_id, "Starting: {artist} — {album}");
    state.log(format!("Starting: {artist} — {album}")).await;
    state
        .emit("job_update", json!({"job_id": job_id, "status": "active"}))
        .await;

    let target_dir = PathBuf::from(&state.config.music_dir)
        .join(util::clean_filename(&artist))
        .join(util::clean_filename(&album));

    if let Err(e) = tokio::fs::create_dir_all(&target_dir).await {
        tracing::error!(job_id, "Failed to create directory {}: {e}", target_dir.display());
        let mut inner = state.inner.write().await;
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = JobStatus::Error;
        }
        state
            .emit("job_update", json!({"job_id": job_id, "status": "error"}))
            .await;
        return;
    }

    let semaphore = std::sync::Arc::new(Semaphore::new(state.config.max_concurrent));
    let mut processed = 0;

    loop {
        // Get unprocessed tracks
        let tracks: Vec<Track> = {
            let inner = state.inner.read().await;
            let job = inner.jobs.iter().find(|j| j.id == job_id).unwrap();
            job.tracks[processed..].to_vec()
        };

        if !tracks.is_empty() {
            let mut handles = Vec::new();

            for track in tracks {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let state = state.clone();
                let job_id = job_id.to_string();
                let target_dir = target_dir.clone();

                handles.push(tokio::spawn(async move {
                    process_track(&state, &job_id, &track, &target_dir).await;
                    drop(permit);
                }));
                processed += 1;
            }

            for handle in handles {
                let _ = handle.await;
            }
        }

        // Check if resolved and all tracks processed
        let resolved = {
            let inner = state.inner.read().await;
            let job = inner.jobs.iter().find(|j| j.id == job_id).unwrap();
            job.resolved
        };

        if resolved {
            // Check again if new tracks arrived between last check and resolved
            let total = {
                let inner = state.inner.read().await;
                inner.jobs.iter().find(|j| j.id == job_id).unwrap().tracks.len()
            };
            if processed >= total {
                break;
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    // Mark done
    {
        let mut inner = state.inner.write().await;
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = JobStatus::Done;
            job.current_track = None;
        }
    }

    state
        .emit("job_update", json!({"job_id": job_id, "status": "done"}))
        .await;
    tracing::info!(job_id, "Finished: {artist} — {album}");
    state.log(format!("Finished: {artist} — {album}")).await;
}

async fn process_track(state: &AppState, job_id: &str, track: &Track, target_dir: &std::path::Path) {
    let idx = track.index;
    let title = &track.title;
    let safe_title = util::clean_filename(title);
    let mp3_path = target_dir.join(format!("{idx:02} - {safe_title}.mp3"));
    let duration_s = util::parse_duration(&track.duration);

    let total = {
        let inner = state.inner.read().await;
        let job = inner.jobs.iter().find(|j| j.id == job_id).unwrap();
        job.total_tracks.unwrap_or(job.tracks.len() as u32)
    };

    // Update current track
    {
        let mut inner = state.inner.write().await;
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
            job.current_track = Some(idx);
        }
    }

    tracing::info!(job_id, "Track {idx}/{total}: {title} — downloading");
    state.log(format!("[{idx}/{total}] Downloading: {title}")).await;
    state
        .emit(
            "track_update",
            json!({"job_id": job_id, "index": idx, "title": title, "status": "downloading"}),
        )
        .await;

    // Download + transcode
    state
        .emit(
            "track_update",
            json!({"job_id": job_id, "index": idx, "title": title, "status": "downloading"}),
        )
        .await;

    match transcode::download_and_transcode(state, &track.url, &mp3_path, job_id, idx, duration_s)
        .await
    {
        Ok(()) => {
            state
                .emit(
                    "track_update",
                    json!({"job_id": job_id, "index": idx, "title": title, "status": "transcoding"}),
                )
                .await;

            // Tag + lyrics
            let artist = &track.artist;
            let album = &track.album;
            if let Err(e) = crate::tagging::tag_mp3(&mp3_path, title, artist, album, idx, total) {
                tracing::warn!(job_id, "Tagging failed for {title}: {e}");
            }

            match crate::lyrics::fetch_lyrics(title, artist, album).await {
                Some(sylt) => {
                    if let Err(e) = crate::tagging::embed_lyrics(&mp3_path, &sylt) {
                        tracing::warn!(job_id, "Lyrics embed failed for {title}: {e}");
                    } else {
                        tracing::info!(job_id, "Lyrics embedded: {title}");
                    }
                }
                None => {
                    tracing::debug!(job_id, "No lyrics found: {title}");
                }
            }

            {
                let mut inner = state.inner.write().await;
                if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
                    job.tracks_done += 1;
                }
            }

            state
                .emit(
                    "track_update",
                    json!({"job_id": job_id, "index": idx, "title": title, "status": "done"}),
                )
                .await;

            tracing::info!(job_id, "Track {idx}: done");
            state.log(format!("[{idx}/{total}] Done: {title}")).await;
        }
        Err(e) => {
            tracing::error!(job_id, "Track {idx} failed: {e}");
            state.log(format!("[{idx}/{total}] FAILED: {title} — {e}")).await;

            {
                let mut inner = state.inner.write().await;
                if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == job_id) {
                    job.tracks_done += 1;
                }
            }

            state
                .emit(
                    "track_update",
                    json!({
                        "job_id": job_id,
                        "index": idx,
                        "title": title,
                        "status": "error",
                        "error": e.to_string(),
                    }),
                )
                .await;
        }
    }
}
