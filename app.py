import base64
import hashlib
import json
import logging
import os
import random
import re
import string
import subprocess
import uuid
from collections import deque

import threading
import time

import gevent
import requests
from flask import Flask, Response, redirect, render_template, request, jsonify
from mutagen.id3 import ID3, SYLT, Encoding
from mutagen.mp3 import MP3

_log_level = os.environ.get("LOG_LEVEL", "DEBUG").upper()
logging.basicConfig(
    level=getattr(logging, _log_level, logging.DEBUG),
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger("musicdl")

LRCLIB_API = "https://lrclib.net/api"

app = Flask(__name__)

MUSIC_DIR = os.environ.get("MUSIC_DIR", "/music")
SOURCE_API = os.environ["SOURCE_API"]
SOURCE_MIRRORS = os.environ.get("SOURCE_MIRRORS", SOURCE_API).split(",")
NAVIDROME_URL = os.environ.get("NAVIDROME_URL", "http://navidrome:4533")
NAVIDROME_USER = os.environ.get("NAVIDROME_USER", "")
NAVIDROME_PASS = os.environ.get("NAVIDROME_PASS", "")

# Job queue
jobs = []  # all jobs, ordered by creation
job_events = deque(maxlen=500)  # rolling log of SSE events for clients to consume
_event_counter = 0


def _emit(event_type, data):
    """Push an event to the global event stream."""
    global _event_counter
    _event_counter += 1
    job_events.append((_event_counter, event_type, data))


_ui_log = os.environ.get("UI_LOG", "true").lower() not in ("0", "false", "no")


class SseLogHandler(logging.Handler):
    """Forwards log records to the SSE event stream so the UI sees all logs."""

    def emit(self, record):
        try:
            data = {"message": self.format(record)}
            job_id = getattr(record, "job_id", None)
            if job_id:
                data["job_id"] = job_id
            _emit("log", data)
        except Exception:
            pass  # never let a logging handler crash the app


if _ui_log:
    _sse_handler = SseLogHandler()
    _sse_handler.setFormatter(logging.Formatter("%(message)s"))
    log.addHandler(_sse_handler)


def source_get(path, **kwargs):
    """Try each source mirror in order, skipping 429s and transient errors."""
    last_exc = None
    for mirror in SOURCE_MIRRORS:
        try:
            resp = requests.get(f"{mirror}{path}", **kwargs)
            if resp.status_code == 429:
                last_exc = Exception(f"429 from {mirror}")
                continue
            resp.raise_for_status()
            return resp
        except Exception as e:
            last_exc = e
            continue
    raise last_exc or Exception("All source mirrors failed")


def scan_artists():
    try:
        return sorted(
            d
            for d in os.listdir(MUSIC_DIR)
            if os.path.isdir(os.path.join(MUSIC_DIR, d))
        )
    except FileNotFoundError:
        return []


def clean_filename(name):
    name = re.sub(r'[<>:"/\\|?*]', "", name)
    name = name.strip(". ")
    return name


def match_artist(name, existing):
    normalized = re.sub(r"\s+", " ", name.strip()).lower()
    for artist in existing:
        if re.sub(r"\s+", " ", artist.strip()).lower() == normalized:
            return artist
    return None


def trigger_rescan():
    if not NAVIDROME_USER or not NAVIDROME_PASS:
        return
    try:
        salt = "".join(random.choices(string.ascii_lowercase + string.digits, k=12))
        token = hashlib.md5((NAVIDROME_PASS + salt).encode()).hexdigest()
        resp = requests.get(
            f"{NAVIDROME_URL}/rest/startScan",
            params={
                "u": NAVIDROME_USER,
                "s": salt,
                "t": token,
                "v": "1.16.1",
                "c": "music-downloader",
                "f": "json",
            },
            timeout=10,
        )
        resp.raise_for_status()
        log.info("Navidrome rescan triggered")
    except Exception as e:
        log.warning("Navidrome rescan failed: %s", e)


def _parse_lrc(lrc_text):
    """Parse LRC text into list of (text, timestamp_ms) tuples for SYLT."""
    lines = []
    for line in lrc_text.splitlines():
        m = re.match(r"\[(\d+):(\d+(?:\.\d+)?)\](.*)", line)
        if m:
            mins, secs, text = m.groups()
            ms = int(float(mins) * 60000 + float(secs) * 1000)
            lines.append((text.strip(), ms))
    return lines


def fetch_lyrics(title, artist, album):
    """Fetch synced LRC lyrics from lrclib.net. Returns parsed SYLT list or None."""
    params_base = {"track_name": title, "artist_name": artist, "album_name": album}
    try:
        resp = requests.get(f"{LRCLIB_API}/search", params=params_base, timeout=10)
        log.debug("lrclib search %r → HTTP %s", title, resp.status_code)
        if resp.ok:
            for result in resp.json():
                lrc = result.get("syncedLyrics") or ""
                if lrc:
                    log.debug("lrclib found synced lyrics for %r", title)
                    return _parse_lrc(lrc)
        log.debug("lrclib: no synced lyrics for %r", title)
    except Exception as e:
        log.warning("lrclib fetch failed for %r: %s", title, e)
    return None


def embed_lyrics(mp3_path, sylt_lines):
    """Write SYLT frame into an existing MP3 file using mutagen."""
    audio = MP3(mp3_path, ID3=ID3)
    if audio.tags is None:
        audio.add_tags()
    audio.tags.add(
        SYLT(
            encoding=Encoding.UTF8,
            lang="eng",
            format=2,  # milliseconds
            type=1,  # lyrics
            desc="",
            text=sylt_lines,
        )
    )
    audio.save()


def _resolve_track_url(track_id):
    """Get CDN download URL for a track, trying each mirror and skipping 429s."""
    for mirror in SOURCE_MIRRORS:
        try:
            resp = requests.get(
                f"{mirror}/track/",
                params={"id": track_id, "quality": "LOSSLESS"},
                timeout=15,
            )
            if resp.status_code == 429:
                log.debug(
                    "429 from %s for track %s, trying next mirror", mirror, track_id
                )
                continue
            resp.raise_for_status()
            manifest_b64 = resp.json()["data"]["manifest"]
            manifest = json.loads(base64.b64decode(manifest_b64))
            url = manifest["urls"][0]
            log.debug("Resolved track %s via %s", track_id, mirror)
            return url
        except Exception as e:
            log.debug("Mirror %s failed for track %s: %s", mirror, track_id, e)
            continue
    log.error("All mirrors failed for track %s", track_id)
    return None


def _process_track(job, track, target_dir):
    """Download, transcode, and embed lyrics for a single track."""
    artist = job["artist"]
    album = job["album"]
    idx = track["index"]
    title = track["title"]
    url = track["url"]
    safe_title = clean_filename(title)
    flac_path = os.path.join(target_dir, f"{idx:02d} - {safe_title}.flac")
    mp3_path = os.path.join(target_dir, f"{idx:02d} - {safe_title}.mp3")
    total = job.get("total_tracks") or len(job["tracks"])

    job["current_track"] = idx
    log.info(
        "Track %d/%d: %s — downloading", idx, total, title, extra={"job_id": job["id"]}
    )
    _emit(
        "track_update",
        {"job_id": job["id"], "index": idx, "title": title, "status": "downloading"},
    )

    # Parse duration for transcode progress
    duration_s = 0
    dur_str = track.get("duration", "")
    if dur_str and ":" in dur_str:
        try:
            m_str, s_str = dur_str.split(":")
            duration_s = int(m_str) * 60 + int(s_str)
        except (ValueError, AttributeError):
            pass

    try:
        resp = requests.get(url, stream=True, timeout=(15, 60))
        resp.raise_for_status()
        content_length = int(resp.headers.get("Content-Length", 0))
        bytes_written = 0
        last_dl_pct = -1
        with gevent.Timeout(300, RuntimeError("Download timed out after 300s")):
            with open(flac_path, "wb") as out:
                for chunk in resp.iter_content(chunk_size=65536):
                    out.write(chunk)
                    bytes_written += len(chunk)
                    if content_length > 0:
                        pct = int(bytes_written / content_length * 100)
                        if pct >= last_dl_pct + 5:
                            last_dl_pct = pct
                            _emit(
                                "track_progress",
                                {
                                    "job_id": job["id"],
                                    "index": idx,
                                    "phase": "download",
                                    "pct": pct,
                                },
                            )

        log.info(
            "Track %d: downloaded %d bytes, transcoding",
            idx,
            bytes_written,
            extra={"job_id": job["id"]},
        )
        _emit(
            "track_update",
            {
                "job_id": job["id"],
                "index": idx,
                "title": title,
                "status": "transcoding",
            },
        )

        proc = subprocess.Popen(
            [
                "ffmpeg",
                "-y",
                "-i",
                flac_path,
                "-codec:a",
                "libmp3lame",
                "-b:a",
                "320k",
                "-metadata",
                f"title={title}",
                "-metadata",
                f"artist={artist}",
                "-metadata",
                f"album={album}",
                "-metadata",
                f"track={idx}/{total}",
                "-progress",
                "pipe:1",
                mp3_path,
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        for line in proc.stdout or []:  # stdout always set; typing appeases linter
            key, _, val = line.partition("=")
            key = key.strip()
            val = val.strip()
            if key == "out_time_us" and duration_s > 0:
                try:
                    us = int(val)
                    if us >= 0:
                        pct = min(100, int(us / (duration_s * 1_000_000) * 100))
                        _emit(
                            "track_progress",
                            {
                                "job_id": job["id"],
                                "index": idx,
                                "phase": "transcode",
                                "pct": pct,
                            },
                        )
                except ValueError:
                    pass
        proc.wait(timeout=300)
        if proc.returncode != 0:
            stderr_out = (proc.stderr.read() if proc.stderr else "")[-200:]
            raise RuntimeError(f"ffmpeg: {stderr_out}")

        os.remove(flac_path)
        log.info("Track %d: transcode done", idx, extra={"job_id": job["id"]})

        sylt = fetch_lyrics(title, artist, album)
        if sylt:
            try:
                embed_lyrics(mp3_path, sylt)
                log.info("Lyrics embedded: %s", title, extra={"job_id": job["id"]})
            except Exception as e:
                log.warning(
                    "Lyrics embed failed for %s: %s",
                    title,
                    e,
                    extra={"job_id": job["id"]},
                )
        else:
            log.debug("No lyrics found: %s", title, extra={"job_id": job["id"]})

        job["tracks_done"] += 1
        _emit(
            "track_update",
            {"job_id": job["id"], "index": idx, "title": title, "status": "done"},
        )

    except Exception as e:
        log.error(
            "Track %d failed: %s", idx, e, exc_info=True, extra={"job_id": job["id"]}
        )
        if os.path.exists(flac_path):
            os.remove(flac_path)
        job["tracks_done"] += 1
        _emit(
            "track_update",
            {
                "job_id": job["id"],
                "index": idx,
                "title": title,
                "status": "error",
                "error": str(e),
            },
        )


def process_job(job):
    job["status"] = "active"
    job["tracks_done"] = 0
    artist = job["artist"]
    album = job["album"]
    target_dir = os.path.join(MUSIC_DIR, clean_filename(artist), clean_filename(album))

    log.info("Starting: %s — %s", artist, album, extra={"job_id": job["id"]})
    _emit("job_update", {"job_id": job["id"], "status": "active"})

    try:
        os.makedirs(target_dir, exist_ok=True)
    except OSError as e:
        log.error(
            "Failed to create directory %s: %s",
            target_dir,
            e,
            extra={"job_id": job["id"]},
        )
        job["status"] = "error"
        _emit("job_update", {"job_id": job["id"], "status": "error"})
        return

    # Process tracks as they arrive; wait if resolution is still in progress
    processed = 0
    while True:
        tracks = job["tracks"]
        while processed < len(tracks):
            _process_track(job, tracks[processed], target_dir)
            processed += 1
        if job.get("resolved", True):
            break
        _sleep(0.5)

    job["status"] = "done"
    job["current_track"] = None
    _emit("job_update", {"job_id": job["id"], "status": "done"})
    log.info("Finished: %s — %s", artist, album, extra={"job_id": job["id"]})


def worker():
    """Background worker that processes queued jobs."""
    while True:
        for job in jobs:
            if job["status"] == "queued":
                process_job(job)
                trigger_rescan()
                break
        _sleep(1)


_worker_started = False


def _is_gevent_patched():
    """Check if gevent monkey-patching is active (i.e. running under gunicorn -k gevent)."""
    try:
        from gevent import monkey
        return monkey.is_module_patched("socket")
    except ImportError:
        return False


_use_gevent = _is_gevent_patched()


def _sleep(seconds):
    """Use gevent.sleep under gunicorn-gevent, time.sleep otherwise."""
    if _use_gevent:
        gevent.sleep(seconds)
    else:
        time.sleep(seconds)


def ensure_worker():
    global _worker_started
    if not _worker_started:
        _worker_started = True
        if _is_gevent_patched():
            gevent.spawn(worker)
        else:
            t = threading.Thread(target=worker, daemon=True)
            t.start()


@app.before_request
def _start_worker():
    ensure_worker()


LINKS = [
    ("/files/", "\U0001f4c1", "Files"),
    ("/", "\U0001f3b5", "Player"),
    ("/lidarr/", "\U0001f3b6", "Lidarr"),
    ("/sonarr/", "\U0001f4fa", "Sonarr"),
    ("/qbt/", "\u2b07", "qBittorrent"),
    ("/prowlarr/", "\U0001f50d", "Prowlarr"),
    ("https://jellyfin.apps.okd.benrachmiel.org", "\U0001f3ac", "Jellyfin"),
    ("/download/", "\U0001f4e5", "Downloader"),
]


@app.route("/")
def root():
    return redirect("/download/")


@app.route("/banner/")
def banner():
    return render_template("banner.html", links=LINKS)


@app.route("/download/", methods=["GET"])
def index():
    return render_template("index.html")


@app.route("/download/start", methods=["POST"])
def start_download():
    """Add a job to the queue."""
    data = request.get_json()
    job = {
        "id": str(uuid.uuid4())[:8],
        "artist": data["artist"],
        "album": data["album"],
        "tracks": list(data["tracks"]),
        "total_tracks": data.get("total_tracks"),
        "resolved": data.get("resolved", True),
        "status": "queued",
        "current_track": None,
    }
    jobs.append(job)
    _emit(
        "job_update",
        {
            "job_id": job["id"],
            "status": "queued",
            "artist": job["artist"],
            "album": job["album"],
            "track_count": job["total_tracks"] or len(job["tracks"]),
        },
    )
    return jsonify({"job_id": job["id"]})


@app.route("/download/jobs/<job_id>/tracks", methods=["POST"])
def append_tracks(job_id):
    """Append newly resolved tracks to a queued/active job."""
    data = request.get_json()
    job = next((j for j in jobs if j["id"] == job_id), None)
    if not job:
        return jsonify({"error": "Job not found"}), 404
    job["tracks"].extend(data.get("tracks", []))
    return jsonify({"ok": True})


@app.route("/download/jobs/<job_id>/resolve", methods=["POST"])
def mark_resolved(job_id):
    """Mark a job's track list as complete so the worker can finish."""
    job = next((j for j in jobs if j["id"] == job_id), None)
    if not job:
        return jsonify({"error": "Job not found"}), 404
    job["resolved"] = True
    final_count = len(job["tracks"])
    job["total_tracks"] = final_count
    _emit(
        "job_update",
        {
            "job_id": job["id"],
            "status": job["status"],
            "track_count": final_count,
        },
    )
    return jsonify({"ok": True})


@app.route("/download/jobs", methods=["GET"])
def get_jobs():
    """Return current state of all jobs."""
    return jsonify(
        [
            {
                "id": j["id"],
                "artist": j["artist"],
                "album": j["album"],
                "status": j["status"],
                "current_track": j["current_track"],
                "track_count": j.get("total_tracks") or len(j["tracks"]),
                "tracks_done": j.get("tracks_done", 0),
            }
            for j in jobs
        ]
    )


@app.route("/download/jobs/clear", methods=["POST"])
def clear_completed():
    """Remove completed jobs from the list."""
    global jobs
    jobs = [j for j in jobs if j["status"] not in ("done", "error")]
    return jsonify({"ok": True})


@app.route("/download/search")
def search_albums():
    """Search for albums via source API."""
    q = request.args.get("q", "").strip()
    if not q:
        return jsonify({"error": "No query"}), 400
    try:
        resp = source_get("/search/", params={"al": q}, timeout=15)
        data = resp.json()
        albums = []
        for item in data.get("data", {}).get("albums", {}).get("items", []):
            artist_name = item.get("artists", [{}])[0].get("name", "Unknown")
            cover = item.get("cover", "")
            cover_url = (
                f"https://resources.tidal.com/images/{cover.replace('-', '/')}/320x320.jpg"
                if cover
                else ""
            )
            albums.append(
                {
                    "id": item["id"],
                    "title": item["title"],
                    "artist": artist_name,
                    "cover": cover_url,
                    "tracks": item.get("numberOfTracks", 0),
                    "year": (item.get("releaseDate") or "")[:4],
                }
            )
        return jsonify({"albums": albums})
    except Exception as e:
        return jsonify({"error": str(e)}), 502


@app.route("/download/resolve/<int:album_id>")
def resolve_album(album_id):
    """Stream album track resolution as SSE: meta → track×N → done."""

    def generate():
        try:
            resp = source_get("/album/", params={"id": album_id}, timeout=15)
            data = resp.json()["data"]

            artist = data.get("artist", {}).get("name", "Unknown")
            album = data["title"]
            album_clean = re.sub(
                r"\s*\((?:Deluxe|Remaster|Expanded|Anniversary).*?\)\s*$",
                "",
                album,
                flags=re.IGNORECASE,
            )
            existing_artists = scan_artists()
            matched = match_artist(artist, existing_artists)
            items = data.get("items", [])

            meta = {
                "artist": artist,
                "album": album_clean,
                "matched_artist": matched,
                "existing_artists": existing_artists,
                "total": len(items),
            }
            yield f"event: meta\ndata: {json.dumps(meta)}\n\n"

            for item_wrapper in items:
                item = item_wrapper.get("item", {})
                track_id = item["id"]
                idx = item.get("trackNumber", 1)
                duration_s = item.get("duration", 0)
                mins, secs = divmod(duration_s, 60)
                url = _resolve_track_url(track_id)
                if url:
                    track = {
                        "index": idx,
                        "title": item["title"],
                        "artist": artist,
                        "album": album_clean,
                        "duration": f"{mins}:{secs:02d}",
                        "url": url,
                    }
                    yield f"event: track\ndata: {json.dumps(track)}\n\n"
                gevent.sleep(0)  # yield to other greenlets between track lookups

            yield "event: done\ndata: {}\n\n"

        except Exception as e:
            yield f"event: error\ndata: {json.dumps({'error': str(e)})}\n\n"

    return Response(
        generate(),
        mimetype="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )


@app.route("/download/status")
def status_stream():
    """SSE stream of all job events."""
    last_id = int(request.args.get("last_id", 0))

    def generate():
        nonlocal last_id
        while True:
            for seq, event_type, data in list(job_events):
                if seq > last_id:
                    last_id = seq
                    yield f"id: {seq}\nevent: {event_type}\ndata: {json.dumps(data)}\n\n"
            _sleep(0.5)

    return Response(
        generate(),
        mimetype="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8080, debug=True)
