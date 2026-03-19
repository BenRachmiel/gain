import logging

from mutagen.id3 import (
    APIC,
    ID3,
    TALB,
    TCON,
    TDRC,
    TIT2,
    TPE1,
    TPE2,
    TRCK,
    TSOP,
)
from mutagen.mp3 import MP3

log = logging.getLogger("musiclib.tags")

# Map of human-readable key → ID3 frame class
_FRAME_MAP = {
    "title": TIT2,
    "artist": TPE1,
    "album_artist": TPE2,
    "album": TALB,
    "track": TRCK,
    "year": TDRC,
    "genre": TCON,
    "artist_sort": TSOP,
}

# Keys to read from ID3 tags
_READ_KEYS = {
    "TIT2": "title",
    "TPE1": "artist",
    "TPE2": "album_artist",
    "TALB": "album",
    "TRCK": "track",
    "TDRC": "year",
    "TCON": "genre",
    "TSOP": "artist_sort",
}


def read_tags(mp3_path: str) -> dict[str, str]:
    """Read relevant ID3 frames from an MP3, returning a dict of human-readable keys."""
    result: dict[str, str] = {}
    try:
        audio = MP3(mp3_path, ID3=ID3)
        if audio.tags is None:
            return result
        for frame_id, key in _READ_KEYS.items():
            frame = audio.tags.get(frame_id)
            if frame:
                result[key] = str(frame)
        # Check for cover art
        for key in audio.tags:
            if key.startswith("APIC"):
                result["has_cover"] = "true"
                break
    except Exception as e:
        log.warning("Failed to read tags from %s: %s", mp3_path, e)
    return result


def write_tags(mp3_path: str, **kwargs: str) -> None:
    """Set specific ID3 frames without clobbering existing ones.

    Only writes frames for keys that are provided and not already set.
    Pass ``force=True`` as a kwarg to overwrite existing values.
    """
    force = kwargs.pop("force", False)
    audio = MP3(mp3_path, ID3=ID3)
    if audio.tags is None:
        audio.add_tags()

    for key, value in kwargs.items():
        frame_cls = _FRAME_MAP.get(key)
        if frame_cls is None:
            log.warning("Unknown tag key: %s", key)
            continue
        existing = audio.tags.get(frame_cls.__name__)
        if existing and not force:
            continue
        audio.tags.add(frame_cls(encoding=3, text=[value]))

    audio.save()


def has_cover_art(mp3_path: str) -> bool:
    """Check whether an MP3 has an APIC (cover art) frame."""
    try:
        audio = MP3(mp3_path, ID3=ID3)
        if audio.tags is None:
            return False
        return any(key.startswith("APIC") for key in audio.tags)
    except Exception:
        return False


def embed_cover_art(mp3_path: str, image_bytes: bytes, mime_type: str = "image/jpeg") -> None:
    """Write an APIC frame (front cover) into an MP3."""
    audio = MP3(mp3_path, ID3=ID3)
    if audio.tags is None:
        audio.add_tags()
    audio.tags.add(
        APIC(
            encoding=3,
            mime=mime_type,
            type=3,  # front cover
            desc="Cover",
            data=image_bytes,
        )
    )
    audio.save()
