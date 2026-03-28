use axum::response::sse::Event;
use serde_json::json;
use tokio::sync::mpsc;

use super::{scan_artists, DownloadSource, SearchAlbum, SourceType};
use crate::state::AppState;
use crate::util;

#[cfg(test)]
use serde_json::Value;

pub struct YouTubeSource {
    pub yt_dlp_path: String,
}

impl YouTubeSource {
    pub fn new(yt_dlp_path: String) -> Self {
        Self { yt_dlp_path }
    }

    async fn run_yt_dlp(
        &self,
        args: &[&str],
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let output = tokio::process::Command::new(&self.yt_dlp_path)
            .args(args)
            .output()
            .await
            .map_err(|e| format!("yt-dlp exec failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp failed: {stderr}").into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn extract_thumbnail(entry: &serde_json::Value) -> String {
        entry["thumbnail"]
            .as_str()
            .or_else(|| {
                entry["thumbnails"]
                    .as_array()
                    .and_then(|t| t.last())
                    .and_then(|t| t["url"].as_str())
            })
            .unwrap_or("")
            .to_string()
    }

    fn is_playlist_entry(entry: &serde_json::Value) -> bool {
        // yt-dlp search results use ie_key to distinguish types
        entry["ie_key"].as_str() == Some("YoutubeTab")
            || entry["_type"].as_str() == Some("playlist")
    }

    fn parse_video_entry(entry: &serde_json::Value) -> Option<SearchAlbum> {
        let id = entry["id"].as_str()?;
        let title = entry["title"].as_str().unwrap_or("Unknown").to_string();
        let uploader = entry["uploader"]
            .as_str()
            .or_else(|| entry["channel"].as_str())
            .unwrap_or("Unknown")
            .to_string();

        Some(SearchAlbum {
            id: format!("yt:{id}"),
            title,
            artist: uploader,
            cover: Self::extract_thumbnail(entry),
            tracks: 1,
            year: String::new(),
            source: SourceType::YouTube,
            kind: Some("video".to_string()),
        })
    }

    fn parse_playlist_entry(entry: &serde_json::Value) -> Option<SearchAlbum> {
        let id = entry["id"].as_str()?;
        let title = entry["title"].as_str().unwrap_or("Unknown").to_string();
        // Playlists from search results often lack uploader info
        let uploader = entry["uploader"]
            .as_str()
            .or_else(|| entry["channel"].as_str())
            .unwrap_or("")
            .to_string();
        let count = entry["playlist_count"]
            .as_u64()
            .or_else(|| entry["n_entries"].as_u64())
            .unwrap_or(0) as u32;

        Some(SearchAlbum {
            id: format!("ytpl:{id}"),
            title,
            artist: uploader,
            cover: Self::extract_thumbnail(entry),
            tracks: count,
            year: String::new(),
            source: SourceType::YouTube,
            kind: Some("playlist".to_string()),
        })
    }

    async fn resolve_video_url(
        &self,
        video_id: &str,
    ) -> Result<(String, String, String, u64), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://www.youtube.com/watch?v={video_id}");
        let output = self
            .run_yt_dlp(&[
                "-f",
                "bestaudio",
                "--dump-json",
                "--no-playlist",
                &url,
            ])
            .await?;

        let data: serde_json::Value = serde_json::from_str(output.trim())?;
        let audio_url = data["url"]
            .as_str()
            .ok_or("no audio URL in yt-dlp output")?
            .to_string();
        let title = data["title"].as_str().unwrap_or("Unknown").to_string();
        let uploader = data["uploader"]
            .as_str()
            .or_else(|| data["channel"].as_str())
            .unwrap_or("Unknown")
            .to_string();
        let duration = data["duration"].as_f64().unwrap_or(0.0) as u64;

        Ok((audio_url, title, uploader, duration))
    }

    /// Resolve just the audio stream URL for a video ID. Used by the worker at download time.
    pub async fn resolve_audio_url(
        &self,
        video_id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://www.youtube.com/watch?v={video_id}");
        let output = self
            .run_yt_dlp(&["-f", "bestaudio", "--get-url", "--no-playlist", &url])
            .await?;
        let audio_url = output.trim().to_string();
        if audio_url.is_empty() {
            return Err("yt-dlp returned no URL".into());
        }
        Ok(audio_url)
    }

    fn is_youtube_url(query: &str) -> bool {
        query.contains("youtube.com/") || query.contains("youtu.be/")
    }

    /// Handle a direct YouTube URL (video or playlist) as a search result
    async fn search_url(
        &self,
        url: &str,
    ) -> Result<Vec<SearchAlbum>, Box<dyn std::error::Error + Send + Sync>> {
        let output = self
            .run_yt_dlp(&["--dump-json", "--flat-playlist", "--no-warnings", url])
            .await?;

        let entries: Vec<serde_json::Value> = output
            .lines()
            .filter_map(|l| serde_json::from_str(l.trim()).ok())
            .collect();

        if entries.is_empty() {
            return Ok(vec![]);
        }

        // Check if this is a playlist (multiple entries) or single video
        if entries.len() == 1 {
            let entry = &entries[0];
            if let Some(album) = Self::parse_video_entry(entry) {
                return Ok(vec![album]);
            }
        }

        // It's a playlist — return as a single playlist entry
        let first = &entries[0];
        let playlist_title = first["playlist_title"]
            .as_str()
            .or_else(|| first["playlist"].as_str())
            .unwrap_or("YouTube Playlist")
            .to_string();
        let uploader = first["uploader"]
            .as_str()
            .or_else(|| first["channel"].as_str())
            .unwrap_or("")
            .to_string();

        // Extract playlist ID from first entry
        let playlist_id = first["playlist_id"]
            .as_str()
            .unwrap_or_else(|| {
                // Fall back to parsing from URL
                url.split("list=")
                    .nth(1)
                    .and_then(|s| s.split('&').next())
                    .unwrap_or("unknown")
            })
            .to_string();

        Ok(vec![SearchAlbum {
            id: format!("ytpl:{playlist_id}"),
            title: playlist_title,
            artist: uploader,
            cover: Self::extract_thumbnail(first),
            tracks: entries.len() as u32,
            year: String::new(),
            source: SourceType::YouTube,
            kind: Some("playlist".to_string()),
        }])
    }
}

const PAGE_SIZE: u32 = 20;

#[async_trait::async_trait]
impl DownloadSource for YouTubeSource {
    async fn search(
        &self,
        query: &str,
        page: u32,
        _state: &AppState,
    ) -> Result<Vec<SearchAlbum>, Box<dyn std::error::Error + Send + Sync>> {
        // If the query is a YouTube URL, resolve it directly
        if Self::is_youtube_url(query) {
            return self.search_url(query).await;
        }

        let mut results = Vec::new();

        let page = page.max(1);
        let start = (page - 1) * PAGE_SIZE + 1;
        let end = start + PAGE_SIZE - 1;
        let range = format!("{start}:{end}");

        // Use YouTube search URL directly — returns videos, playlists, and channels
        let search_url = format!(
            "https://www.youtube.com/results?search_query={}",
            urlencoding::encode(query)
        );
        let output = self
            .run_yt_dlp(&[
                "--dump-json",
                "--flat-playlist",
                "--no-warnings",
                "--playlist-items",
                &range,
                &search_url,
            ])
            .await?;

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if Self::is_playlist_entry(&entry) {
                    if let Some(album) = Self::parse_playlist_entry(&entry) {
                        results.push(album);
                    }
                } else if let Some(album) = Self::parse_video_entry(&entry) {
                    results.push(album);
                }
            }
        }

