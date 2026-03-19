import os

from musiclib.fs import (
    clean_filename,
    parse_track_path,
    scan_albums,
    scan_library,
    scan_tracks,
)


def test_clean_filename_strips_illegal_chars():
    assert clean_filename('Track: "Hello" <World>') == "Track Hello World"


def test_clean_filename_strips_trailing_dots():
    assert clean_filename("file...") == "file"


def test_clean_filename_strips_trailing_spaces():
    assert clean_filename("file   ") == "file"


def test_scan_library(sample_library):
    albums = scan_library(sample_library)
    assert len(albums) == 4
    artists = {a.artist for a in albums}
    assert artists == {"Artist1", "Artist2"}


def test_scan_library_nonexistent(tmp_path):
    albums = scan_library(str(tmp_path / "nonexistent"))
    assert albums == []


def test_scan_albums(sample_library):
    artist_dir = os.path.join(sample_library, "Artist1")
    albums = scan_albums(artist_dir)
    assert len(albums) == 2
    assert all(a.artist == "Artist1" for a in albums)


def test_scan_tracks(sample_library):
    album_dir = os.path.join(sample_library, "Artist1", "Album1")
    tracks = scan_tracks(album_dir)
    assert len(tracks) == 3
    assert tracks[0].index == 1
    assert tracks[0].title == "Track1"


def test_parse_track_path(sample_library):
    path = os.path.join(sample_library, "Artist1", "Album1", "01 - Track1.mp3")
    artist, album, index, title = parse_track_path(path)
    assert artist == "Artist1"
    assert album == "Album1"
    assert index == 1
    assert title == "Track1"


def test_parse_track_path_invalid():
    import pytest

    with pytest.raises(ValueError):
        parse_track_path("/some/path/badname.mp3")
