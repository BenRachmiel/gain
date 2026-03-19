"""Fill missing metadata (year, genre, etc.) using MusicBrainz and source API as fallback."""

import argparse
import logging
import sys

from musiclib.fs import scan_tracks
from musiclib.musicbrainz import (
    CONFIDENCE_THRESHOLD,
    fetch_release_metadata,
    search_release,
)
from musiclib.source import search_album
from musiclib.tags import read_tags, write_tags
from tools._scope import resolve_albums

log = logging.getLogger("tools.complete_metadata")

DESIRED_FIELDS = {"year", "genre", "album_artist"}


def _find_missing_fields(tracks_tags: list[dict]) -> set[str]:
    """Identify which DESIRED_FIELDS are missing across all tracks in an album."""
    missing: set[str] = set()
    for field in DESIRED_FIELDS:
        if not any(t.get(field) for t in tracks_tags):
            missing.add(field)
    return missing


def main(path: str) -> None:
    albums = resolve_albums(path)
    if not albums:
        print("No albums found.", file=sys.stderr)
        sys.exit(1)

    updated = 0
    skipped = 0
    no_match = 0

    for album in albums:
        tracks = scan_tracks(album.path)
        if not tracks:
            continue

        tracks_tags = [read_tags(t.path) for t in tracks]
        missing = _find_missing_fields(tracks_tags)
        if not missing:
            skipped += 1
            continue

        # Try MusicBrainz
        metadata: dict = {}
        release = search_release(album.artist, album.title)
        if release and release.score >= CONFIDENCE_THRESHOLD:
            metadata = fetch_release_metadata(release.mbid)
            log.info(
                "MusicBrainz match (score=%d): %s - %s",
                release.score,
                album.artist,
                album.title,
            )
        else:
            # Fallback: source API
            score_info = f" (score={release.score})" if release else ""
            log.info(
                "MusicBrainz low confidence%s for %s - %s, trying source API",
                score_info,
                album.artist,
                album.title,
            )
            results = search_album(f"{album.artist} {album.title}")
            if results:
                metadata = {
                    "year": results[0].get("year", ""),
                }

        if not metadata:
            no_match += 1
            log.info("No metadata source for %s - %s", album.artist, album.title)
            continue

        # Apply missing fields to all tracks
        kwargs: dict[str, str] = {}
        if "year" in missing and metadata.get("year"):
            kwargs["year"] = metadata["year"]
        if "genre" in missing and metadata.get("genres"):
            kwargs["genre"] = metadata["genres"][0]
        if "album_artist" in missing and (release and release.artist):
            kwargs["album_artist"] = release.artist

        if not kwargs:
            skipped += 1
            continue

        for track in tracks:
            write_tags(track.path, **kwargs)

        updated += 1
        log.info("Updated %s - %s: %s", album.artist, album.title, list(kwargs.keys()))

    print(f"updated: {updated}, skipped (complete): {skipped}, no match: {no_match}")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    parser = argparse.ArgumentParser(description="Complete missing metadata in MP3 files")
    parser.add_argument("path", help="Track, album dir, artist dir, or library root")
    args = parser.parse_args()
    main(args.path)