        Ok(results)
    }

    async fn resolve_album(
        &self,
        album_id: &str,
        state: &AppState,
    ) -> Result<mpsc::Receiver<Event>, Box<dyn std::error::Error + Send + Sync>> {
        let (tx, rx) = mpsc::channel(32);
        let yt_dlp_path = self.yt_dlp_path.clone();
        let album_id = album_id.to_string();
        let state = state.clone();

        tokio::spawn(async move {
            let yt = YouTubeSource::new(yt_dlp_path);

            let existing_artists = scan_artists(&state.config.music_dir);

            if let Some(video_id) = album_id.strip_prefix("yt:") {
                // Single video
                match yt.resolve_video_url(video_id).await {
                    Ok((url, title, uploader, duration)) => {
                        let matched = util::match_artist(&uploader, &existing_artists)
                            .map(|s| s.to_string());
                        let mins = duration / 60;
                        let secs = duration % 60;

                        let meta = json!({
                            "artist": uploader,
                            "album": title,
                            "matched_artist": matched,
                            "existing_artists": existing_artists,
                            "total": 1,
                        });
                        let _ = tx
                            .send(Event::default().event("meta").json_data(&meta).unwrap())
                            .await;

                        let track = json!({
                            "index": 1,
                            "title": title,
                            "artist": uploader,
                            "album": title,
                            "duration": format!("{mins}:{secs:02}"),
                            "url": url,
                        });
                        let _ = tx
                            .send(Event::default().event("track").json_data(&track).unwrap())
                            .await;
                    }
                    Err(e) => {
                        let err = json!({"index": 1, "title": "Unknown", "error": e.to_string()});
                        let _ = tx
                            .send(
                                Event::default()
                                    .event("track_error")
                                    .json_data(&err)
                                    .unwrap(),
                            )
                            .await;
                    }
                }
            } else if let Some(playlist_id) = album_id.strip_prefix("ytpl:") {
                // Playlist — get full metadata
                let playlist_url =
                    format!("https://www.youtube.com/playlist?list={playlist_id}");
                let dump_output = yt
                    .run_yt_dlp(&[
                        "--dump-json",
                        "--flat-playlist",
                        "--no-warnings",
                        &playlist_url,
                    ])
                    .await;

                let entries: Vec<serde_json::Value> = match dump_output {
                    Ok(output) => output
                        .lines()
                        .filter_map(|line| serde_json::from_str(line.trim()).ok())
                        .collect(),
                    Err(e) => {
                        let err = json!({"error": e.to_string()});
                        let _ = tx
                            .send(
                                Event::default()
                                    .event("error")
                                    .json_data(&err)
                                    .unwrap(),
                            )
                            .await;
                        return;
                    }
                };

                // Use first entry's uploader as artist, playlist title from entries
                let uploader = entries
                    .first()
                    .and_then(|e| {
                        e["uploader"]
                            .as_str()
                            .or_else(|| e["channel"].as_str())
                    })
                    .unwrap_or("Unknown")
                    .to_string();

                // Try to get playlist title from first entry
                let playlist_title = entries
                    .first()
                    .and_then(|e| e["playlist_title"].as_str())
                    .unwrap_or("YouTube Playlist")
                    .to_string();

                let matched = util::match_artist(&uploader, &existing_artists)
                    .map(|s| s.to_string());

                let thumbnail = entries
                    .first()
                    .and_then(|e| {
                        e["thumbnail"].as_str().or_else(|| {
                            e["thumbnails"]
                                .as_array()
                                .and_then(|t| t.last())
                                .and_then(|t| t["url"].as_str())
                        })
                    })
                    .unwrap_or("");

                let meta = json!({
                    "artist": uploader,
                    "album": playlist_title,
                    "matched_artist": matched,
                    "existing_artists": existing_artists,
                    "total": entries.len(),
                    "cover_url": thumbnail,
                });
                let _ = tx
                    .send(Event::default().event("meta").json_data(&meta).unwrap())
                    .await;

                // Emit tracks immediately from flat-playlist data.
                // Use ytdl:{video_id} as URL — the worker resolves the actual
                // audio stream URL at download time.
                for (idx, entry) in entries.iter().enumerate() {
                    let video_id = match entry["id"].as_str() {
                        Some(id) => id,
                        None => continue,
                    };
                    let entry_title = entry["title"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string();
                    let duration = entry["duration"].as_f64().unwrap_or(0.0) as u64;
                    let mins = duration / 60;
                    let secs = duration % 60;

                    let track = json!({
                        "index": idx + 1,
                        "title": entry_title,
                        "artist": uploader,
                        "album": playlist_title,
                        "duration": format!("{mins}:{secs:02}"),
                        "url": format!("ytdl:{video_id}"),
                    });
                    let _ = tx
                        .send(
                            Event::default()
                                .event("track")
                                .json_data(&track)
                                .unwrap(),
                        )
                        .await;
                }
            } else {
                let err = json!({"error": format!("Unknown album ID format: {album_id}")});
                let _ = tx
                    .send(Event::default().event("error").json_data(&err).unwrap())
                    .await;
            }

            let _ = tx
                .send(Event::default().event("done").json_data(&json!({})).unwrap())
                .await;
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_video_json() -> Value {
        json!({
            "id": "dQw4w9WgXcQ",
            "title": "Rick Astley - Never Gonna Give You Up",
            "uploader": "Rick Astley",
            "channel": "Rick Astley",
            "thumbnail": "https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg",
            "duration": 213.0,
            "_type": "url",
            "ie_key": "Youtube"
        })
    }

    fn sample_playlist_json() -> Value {
        json!({
            "id": "PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf",
            "title": "Top Hits 2024",
            "uploader": "Various Artists",
            "channel": "Music Channel",
            "thumbnail": "https://i.ytimg.com/vi/abc/maxresdefault.jpg",
            "playlist_count": 25,
            "_type": "url",
            "ie_key": "YoutubeTab"
        })
    }

    #[test]
    fn parse_video_entry_basic() {
        let entry = sample_video_json();
        let album = YouTubeSource::parse_video_entry(&entry).unwrap();

        assert_eq!(album.id, "yt:dQw4w9WgXcQ");
        assert_eq!(album.title, "Rick Astley - Never Gonna Give You Up");
        assert_eq!(album.artist, "Rick Astley");
        assert_eq!(album.tracks, 1);
        assert_eq!(album.source, SourceType::YouTube);
        assert_eq!(album.kind.as_deref(), Some("video"));
        assert!(!album.cover.is_empty());
    }

    #[test]
    fn parse_video_entry_missing_uploader_uses_channel() {
        let entry = json!({
            "id": "abc123",
            "title": "Test Video",
            "channel": "Test Channel",
            "thumbnails": [
                {"url": "https://example.com/thumb_small.jpg"},
                {"url": "https://example.com/thumb_large.jpg"}
            ]
        });
        let album = YouTubeSource::parse_video_entry(&entry).unwrap();
        assert_eq!(album.artist, "Test Channel");
        assert_eq!(album.cover, "https://example.com/thumb_large.jpg");
    }

    #[test]
    fn parse_video_entry_missing_id_returns_none() {
        let entry = json!({"title": "No ID"});
        assert!(YouTubeSource::parse_video_entry(&entry).is_none());
    }

    #[test]
    fn parse_playlist_entry_basic() {
        let entry = sample_playlist_json();
        let album = YouTubeSource::parse_playlist_entry(&entry).unwrap();

        assert_eq!(album.id, "ytpl:PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf");
        assert_eq!(album.title, "Top Hits 2024");
        assert_eq!(album.artist, "Various Artists");
        assert_eq!(album.tracks, 25);
        assert_eq!(album.source, SourceType::YouTube);
        assert_eq!(album.kind.as_deref(), Some("playlist"));
    }

    #[test]
    fn parse_playlist_entry_uses_n_entries_fallback() {
        let entry = json!({
            "id": "PLtest",
            "title": "Test Playlist",
            "channel": "Channel",
            "n_entries": 10
        });
        let album = YouTubeSource::parse_playlist_entry(&entry).unwrap();
        assert_eq!(album.tracks, 10);
    }

    #[test]
    fn is_playlist_entry_detects_youtube_tab() {
        let video = sample_video_json();
        let playlist = sample_playlist_json();
        assert!(!YouTubeSource::is_playlist_entry(&video));
        assert!(YouTubeSource::is_playlist_entry(&playlist));
    }

    #[test]
    fn is_playlist_entry_legacy_type() {
        let entry = json!({"_type": "playlist", "id": "test"});
        assert!(YouTubeSource::is_playlist_entry(&entry));
    }

    #[test]
    fn parse_playlist_entry_no_uploader() {
        // Real yt-dlp search results often lack uploader for playlists
        let entry = json!({
            "id": "PLtest123",
            "title": "My Playlist",
            "_type": "url",
            "ie_key": "YoutubeTab",
            "thumbnails": [{"url": "https://example.com/thumb.jpg"}]
        });
        let album = YouTubeSource::parse_playlist_entry(&entry).unwrap();
        assert_eq!(album.artist, "");
        assert_eq!(album.tracks, 0);
    }

    #[test]
    fn search_album_serialization() {
        let album = SearchAlbum {
            id: "yt:abc".to_string(),
            title: "Test".to_string(),
            artist: "Artist".to_string(),
            cover: "https://example.com/cover.jpg".to_string(),
            tracks: 1,
            year: String::new(),
            source: SourceType::YouTube,
            kind: Some("video".to_string()),
        };
        let json = serde_json::to_value(&album).unwrap();
        assert_eq!(json["source"], "youtube");
        assert_eq!(json["kind"], "video");
        assert_eq!(json["id"], "yt:abc");
    }
}
