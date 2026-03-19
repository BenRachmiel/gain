import os
import re
from typing import NamedTuple


class Album(NamedTuple):
    artist: str
    title: str
    path: str


class Track(NamedTuple):
    index: int
    title: str
    path: str


def clean_filename(name: str) -> str:
    """Sanitize a string for use as a filename."""
    name = re.sub(r'[<>:"/\\|?*]', "", name)
    return name.strip(". ")


_TRACK_RE = re.compile(r"^(\d+)\s*-\s*(.+)\.mp3$", re.IGNORECASE)


def scan_library(root: str) -> list[Album]:
    """Walk ``{root}/{artist}/{album}/`` and return Album namedtuples."""
    albums: list[Album] = []
    try:
        artists = sorted(
            d for d in os.listdir(root) if os.path.isdir(os.path.join(root, d))
        )
    except FileNotFoundError:
        return albums

    for artist in artists:
        artist_dir = os.path.join(root, artist)
        try:
            album_dirs = sorted(
                d
                for d in os.listdir(artist_dir)
                if os.path.isdir(os.path.join(artist_dir, d))
            )
        except OSError:
            continue
        for album in album_dirs:
            albums.append(Album(artist=artist, title=album, path=os.path.join(artist_dir, album)))
    return albums


def scan_albums(artist_dir: str) -> list[Album]:
    """List albums under a single artist directory."""
    artist = os.path.basename(artist_dir)
    albums: list[Album] = []
    try:
        for d in sorted(os.listdir(artist_dir)):
            full = os.path.join(artist_dir, d)
            if os.path.isdir(full):
                albums.append(Album(artist=artist, title=d, path=full))
    except OSError:
        pass
    return albums


def scan_tracks(album_dir: str) -> list[Track]:
    """List MP3s in an album dir, parsing ``NN - Title.mp3`` into Track tuples."""
    tracks: list[Track] = []
    try:
        for f in sorted(os.listdir(album_dir)):
            m = _TRACK_RE.match(f)
            if m:
                tracks.append(
                    Track(
                        index=int(m.group(1)),
                        title=m.group(2).strip(),
                        path=os.path.join(album_dir, f),
                    )
                )
    except OSError:
        pass
    return tracks


def parse_track_path(path: str) -> tuple[str, str, int, str]:
    """Derive ``(artist, album, index, title)`` from a track's filesystem path.

    Expects ``…/{artist}/{album}/NN - Title.mp3``.
    """
    filename = os.path.basename(path)
    m = _TRACK_RE.match(filename)
    if not m:
        raise ValueError(f"Cannot parse track filename: {filename}")
    index = int(m.group(1))
    title = m.group(2).strip()
    album_dir = os.path.dirname(path)
    album = os.path.basename(album_dir)
    artist = os.path.basename(os.path.dirname(album_dir))
    return artist, album, index, title
