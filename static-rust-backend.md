# Rust Backend Design

Drop-in replacement for the Python/Flask backend. The React frontend is untouched — same HTTP+SSE API contract, different implementation language.

## API Contract

The frontend talks to `VITE_API_URL` (default `/api`). Every endpoint below must be preserved exactly.

### REST endpoints

| Method | Path | Request | Response |
|--------|------|---------|----------|
| GET | `/api/search?q=<query>` | — | `{ albums: Album[] }` or `{ error: string }` |
| GET | `/api/resolve/<album_id>` | — | SSE stream: `meta` → `track`×N → `done` (or `error`) |
| POST | `/api/start` | `{ artist, album, tracks: Track[], resolved: bool, total_tracks: int }` | `{ job_id: string }` |
| POST | `/api/jobs/<id>/tracks` | `{ tracks: Track[] }` | `{ ok: true }` |
| POST | `/api/jobs/<id>/resolve` | — | `{ ok: true }` |
| GET | `/api/jobs` | — | `Job[]` |
| POST | `/api/jobs/clear` | — | `{ ok: true }` |
| GET | `/api/status?last_id=<n>` | — | SSE stream (long-lived) |

### SSE event types on `/api/status`

Each event carries `id: <monotonic_seq>` for `Last-Event-ID` resumption.

- **`job_update`** — `{ job_id, status, artist?, album?, track_count? }`
- **`track_update`** — `{ job_id, index, title, status: "downloading"|"transcoding"|"done"|"error", error? }`
- **`track_progress`** — `{ job_id, index, phase: "download"|"transcode", pct: 0-100 }`
- **`log`** — `{ message, job_id? }`

### SSE event types on `/api/resolve/<album_id>`

- **`meta`** — `{ artist, album, matched_artist, existing_artists, total }`
- **`track`** — `{ index, title, artist, album, duration, url }`
- **`done`** — `{}`
- **`error`** — `{ error: string }`

### Data types

```
Album  { id: u64, title: String, artist: String, cover: String, tracks: u32, year: String }
Track  { index: u32, title: String, artist: String, album: String, duration: String, url: String }
Job    { id: String, artist: String, album: String, status: "queued"|"active"|"done"|"error",
         current_track: Option<u32>, track_count: u32, tracks_done: u32 }
```

## Architecture

```
main()
  ├── axum Router (HTTP + SSE)
  ├── AppState (Arc<RwLock<...>>)
  │     ├── jobs: Vec<Job>
  │     ├── event_ring: VecDeque<(u64, EventType, Value)>  // 500 cap
  │     └── event_counter: AtomicU64
  └── worker task (tokio::spawn)
        └── loops over queued jobs
              └── per-track: tokio::spawn with Semaphore permit
                    ├── download (reqwest, streaming)
                    ├── transcode (spawn_blocking → libav*)
                    ├── tag (lofty)
                    └── lyrics (lrclib.net → SYLT embed)
```

### Concurrency model

- **Job processing**: single tokio task polls `AppState.jobs` for `status == "queued"`, processes one job at a time (matches current behavior).
- **Track processing within a job**: `tokio::spawn` per track, bounded by `Semaphore(MAX_CONCURRENT)` (env var, default 4). All tracks in a job run concurrently up to the semaphore limit.
- **CPU-bound transcode**: `tokio::task::spawn_blocking` wraps the libav* decode→encode loop so the tokio runtime isn't blocked.
- **State access**: `Arc<tokio::sync::RwLock<AppState>>` — reads (SSE, job list) take read locks, mutations (job status, event push) take write locks. Event counter is `AtomicU64` to avoid locking on every SSE emit.

### Mirrors / retry logic

Same mirror list as Python. `squid_get()` becomes an async fn that tries each mirror sequentially, skipping 429s. `reqwest::Client` with connection pooling, reused across all requests.

