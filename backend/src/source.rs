use axum::response::sse::Event;
use serde_json::json;

use crate::state::{Album, AppState};
use crate::util;

pub async fn source_get(
    state: &AppState,
    path: &str,
    query: &[(&str, &str)],
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for mirror in &state.config.source_mirrors {
        let url = format!("{mirror}{path}");
        match state
            .http_client
            .get(&url)
            .query(query)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().as_u16() == 429 {
                    last_err = Some(format!("429 from {mirror}").into());
                    continue;
                }
                let resp = resp.error_for_status()?;
                return Ok(resp.json().await?);
            }
            Err(e) => {
                last_err = Some(e.into());
                continue;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "All source mirrors failed".into()))
}

pub async fn search_albums(
    state: &AppState,
    query: &str,
) -> Result<Vec<Album>, Box<dyn std::error::Error + Send + Sync>> {
    let data = source_get(state, "/search/", &[("al", query)]).await?;

    let items = data
        .pointer("/data/albums/items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let albums: Vec<Album> = items
        .into_iter()
        .filter_map(|item| {
            let id = item["id"].as_u64()?;
            let title = item["title"].as_str()?.to_string();
            let artist = item
                .pointer("/artists/0/name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let cover = item["cover"].as_str().unwrap_or("").to_string();
            let cover_url = if cover.is_empty() {
                String::new()
            } else {
                format!(
                    "https://resources.tidal.com/images/{}/320x320.jpg",
                    cover.replace('-', "/")
                )
            };
            let tracks = item["numberOfTracks"].as_u64().unwrap_or(0) as u32;
            let year = item["releaseDate"]
                .as_str()
                .unwrap_or("")
                .get(..4)
                .unwrap_or("")
                .to_string();

            Some(Album {
                id,
                title,
                artist,
                cover: cover_url,
                tracks,
                year,
            })
        })
        .collect();

    Ok(albums)
}

pub async fn resolve_track_url(
    state: &AppState,
    track_id: u64,
) -> Option<String> {
    for mirror in &state.config.source_mirrors {
        let url = format!("{mirror}/track/");
        match state
            .http_client
            .get(&url)
            .query(&[("id", track_id.to_string()), ("quality", "LOSSLESS".into())])
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().as_u16() == 429 {
                    tracing::debug!("429 from {mirror} for track {track_id}, trying next");
                    continue;
                }
                if !resp.status().is_success() {
                    continue;
                }
                let data: serde_json::Value = match resp.json().await {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let manifest_b64 = match data.pointer("/data/manifest").and_then(|v| v.as_str()) {
                    Some(m) => m,
                    None => continue,
                };
                let manifest_bytes = match base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    manifest_b64,
                ) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let manifest: serde_json::Value = match serde_json::from_slice(&manifest_bytes) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(url) = manifest.pointer("/urls/0").and_then(|v| v.as_str()) {
                    tracing::debug!("Resolved track {track_id} via {mirror}");
                    return Some(url.to_string());
                }
            }
            Err(e) => {
                tracing::debug!("Mirror {mirror} failed for track {track_id}: {e}");
                continue;
            }
        }
    }
    tracing::error!("All mirrors failed for track {track_id}");
    None
}

pub async fn resolve_album_stream(
    state: &AppState,
    album_id: u64,
) -> Result<tokio::sync::mpsc::Receiver<Event>, Box<dyn std::error::Error + Send + Sync>> {
    let data = source_get(state, "/album/", &[("id", &album_id.to_string())]).await?;
    let album_data = data
        .get("data")
        .ok_or("Missing data field")?
        .clone();

    let state = state.clone();
    let (tx, rx) = tokio::sync::mpsc::channel(32);

    tokio::spawn(async move {
        let artist = album_data
            .pointer("/artist/name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let album_title = album_data["title"].as_str().unwrap_or("Unknown").to_string();

        // Clean album title
        let album_clean = regex::Regex::new(
            r"(?i)\s*\((?:Deluxe|Remaster|Expanded|Anniversary).*?\)\s*$",
        )
        .unwrap()
        .replace(&album_title, "")
        .to_string();

        let existing_artists = scan_artists(&state.config.music_dir);
        let matched = util::match_artist(&artist, &existing_artists).map(|s| s.to_string());

        let items = album_data["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let meta = json!({
            "artist": artist,
            "album": album_clean,
            "matched_artist": matched,
            "existing_artists": existing_artists,
            "total": items.len(),
        });

        let _ = tx
            .send(Event::default().event("meta").json_data(&meta).unwrap())
            .await;

        for item_wrapper in &items {
            let item = &item_wrapper["item"];
            let track_id = match item["id"].as_u64() {
                Some(id) => id,
                None => continue,
            };
            let idx = item["trackNumber"].as_u64().unwrap_or(1) as u32;
            let duration_s = item["duration"].as_u64().unwrap_or(0);
            let mins = duration_s / 60;
            let secs = duration_s % 60;

            let title = item["title"].as_str().unwrap_or("");
            match resolve_track_url(&state, track_id).await {
                Some(url) => {
                    let track = json!({
                        "index": idx,
                        "title": title,
                        "artist": artist,
                        "album": album_clean,
                        "duration": format!("{mins}:{secs:02}"),
                        "url": url,
                    });
                    let _ = tx
                        .send(Event::default().event("track").json_data(&track).unwrap())
                        .await;
                }
                None => {
                    tracing::warn!("Failed to resolve track {track_id}: {title}");
                    let err = json!({
                        "index": idx,
                        "title": title,
                        "error": format!("All mirrors failed for track {track_id}"),
                    });
                    let _ = tx
                        .send(Event::default().event("track_error").json_data(&err).unwrap())
                        .await;
                }
            }
        }

        let _ = tx
            .send(Event::default().event("done").json_data(&json!({})).unwrap())
            .await;
    });

    Ok(rx)
}

fn scan_artists(music_dir: &str) -> Vec<String> {
    let mut artists = Vec::new();
    if let Ok(entries) = std::fs::read_dir(music_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    artists.push(name.to_string());
                }
            }
        }
    }
    artists.sort();
    artists
}
