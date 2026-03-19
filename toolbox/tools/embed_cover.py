"""Find albums without APIC frames, fetch cover art, and embed into all tracks."""

import argparse
import logging
import sys

from musiclib.fs import scan_tracks
from musiclib.musicbrainz import (
    CONFIDENCE_THRESHOLD,
    fetch_cover_art,
    search_release,
)
from musiclib.source import fetch_cover_image, search_album
from musiclib.tags import embed_cover_art, has_cover_art
from tools._scope import resolve_albums

log = logging.getLogger("tools.embed_cover")


def _fetch_cover_for_album(artist: str, album: str) -> tuple[bytes, str] | None:
    """Try MusicBrainz/CAA first, fall back to source API/Tidal CDN."""
    release = search_release(artist, album)
    if release and release.score >= CONFIDENCE_THRESHOLD:
        cover = fetch_cover_art(release.mbid)
        if cover:
            log.info("Cover from CAA for %s - %s", artist, album)
            return cover

    # Fallback: source API → Tidal CDN
    results = search_album(f"{artist} {album}")
    for result in results:
        cover_id = result.get("cover", "")
        if cover_id:
            cover = fetch_cover_image(cover_id)
            if cover:
                log.info("Cover from Tidal CDN for %s - %s", artist, album)
                return cover

    return None


def main(path: str) -> None:
    albums = resolve_albums(path)
    if not albums:
        print("No albums found.", file=sys.stderr)
        sys.exit(1)

    embedded = 0
    not_found = 0
    already_had = 0

    for album in albums:
        tracks = scan_tracks(album.path)
        if not tracks:
            continue

        # Check if any track already has cover art
        if all(has_cover_art(t.path) for t in tracks):
            already_had += 1
            continue

        cover = _fetch_cover_for_album(album.artist, album.title)
        if not cover:
            not_found += 1
            log.info("No cover found: %s - %s", album.artist, album.title)
            continue

        image_bytes, mime_type = cover
        for track in tracks:
            if not has_cover_art(track.path):
                embed_cover_art(track.path, image_bytes, mime_type)

        embedded += 1
        log.info("Embedded cover: %s - %s (%d tracks)", album.artist, album.title, len(tracks))

    print(f"albums covered: {embedded}, not found: {not_found}, already had: {already_had}")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    parser = argparse.ArgumentParser(description="Embed cover art into MP3 files")
    parser.add_argument("path", help="Track, album dir, artist dir, or library root")
    args = parser.parse_args()
    main(args.path)