```rust
static MIRRORS: &[&str] = &[
    "https://triton.squid.wtf",
    "https://arran.monochrome.tf",
    "https://hifi-one.spotisaver.net",
    "https://hifi-two.spotisaver.net",
    "https://hund.qqdl.site",
    "https://katze.qqdl.site",
    "https://maus.qqdl.site",
];
```

## Streaming Transcode Pipeline

The core win over the Python version. Instead of download-to-disk → shell out to ffmpeg → delete temp file, we do it in-process with no temp files.

### Current flow (Python)
```
HTTP GET → disk (full FLAC) → ffmpeg subprocess → disk (MP3) → delete FLAC
```

### New flow (Rust)
```
HTTP GET → memory buffer → libavcodec FLAC decode → libavcodec MP3 encode → disk (MP3)
```

### Implementation detail

The streaming transcode runs inside `spawn_blocking` since libav* calls are synchronous and CPU-bound:

1. **Download phase**: `reqwest` streams the response body. Chunks accumulate in a bounded buffer (`Vec<u8>`). Download progress is emitted based on `Content-Length`.
2. **Handoff**: once the full FLAC is in memory (we need the full file for demuxing — FLAC container metadata is at the start but seeking may be needed), pass the buffer to the transcode task.
3. **Transcode phase**: open the in-memory FLAC via `avio_alloc_context` with a custom read callback over the `Vec<u8>`. Decode FLAC frames → resample if needed (libswresample) → encode to MP3 via libmp3lame → write output frames directly to the target MP3 file.
4. **Progress**: track `out_time_us` from the encoder relative to the known duration, emit `track_progress` events at 5% increments.

> **Why not true streaming decode-while-downloading?** FLAC's container format (and the libav demuxer) needs to read the stream header and seek table up front. We'd need the full file anyway, or a custom demuxer that handles incomplete data. Buffering the full FLAC in memory (~30-50MB per track) is simple and still eliminates temp FLAC files on disk. For a typical album download with 4 concurrent tracks, peak memory is ~120-200MB — well within the 256Mi pod limit since the Rust binary itself is tiny.

### libav* API surface

```rust
// Pseudo-code for the transcode core
fn transcode(flac_data: &[u8], mp3_path: &Path, metadata: &TrackMeta) -> Result<()> {
    // 1. Create custom AVIOContext reading from flac_data slice
    // 2. Open input format context (FLAC demuxer auto-detected)
    // 3. Find audio stream, get decoder (FLAC)
    // 4. Create output format context for mp3_path
    // 5. Add audio stream, get encoder (libmp3lame), set bitrate 320k
    // 6. Set metadata (title, artist, album, track)
    // 7. Open resampler if sample formats differ (FLAC is s16/s32, MP3 wants s16p/fltp)
    // 8. Read packets → decode frames → resample → encode frames → write packets
    // 9. Flush encoder, write trailer
}
```

## ffmpeg-next Bindings & Static Build

### Minimal ffmpeg configuration

```bash
./configure \
  --disable-everything \
  --disable-programs \
  --disable-doc \
  --disable-network \
  --disable-autodetect \
  --enable-small \
  --enable-decoder=flac \
  --enable-encoder=libmp3lame \
  --enable-demuxer=flac \
  --enable-muxer=mp3 \
  --enable-protocol=file \
  --enable-protocol=pipe \
  --enable-libmp3lame \
  --enable-swresample \
  --enable-gpl \
  --enable-static \
  --disable-shared \
  --prefix=/opt/ffmpeg
```

This pulls in only:
- `libavcodec` (FLAC decoder + libmp3lame encoder)
- `libavformat` (FLAC demuxer + MP3 muxer + file/pipe protocols)
- `libswresample` (sample format conversion)
- `libavutil` (required by all above)

No video, no filters, no hwaccel, no network protocols, no subtitle parsers.

### Rust crate: `ffmpeg-next`

Bindgen wrapper around the C libraries. Set environment variables to point at our custom-built ffmpeg:

