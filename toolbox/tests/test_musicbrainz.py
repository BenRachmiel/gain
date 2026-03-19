from unittest.mock import patch

from musiclib.musicbrainz import CONFIDENCE_THRESHOLD, search_release


def test_search_release_high_confidence():
    mock_result = {
        "release-list": [
            {
                "id": "abc-123",
                "title": "Test Album",
                "ext:score": "100",
                "artist-credit": [{"name": "Test Artist"}],
            }
        ]
    }
    with patch("musiclib.musicbrainz.musicbrainzngs.search_releases", return_value=mock_result):
        release = search_release("Test Artist", "Test Album")
        assert release is not None
        assert release.score >= CONFIDENCE_THRESHOLD
        assert release.mbid == "abc-123"


def test_search_release_low_confidence():
    mock_result = {
        "release-list": [
            {
                "id": "xyz-789",
                "title": "Wrong Album",
                "ext:score": "50",
                "artist-credit": [{"name": "Wrong Artist"}],
            }
        ]
    }
    with patch("musiclib.musicbrainz.musicbrainzngs.search_releases", return_value=mock_result):
        release = search_release("Test Artist", "Test Album")
        assert release is not None
        assert release.score < CONFIDENCE_THRESHOLD


def test_search_release_no_results():
    mock_result = {"release-list": []}
    with patch("musiclib.musicbrainz.musicbrainzngs.search_releases", return_value=mock_result):
        release = search_release("Nobody", "Nothing")
        assert release is None
