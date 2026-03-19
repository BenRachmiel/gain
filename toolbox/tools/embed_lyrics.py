"""Find tracks without SYLT frames, fetch lyrics from lrclib, and embed them."""

import argparse
import logging
import sys

from musiclib.fs import parse_track_path
from musiclib.lyrics import embed_lyrics, fetch_lyrics, has_lyrics
from tools._scope import resolve_tracks

log = logging.getLogger("tools.embed_lyrics")


def main(path: str) -> None:
    tracks = resolve_tracks(path)
    if not tracks:
        print("No MP3 files found.", file=sys.stderr)
        sys.exit(1)

    embedded = 0
    not_found = 0
    already_had = 0

    for mp3 in tracks:
        if has_lyrics(mp3):
            already_had += 1
            continue

        try:
            artist, album, _idx, title = parse_track_path(mp3)
        except ValueError as e:
            log.warning("Skipping %s: %s", mp3, e)
            continue

        sylt = fetch_lyrics(title, artist, album)
        if sylt:
            embed_lyrics(mp3, sylt)
            embedded += 1
            log.info("Embedded lyrics: %s - %s", artist, title)
        else:
            not_found += 1
            log.info("No lyrics found: %s - %s", artist, title)

    print(f"embedded: {embedded}, not found: {not_found}, already had: {already_had}")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    parser = argparse.ArgumentParser(description="Embed synced lyrics into MP3 files")
    parser.add_argument("path", help="Track, album dir, artist dir, or library root")
    args = parser.parse_args()
    main(args.path)
