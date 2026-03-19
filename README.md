# gain

A music downloader with source API integration, FLAC-to-MP3 transcoding, lyrics embedding, and metadata completion. React frontend, Rust backend, oauth2-proxy sidecar.

## Why

Downloading, transcoding, tagging, and organizing music by hand is tedious. gain automates the full pipeline: search an album, resolve track URLs, download FLAC, transcode to 320k MP3, embed metadata and synced lyrics, drop it in the right folder.

```
React UI --- search, select, monitor
    |
    v
Rust Backend --- resolve, download, transcode, tag
    |
    v
Music Dir --- organized Artist/Album/Track.mp3
```

## Quick Start

```bash
# Backend
cd backend
SOURCE_API=https://your-source MUSIC_DIR=/path/to/music cargo run

# Frontend (dev)
cd frontend
npm install && npm run dev
```

### Docker Compose

```bash
docker compose up --build
```

### Helm

```bash
helm install gain chart/gain-downloader/ -f values.yaml -n pedals
```

Chart includes deployment (frontend + backend + oauth2-proxy), service, HTTPRoute (Gateway API), ExternalSecret support, and music PVC.

## Architecture

Three containers in a single pod:

| Container | Role | Port |
|-----------|------|------|
| Frontend | Nginx serving React SPA, proxies `/api/` to backend | 8080 |
| Backend | Rust (Axum) -- search, download, transcode, tag | 3000 |
| oauth2-proxy | OIDC auth in front of frontend | 4180 |

## Features

- Album search via source API with mirror failover
- FLAC download with progress streaming (SSE)
- FFmpeg transcode to 320k MP3 with real-time progress
- Synced lyrics from lrclib.net (SYLT frames)
- Artist name matching against existing library
- Job queue with concurrent downloads
- Metadata completion and cover art embedding (toolbox)

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `SOURCE_API` | *required* | Primary source API endpoint |
| `SOURCE_MIRRORS` | `SOURCE_API` | Comma-separated mirror list |
| `MUSIC_DIR` | `/music` | Music library path |
| `MAX_CONCURRENT` | `4` | Concurrent download jobs |
| `BIND_ADDR` | `0.0.0.0:8080` | Backend listen address |

## Toolbox

CLI tools for bulk library maintenance, run as one-shot k8s Jobs:

| Tool | Purpose |
|------|---------|
| `complete_metadata` | Fill missing year/genre/album_artist via MusicBrainz + source API |
| `embed_cover` | Fetch and embed cover art from CAA + Tidal CDN |
| `embed_lyrics` | Fetch and embed synced lyrics from lrclib.net |
| `find_gaps` | Find albums with missing tracks |
| `fix_artist` | Normalize artist names |

```bash
cd toolbox
python -m tools.complete_metadata /path/to/music
```

## Tech Stack

| Concern | Choice |
|---------|--------|
| Backend | Rust + Axum |
| Frontend | React + TypeScript + Vite + shadcn/ui |
| Transcoding | FFmpeg (statically linked in backend image) |
| Tag reading | `dhowden/tag` (toolbox), `lofty` (backend) |
| Auth | oauth2-proxy sidecar with OIDC |
| Deployment | Helm chart, distroless/scratch images |

## License

GPLv3 -- see [LICENSE](LICENSE).
