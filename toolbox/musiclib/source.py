import logging
import os

import requests

log = logging.getLogger("musiclib.source")

SOURCE_API = os.environ.get("SOURCE_API", "")
SOURCE_MIRRORS = os.environ.get("SOURCE_MIRRORS", SOURCE_API).split(",") if SOURCE_API else []


def source_get(path: str, **kwargs) -> requests.Response:
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


def search_album(query: str) -> list[dict]:
    """Search the source API for albums matching a query."""
    resp = source_get("/search/", params={"al": query}, timeout=15)
    data = resp.json()
    albums = []
    for item in data.get("data", {}).get("albums", {}).get("items", []):
        artist_name = item.get("artists", [{}])[0].get("name", "Unknown")
        cover = item.get("cover", "")
        albums.append(
            {
                "id": item["id"],
                "title": item["title"],
                "artist": artist_name,
                "cover": cover,
                "tracks": item.get("numberOfTracks", 0),
                "year": (item.get("releaseDate") or "")[:4],
            }
        )
    return albums


def get_album_metadata(album_id: int) -> dict:
    """Fetch full album details from source API."""
    resp = source_get("/album/", params={"id": album_id}, timeout=15)
    return resp.json().get("data", {})


def get_cover_url(cover_id: str, size: int = 1280) -> str:
    """Construct a Tidal CDN cover URL from a cover ID."""
    return f"https://resources.tidal.com/images/{cover_id.replace('-', '/')}/{size}x{size}.jpg"


def fetch_cover_image(cover_id: str, size: int = 1280) -> tuple[bytes, str] | None:
    """Download cover art from Tidal CDN. Returns ``(image_bytes, mime_type)`` or None."""
    url = get_cover_url(cover_id, size)
    try:
        resp = requests.get(url, timeout=15)
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        return resp.content, resp.headers.get("Content-Type", "image/jpeg")
    except Exception as e:
        log.warning("Tidal CDN cover fetch failed for %s: %s", cover_id, e)
        return None
