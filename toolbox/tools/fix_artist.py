"""Fix 'Unknown Artist' albums by matching album names against known artists in the library.

If an album under 'Unknown Artist' has the same name as an album under a real artist,
moves the tracks into that artist's album folder and updates the artist ID3 tag.
"""

import argparse
import logging
import os
import shutil
import sys

from musiclib.fs import scan_library, scan_tracks
from musiclib.tags import write_tags

log = logging.getLogger("tools.fix_artist")

UNKNOWN = "Unknown Artist"


def main(path: str, dry_run: bool = False) -> None:
    albums = scan_library(path)
    if not albums:
        print("No albums found.", file=sys.stderr)
        sys.exit(1)

    # Index known albums: album_title → artist name (skip Unknown Artist)
    known: dict[str, str] = {}
    for album in albums:
        if album.artist != UNKNOWN:
            lower = album.title.lower()
            if lower not in known:
                known[lower] = album.artist

    # Find Unknown Artist albums that match
    unknown_albums = [a for a in albums if a.artist == UNKNOWN]
    if not unknown_albums:
        print("No 'Unknown Artist' albums found.")
        return

    fixed = 0
    skipped = 0

    for album in unknown_albums:
        match = known.get(album.title.lower())
        if not match:
            skipped += 1
            log.info("No match for album: %s", album.title)
            continue

        target_artist_dir = os.path.join(path, match)
        target_album_dir = os.path.join(target_artist_dir, album.title)

        tracks = scan_tracks(album.path)
        if not tracks:
            continue

        if dry_run:
            print(f"Would move: {UNKNOWN}/{album.title} → {match}/{album.title} ({len(tracks)} tracks)")
            fixed += 1
            continue

        os.makedirs(target_album_dir, exist_ok=True)

        for track in tracks:
            dest = os.path.join(target_album_dir, os.path.basename(track.path))
            if os.path.exists(dest):
                log.warning("Target exists, skipping: %s", dest)
                continue
            shutil.move(track.path, dest)
            write_tags(dest, artist=match, force=True)
            log.info("Moved and tagged: %s → %s", track.path, dest)

        # Remove empty album dir under Unknown Artist
        try:
            os.rmdir(album.path)
        except OSError:
            pass

        fixed += 1
        log.info("Fixed: %s/%s → %s/%s", UNKNOWN, album.title, match, album.title)

    # Remove Unknown Artist dir if empty
    unknown_dir = os.path.join(path, UNKNOWN)
    try:
        os.rmdir(unknown_dir)
    except OSError:
        pass

    action = "Would fix" if dry_run else "Fixed"
    print(f"{action}: {fixed}, no match: {skipped}")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    parser = argparse.ArgumentParser(description="Fix Unknown Artist albums by matching against known artists")
    parser.add_argument("path", help="Library root")
    parser.add_argument("--dry-run", action="store_true", help="Show what would be done without making changes")
    args = parser.parse_args()
    main(args.path, args.dry_run)
