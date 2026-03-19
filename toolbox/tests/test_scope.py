import os

from tools._scope import resolve_albums, resolve_tracks


def test_resolve_tracks_single_file(sample_mp3):
    tracks = resolve_tracks(sample_mp3)
    assert len(tracks) == 1
    assert tracks[0] == sample_mp3


def test_resolve_tracks_album_dir(sample_library):
    album_dir = os.path.join(sample_library, "Artist1", "Album1")
    tracks = resolve_tracks(album_dir)
    assert len(tracks) == 3


def test_resolve_tracks_artist_dir(sample_library):
    artist_dir = os.path.join(sample_library, "Artist1")
    tracks = resolve_tracks(artist_dir)
    assert len(tracks) == 6  # 2 albums × 3 tracks


def test_resolve_tracks_library_root(sample_library):
    tracks = resolve_tracks(sample_library)
    assert len(tracks) == 12  # 2 artists × 2 albums × 3 tracks


def test_resolve_albums_single_file(sample_mp3):
    albums = resolve_albums(sample_mp3)
    assert len(albums) == 1
    assert albums[0].artist == "Artist"
    assert albums[0].title == "Album"


def test_resolve_albums_library_root(sample_library):
    albums = resolve_albums(sample_library)
    assert len(albums) == 4
