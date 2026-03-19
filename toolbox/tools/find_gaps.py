"""Diagnostic tool: report missing cover art, lyrics, and metadata across the library."""

import argparse
import json
import logging
import sys

from musiclib.fs import scan_tracks
from musiclib.lyrics import has_lyrics
from musiclib.tags import has_cover_art, read_tags
from tools._scope import resolve_albums

log = logging.getLogger("tools.find_gaps")

EXPECTED_FIELDS = {"title", "artist", "album", "track", "year", "genre"}


def main(path: str, output_json: bool = False) -> None:
    albums = resolve_albums(path)
    if not albums:
        print("No albums found.", file=sys.stderr)
        sys.exit(1)

    report: list[dict] = []

    for album in albums:
        tracks = scan_tracks(album.path)
        if not tracks:
            continue

        missing_lyrics = 0
        missing_cover = 0
        missing_fields: dict[str, int] = {}

        for track in tracks:
            if not has_lyrics(track.path):
                missing_lyrics += 1
            if not has_cover_art(track.path):
                missing_cover += 1

            tags = read_tags(track.path)
            for field in EXPECTED_FIELDS:
                if not tags.get(field):
                    missing_fields[field] = missing_fields.get(field, 0) + 1

        total = len(tracks)
        entry = {
            "artist": album.artist,
            "album": album.title,
            "tracks": total,
            "missing_lyrics": missing_lyrics,
            "missing_cover": missing_cover,
            "missing_fields": missing_fields,
        }

        has_gaps = missing_lyrics > 0 or missing_cover > 0 or missing_fields
        if has_gaps:
            report.append(entry)

    if output_json:
        print(json.dumps(report, indent=2))
    else:
        if not report:
            print("No gaps found.")
            return

        for entry in report:
            print(f"\n{entry['artist']} - {entry['album']} ({entry['tracks']} tracks)")
            if entry["missing_cover"] > 0:
                print(f"  Cover art: missing on {entry['missing_cover']}/{entry['tracks']} tracks")
            if entry["missing_lyrics"] > 0:
                print(f"  Lyrics:    missing on {entry['missing_lyrics']}/{entry['tracks']} tracks")
            if entry["missing_fields"]:
                fields = ", ".join(
                    f"{k} ({v}/{entry['tracks']})" for k, v in entry["missing_fields"].items()
                )
                print(f"  Metadata:  {fields}")

    print(f"\n{len(report)} album(s) with gaps out of {len(albums)} scanned.")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    parser = argparse.ArgumentParser(description="Report gaps in music library metadata")
    parser.add_argument("path", help="Track, album dir, artist dir, or library root")
    parser.add_argument("--json", dest="output_json", action="store_true", help="Output as JSON")
    args = parser.parse_args()
    main(args.path, args.output_json)
