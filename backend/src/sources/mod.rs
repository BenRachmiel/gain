pub mod tidal;
pub mod youtube;

use axum::response::sse::Event;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    #[default]
    Tidal,
    YouTube,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tidal => write!(f, "tidal"),
            Self::YouTube => write!(f, "youtube"),
        }
    }
}

impl std::str::FromStr for SourceType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tidal" => Ok(Self::Tidal),
            "youtube" | "yt" => Ok(Self::YouTube),
            other => Err(format!("unknown source: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub cover: String,
    pub tracks: u32,
    pub year: String,
    pub source: SourceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[async_trait::async_trait]
pub trait DownloadSource: Send + Sync {
    async fn search(
        &self,
        query: &str,
        page: u32,
        state: &AppState,
    ) -> Result<Vec<SearchAlbum>, Box<dyn std::error::Error + Send + Sync>>;

    async fn resolve_album(
        &self,
        album_id: &str,
        state: &AppState,
    ) -> Result<mpsc::Receiver<Event>, Box<dyn std::error::Error + Send + Sync>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_type() {
        assert_eq!("tidal".parse::<SourceType>().unwrap(), SourceType::Tidal);
        assert_eq!("youtube".parse::<SourceType>().unwrap(), SourceType::YouTube);
        assert_eq!("yt".parse::<SourceType>().unwrap(), SourceType::YouTube);
        assert_eq!("YouTube".parse::<SourceType>().unwrap(), SourceType::YouTube);
        assert!("spotify".parse::<SourceType>().is_err());
    }

    #[test]
    fn source_type_default_is_tidal() {
        assert_eq!(SourceType::default(), SourceType::Tidal);
    }

    #[test]
    fn source_type_display() {
        assert_eq!(SourceType::Tidal.to_string(), "tidal");
        assert_eq!(SourceType::YouTube.to_string(), "youtube");
    }

    #[test]
    fn source_type_serde_roundtrip() {
        let json = serde_json::to_string(&SourceType::YouTube).unwrap();
        assert_eq!(json, "\"youtube\"");
        let parsed: SourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SourceType::YouTube);
    }
}

pub fn scan_artists(music_dir: &str) -> Vec<String> {
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
