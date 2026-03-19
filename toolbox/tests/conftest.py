import subprocess

import pytest
from mutagen.id3 import TALB, TIT2, TPE1, TRCK
from mutagen.mp3 import MP3


def _make_silent_mp3(path: str) -> None:
    """Create a minimal valid MP3 file using ffmpeg."""
    subprocess.run(
        [
            "ffmpeg", "-y", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono",
            "-t", "0.1", "-b:a", "128k", "-f", "mp3", path,
        ],
        capture_output=True,
        check=True,
    )


def _tag_mp3(path: str, **tags) -> None:
    """Write ID3 tags onto an MP3, creating tag header if needed."""
    audio = MP3(path)
    if audio.tags is None:
        audio.add_tags()
    for key, value in tags.items():
        frame_cls = {"title": TIT2, "artist": TPE1, "album": TALB, "track": TRCK}[key]
        audio.tags.add(frame_cls(encoding=3, text=[value]))
    audio.save()


@pytest.fixture()
def sample_mp3(tmp_path):
    """Create a single sample MP3 at ``tmp_path/Artist/Album/01 - Track.mp3``."""
    album_dir = tmp_path / "Artist" / "Album"
    album_dir.mkdir(parents=True)
    mp3_path = album_dir / "01 - Track.mp3"
    _make_silent_mp3(str(mp3_path))
    _tag_mp3(str(mp3_path), title="Track", artist="Artist", album="Album", track="1/3")
    return str(mp3_path)


@pytest.fixture()
def sample_library(tmp_path):
    """Create a small library with 2 artists, 2 albums each, 3 tracks each."""
    for artist_idx in range(1, 3):
        artist = f"Artist{artist_idx}"
        for album_idx in range(1, 3):
            album = f"Album{album_idx}"
            album_dir = tmp_path / artist / album
            album_dir.mkdir(parents=True)
            for track_idx in range(1, 4):
                mp3_path = album_dir / f"{track_idx:02d} - Track{track_idx}.mp3"
                _make_silent_mp3(str(mp3_path))
                _tag_mp3(
                    str(mp3_path),
                    title=f"Track{track_idx}",
                    artist=artist,
                    album=album,
                    track=f"{track_idx}/3",
                )

    return str(tmp_path)
