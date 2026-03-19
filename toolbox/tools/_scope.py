"""Shared scope-detection logic for CLI tools.

Given a path argument, determine whether it points to a single track, an album
directory, an artist directory, or the library root, and yield MP3 paths or
Album tuples accordingly.
"""

import os
import sys

from musiclib.fs import Album, scan_albums, scan_library, scan_tracks


def resolve_tracks(path: str) -> list[str]:
    """Return a flat list of MP3 file paths covered by *path*."""
    path = os.path.abspath(path)

    # Single file
    if os.path.isfile(path):
        if path.lower().endswith(".mp3"):
            return [path]
        print(f"Not an MP3 file: {path}", file=sys.stderr)
        return []

    if not os.path.isdir(path):
        print(f"Path does not exist: {path}", file=sys.stderr)
        return []

    # Try as album dir (contains MP3s directly)
    tracks = scan_tracks(path)
    if tracks:
        return [t.path for t in tracks]

    # Try as artist dir (contains album subdirs with MP3s)
    albums = scan_albums(path)
    if albums:
        result: list[str] = []
        for album in albums:
            result.extend(t.path for t in scan_tracks(album.path))
        if result:
            return result

    # Library root (artist/album/track structure)
    all_albums = scan_library(path)
    if all_albums:
        result = []
        for album in all_albums:
            result.extend(t.path for t in scan_tracks(album.path))
        return result

    return []


def resolve_albums(path: str) -> list[Album]:
    """Return a list of Album tuples covered by *path*.

    For a single track or album dir, returns the containing album.
    """
    path = os.path.abspath(path)

    if os.path.isfile(path):
        album_dir = os.path.dirname(path)
        artist = os.path.basename(os.path.dirname(album_dir))
        title = os.path.basename(album_dir)
        return [Album(artist=artist, title=title, path=album_dir)]

    if not os.path.isdir(path):
        print(f"Path does not exist: {path}", file=sys.stderr)
        return []

    # Album dir (has MP3s)
    tracks = scan_tracks(path)
    if tracks:
        artist = os.path.basename(os.path.dirname(path))
        title = os.path.basename(path)
        return [Album(artist=artist, title=title, path=path)]

    # Artist dir — check that subdirs actually contain MP3s
    albums = scan_albums(path)
    if albums and any(scan_tracks(a.path) for a in albums):
        return [a for a in albums if scan_tracks(a.path)]

    # Library root
    return scan_library(path)
