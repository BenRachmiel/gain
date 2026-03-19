from musiclib.tags import embed_cover_art, has_cover_art, read_tags, write_tags


def test_read_tags(sample_mp3):
    tags = read_tags(sample_mp3)
    assert tags["title"] == "Track"
    assert tags["artist"] == "Artist"
    assert tags["album"] == "Album"
    assert tags["track"] == "1/3"


def test_write_tags_no_clobber(sample_mp3):
    write_tags(sample_mp3, title="New Title")
    tags = read_tags(sample_mp3)
    # Should NOT overwrite existing title
    assert tags["title"] == "Track"


def test_write_tags_fills_missing(sample_mp3):
    write_tags(sample_mp3, year="2020", genre="Rock")
    tags = read_tags(sample_mp3)
    assert tags["year"] == "2020"
    assert tags["genre"] == "Rock"


def test_write_tags_force(sample_mp3):
    write_tags(sample_mp3, title="Overwritten", force=True)
    tags = read_tags(sample_mp3)
    assert tags["title"] == "Overwritten"


def test_has_cover_art_initially_false(sample_mp3):
    assert has_cover_art(sample_mp3) is False


def test_embed_and_has_cover_art(sample_mp3):
    # 1x1 white JPEG
    jpeg = (
        b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00"
        b"\xff\xdb\x00C\x00\x08\x06\x06\x07\x06\x05\x08\x07\x07\x07\t\t"
        b"\x08\n\x0c\x14\r\x0c\x0b\x0b\x0c\x19\x12\x13\x0f\x14\x1d\x1a"
        b"\x1f\x1e\x1d\x1a\x1c\x1c $.\' ',#\x1c\x1c(7),01444\x1f\'9=82<.342"
        b"\xff\xc0\x00\x0b\x08\x00\x01\x00\x01\x01\x01\x11\x00"
        b"\xff\xc4\x00\x1f\x00\x00\x01\x05\x01\x01\x01\x01\x01\x01\x00"
        b"\x00\x00\x00\x00\x00\x00\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b"
        b"\xff\xda\x00\x08\x01\x01\x00\x00?\x00T\xdb\x9e\xa7\x13\xa2\x80"
        b"\xff\xd9"
    )
    embed_cover_art(sample_mp3, jpeg, "image/jpeg")
    assert has_cover_art(sample_mp3) is True
