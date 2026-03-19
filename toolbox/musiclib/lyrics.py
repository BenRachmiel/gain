import logging
import re

import requests
from mutagen.id3 import ID3, SYLT, Encoding
from mutagen.mp3 import MP3

log = logging.getLogger("musiclib.lyrics")

LRCLIB_API = "https://lrclib.net/api"


def has_lyrics(mp3_path: str) -> bool:
    """Check whether an MP3 already has a SYLT (synced lyrics) frame."""
    try:
        audio = MP3(mp3_path, ID3=ID3)
        if audio.tags is None:
            return False
        return any(frame.startswith("SYLT") for frame in audio.tags)
    except Exception:
        return False


def parse_lrc(lrc_text: str) -> list[tuple[str, int]]:
    """Parse LRC text into ``[(text, timestamp_ms), ...]`` for SYLT."""
    lines: list[tuple[str, int]] = []
    for line in lrc_text.splitlines():
        m = re.match(r"\[(\d+):(\d+(?:\.\d+)?)\](.*)", line)
        if m:
            mins, secs, text = m.groups()
            ms = int(float(mins) * 60000 + float(secs) * 1000)
            lines.append((text.strip(), ms))
    return lines


def fetch_lyrics(
    title: str, artist: str, album: str
) -> list[tuple[str, int]] | None:
    """Fetch synced LRC lyrics from lrclib.net. Returns parsed SYLT list or None."""
    params = {"track_name": title, "artist_name": artist, "album_name": album}
    try:
        resp = requests.get(f"{LRCLIB_API}/search", params=params, timeout=10)
        log.debug("lrclib search %r → HTTP %s", title, resp.status_code)
        if resp.ok:
            for result in resp.json():
                lrc = result.get("syncedLyrics") or ""
                if lrc:
                    log.debug("lrclib found synced lyrics for %r", title)
                    return parse_lrc(lrc)
        log.debug("lrclib: no synced lyrics for %r", title)
    except Exception as e:
        log.warning("lrclib fetch failed for %r: %s", title, e)
    return None


def embed_lyrics(mp3_path: str, sylt_lines: list[tuple[str, int]]) -> None:
    """Write a SYLT frame into an existing MP3 file."""
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
