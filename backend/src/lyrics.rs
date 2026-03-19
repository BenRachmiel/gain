use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

const LRCLIB_API: &str = "https://lrclib.net/api";

static LRC_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(\d+):(\d+(?:\.\d+)?)\](.*)").unwrap());

#[derive(Deserialize)]
struct LrclibResult {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

/// Parse LRC text into (text, timestamp_ms) pairs for SYLT embedding.
fn parse_lrc(lrc: &str) -> Vec<(String, u32)> {
    lrc.lines()
        .filter_map(|line| {
            let caps = LRC_LINE.captures(line)?;
            let mins: f64 = caps[1].parse().ok()?;
            let secs: f64 = caps[2].parse().ok()?;
            let ms = (mins * 60_000.0 + secs * 1_000.0) as u32;
            let text = caps[3].trim().to_string();
            Some((text, ms))
        })
        .collect()
}

/// Fetch synced lyrics from lrclib.net. Returns parsed SYLT list or None.
pub async fn fetch_lyrics(title: &str, artist: &str, album: &str) -> Option<Vec<(String, u32)>> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{LRCLIB_API}/search"))
        .query(&[
            ("track_name", title),
            ("artist_name", artist),
            ("album_name", album),
        ])
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::debug!("lrclib search {title:?} → HTTP {}", resp.status());
        return None;
    }

    let results: Vec<LrclibResult> = resp.json().await.ok()?;
    for result in &results {
        if let Some(lrc) = &result.synced_lyrics {
            if !lrc.is_empty() {
                tracing::debug!("lrclib found synced lyrics for {title:?}");
                return Some(parse_lrc(lrc));
            }
        }
    }

    tracing::debug!("lrclib: no synced lyrics for {title:?}");
    None
}
