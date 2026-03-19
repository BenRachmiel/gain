import logging
from typing import NamedTuple

import musicbrainzngs
import requests

log = logging.getLogger("musiclib.musicbrainz")

musicbrainzngs.set_useragent("music-toolbox", "1.0", "https://github.com/example/music-toolbox")

COVER_ART_ARCHIVE = "https://coverartarchive.org"
CONFIDENCE_THRESHOLD = 90


class Release(NamedTuple):
    mbid: str
    title: str
    artist: str
    score: int


def search_release(artist: str, album: str) -> Release | None:
    """Search MusicBrainz for a release matching artist + album.

    Returns the best match with its confidence score, or None.
    """
    try:
        result = musicbrainzngs.search_releases(
            artist=artist, release=album, limit=5
        )
        releases = result.get("release-list", [])
        if not releases:
            return None

        best = releases[0]
        score = int(best.get("ext:score", 0))
        artist_credit = best.get("artist-credit", [{}])
        artist_name = artist_credit[0].get("name", artist) if artist_credit else artist

        return Release(
            mbid=best["id"],
            title=best["title"],
            artist=artist_name,
            score=score,
        )
    except Exception as e:
        log.warning("MusicBrainz search failed for %s - %s: %s", artist, album, e)
        return None


def fetch_release_metadata(mbid: str) -> dict:
    """Fetch detailed metadata for a release by MBID.

    Returns dict with keys: year, genres, tracks.
    """
    try:
        result = musicbrainzngs.get_release_by_id(
            mbid, includes=["recordings", "tags", "release-groups"]
        )
        release = result["release"]

        year = (release.get("date") or "")[:4]
        genres: list[str] = []

        # Tags from release
        for tag in release.get("tag-list", []):
            genres.append(tag["name"])

        # Tags from release group
        rg = release.get("release-group", {})
        for tag in rg.get("tag-list", []):
            if tag["name"] not in genres:
                genres.append(tag["name"])

        tracks: list[dict] = []
        for medium in release.get("medium-list", []):
            for track in medium.get("track-list", []):
                recording = track.get("recording", {})
                tracks.append(
                    {
                        "index": int(track.get("number", 0)),
                        "title": recording.get("title", track.get("title", "")),
                        "length_ms": int(recording.get("length", 0)),
                    }
                )

        return {"year": year, "genres": genres, "tracks": tracks}
    except Exception as e:
        log.warning("MusicBrainz metadata fetch failed for %s: %s", mbid, e)
        return {}


def fetch_cover_art(mbid: str) -> tuple[bytes, str] | None:
    """Fetch front cover art from the Cover Art Archive.

    Returns ``(image_bytes, mime_type)`` or None.
    """
    try:
        resp = requests.get(
            f"{COVER_ART_ARCHIVE}/release/{mbid}/front",
            timeout=15,
            allow_redirects=True,
        )
        if resp.status_code == 404:
            log.debug("No cover art on CAA for %s", mbid)
            return None
        resp.raise_for_status()
        mime = resp.headers.get("Content-Type", "image/jpeg")
        return resp.content, mime
    except Exception as e:
        log.warning("Cover Art Archive fetch failed for %s: %s", mbid, e)
        return None