```toml
[dependencies]
ffmpeg-next = { version = "7", features = ["static"] }
```

```bash
export FFMPEG_DIR=/opt/ffmpeg
export PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig
```

### lame dependency

`libmp3lame` is an external library that ffmpeg links against. Must be built as a static lib too:

```bash
cd lame-3.100
./configure --disable-shared --enable-static --enable-nasm --prefix=/opt/lame
make -j$(nproc) && make install
export LAME_DIR=/opt/lame  # ffmpeg's --enable-libmp3lame finds it via pkg-config
```

## ID3 Tagging

Use `lofty` crate — pure Rust, no C dependencies, supports ID3v2.4 including SYLT (synced lyrics).

```rust
use lofty::{Tag, TagExt, TaggedFileExt, Accessor};
use lofty::id3::v2::{Id3v2Tag, SyncTextFrame, TimestampFormat, SyncTextContentType};

fn tag_mp3(path: &Path, meta: &TrackMeta, lyrics: Option<Vec<(String, u32)>>) -> Result<()> {
    let mut tag = Id3v2Tag::default();
    tag.set_title(meta.title.clone());
    tag.set_artist(meta.artist.clone());
    tag.set_album(meta.album.clone());
    tag.set_track(meta.index as u32);
    tag.set_track_total(meta.total as u32);

    if let Some(lyrics) = lyrics {
        // Add SYLT frame for synced lyrics
        tag.insert(SyncTextFrame {
            timestamp_format: TimestampFormat::MS,
            content_type: SyncTextContentType::Lyrics,
            description: String::new(),
            language: *b"eng",
            content: lyrics,
        }.into());
    }

    tag.save_to_path(path)?;
    Ok(())
}
```

Note: ffmpeg's `-metadata` flags in the Python version set ID3 tags during encoding. In the Rust version, we skip metadata in the encoder and do a second pass with `lofty` after the MP3 is written. This is cleaner — separation of concerns, and lofty gives us SYLT support that ffmpeg's muxer doesn't.

## Lyrics

Same lrclib.net API. Async reqwest call, parse LRC timestamps into `Vec<(String, u32)>` for SYLT embedding.

```rust
async fn fetch_lyrics(title: &str, artist: &str, album: &str) -> Option<Vec<(String, u32)>> {
    let resp = CLIENT.get("https://lrclib.net/api/search")
        .query(&[("track_name", title), ("artist_name", artist), ("album_name", album)])
        .timeout(Duration::from_secs(10))
        .send().await.ok()?;

    let results: Vec<LrclibResult> = resp.json().await.ok()?;
    results.iter()
        .find_map(|r| r.synced_lyrics.as_deref())
        .map(parse_lrc)
}
```

## Navidrome Rescan

Same Subsonic API, same MD5 token auth. Async reqwest call.

## File Layout & Filename Sanitization

Same convention: `{MUSIC_DIR}/{artist}/{album}/{index:02} - {title}.mp3`

Sanitization: strip `<>:"/\|?*`, trim dots and spaces. Same regex as Python.

## Crate Dependencies

```toml
[dependencies]
axum = { version = "0.8", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["stream", "json"] }
ffmpeg-next = { version = "7", features = ["static"] }
lofty = "0.22"
uuid = { version = "1", features = ["v4"] }
md-5 = "0.10"
rand = "0.8"
base64 = "0.22"
regex = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tower-http = { version = "0.6", features = ["cors"] }
tokio-stream = "0.1"
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MUSIC_DIR` | `/music` | Output directory for downloaded music |
| `SQUID_API` | `https://triton.squid.wtf` | Primary Squid mirror (prepended to default list) |
| `NAVIDROME_URL` | `http://navidrome:4533` | Navidrome server URL |
| `NAVIDROME_USER` | — | Navidrome username for rescan |
| `NAVIDROME_PASS` | — | Navidrome password for rescan |
| `MAX_CONCURRENT` | `4` | Max parallel track downloads+transcodes per job |
| `LOG_LEVEL` | `debug` | tracing filter directive |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address |

