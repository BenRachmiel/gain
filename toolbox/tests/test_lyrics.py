from unittest.mock import patch

from musiclib.lyrics import embed_lyrics, has_lyrics, parse_lrc


def test_parse_lrc_basic():
    lrc = "[00:05.00]Hello\n[00:10.50]World"
    result = parse_lrc(lrc)
    assert len(result) == 2
    assert result[0] == ("Hello", 5000)
    assert result[1] == ("World", 10500)


def test_parse_lrc_ignores_non_timestamp_lines():
    lrc = "[ti:Song Title]\n[00:01.00]Line one\nplain text\n[00:05.00]Line two"
    result = parse_lrc(lrc)
    assert len(result) == 2


def test_has_lyrics_no_sylt(sample_mp3):
    assert has_lyrics(sample_mp3) is False


def test_embed_and_has_lyrics(sample_mp3):
    sylt = [("Hello", 1000), ("World", 2000)]
    embed_lyrics(sample_mp3, sylt)
    assert has_lyrics(sample_mp3) is True


def test_fetch_lyrics_success():
    mock_response = type("R", (), {
        "ok": True,
        "status_code": 200,
        "json": lambda self: [{"syncedLyrics": "[00:01.00]Test line"}],
    })()

    with patch("musiclib.lyrics.requests.get", return_value=mock_response):
        from musiclib.lyrics import fetch_lyrics

        result = fetch_lyrics("Song", "Artist", "Album")
        assert result is not None
        assert len(result) == 1
        assert result[0] == ("Test line", 1000)


def test_fetch_lyrics_not_found():
    mock_response = type("R", (), {
        "ok": True,
        "status_code": 200,
        "json": lambda self: [],
    })()

    with patch("musiclib.lyrics.requests.get", return_value=mock_response):
        from musiclib.lyrics import fetch_lyrics

        result = fetch_lyrics("Song", "Artist", "Album")
        assert result is None