## Dockerfile

```dockerfile
# --- Stage 1: build lame + ffmpeg static libs ---
FROM debian:bookworm-slim AS ffmpeg-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential nasm pkg-config curl ca-certificates xz-utils && \
    rm -rf /var/lib/apt/lists/*

# Build static lame
WORKDIR /build/lame
RUN curl -L https://downloads.sourceforge.net/lame/lame-3.100.tar.gz | tar xz --strip-components=1 && \
    ./configure --disable-shared --enable-static --enable-nasm --prefix=/opt/lame && \
    make -j$(nproc) && make install

# Build static ffmpeg (minimal)
WORKDIR /build/ffmpeg
RUN curl -L https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz | tar xJ --strip-components=1 && \
    PKG_CONFIG_PATH=/opt/lame/lib/pkgconfig ./configure \
      --disable-everything --disable-programs --disable-doc --disable-network --disable-autodetect \
      --enable-small \
      --enable-decoder=flac --enable-encoder=libmp3lame \
      --enable-demuxer=flac --enable-muxer=mp3 \
      --enable-protocol=file --enable-protocol=pipe \
      --enable-libmp3lame --enable-swresample --enable-gpl \
      --enable-static --disable-shared \
      --extra-cflags="-I/opt/lame/include" --extra-ldflags="-L/opt/lame/lib" \
      --prefix=/opt/ffmpeg && \
    make -j$(nproc) && make install

# --- Stage 2: build Rust binary ---
FROM rust:1.86-bookworm AS rust-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config clang && \
    rm -rf /var/lib/apt/lists/*

COPY --from=ffmpeg-builder /opt/ffmpeg /opt/ffmpeg
COPY --from=ffmpeg-builder /opt/lame /opt/lame

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

ENV FFMPEG_DIR=/opt/ffmpeg
ENV PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig:/opt/lame/lib/pkgconfig
ENV FFMPEG_PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig

RUN cargo build --release

# --- Stage 3: minimal runtime ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -r -u 1000 app

COPY --from=rust-builder /app/target/release/music-downloader /usr/local/bin/music-downloader

USER 1000
EXPOSE 8080

CMD ["music-downloader"]
```

Final image: ~20-30MB (debian-slim base + single static binary + ca-certificates).

## CI Changes

Update `.gitlab-ci.yml` `build-backend` job — no changes needed since it just runs `docker build` with kaniko. The Dockerfile change is self-contained.

Build time will increase (ffmpeg compile + Rust compile). Mitigations:
- kaniko `--cache=true` caches Docker layers — ffmpeg build layer rarely changes
- `cargo build` benefits from layer caching if `Cargo.toml`/`Cargo.lock` are copied before `src/`
- Consider a pre-built ffmpeg-builder image pushed to the registry as a base

## k8s Changes

Minimal:
- Bump resource limits: `cpu: 1000m` (transcode is CPU-hungry), `memory: 512Mi` (in-memory FLAC buffers)
- Add `MAX_CONCURRENT` env var to deployment
- Everything else (PVC, service, routes) stays the same

## Migration Path

1. Scaffold Rust project, get `cargo build` working with ffmpeg-next static linking
2. Implement HTTP routes + SSE with axum (matching API contract exactly)
3. Implement Squid API client (search, resolve, track URL resolution)
4. Implement in-memory transcode pipeline (FLAC→MP3 via libav*)
5. Implement ID3 tagging with lofty (including SYLT lyrics)
6. Implement lyrics fetching from lrclib.net
7. Implement Navidrome rescan
8. Implement job queue + worker + concurrency (semaphore-bounded parallel tracks)
9. Wire up SSE event emission + log forwarding
10. Dockerfile multi-stage build
11. Test against live Squid API + Navidrome
12. Swap image in deployment, verify frontend works unchanged
