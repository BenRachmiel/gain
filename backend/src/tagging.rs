use lofty::config::WriteOptions;
use lofty::id3::v2::{
    BinaryFrame, FrameId, Id3v2Tag, SyncTextContentType, SynchronizedTextFrame, TimestampFormat,
};
use lofty::prelude::*;
use lofty::tag::TagType;
use lofty::TextEncoding;
use std::borrow::Cow;
use std::path::Path;

pub fn tag_mp3(
    path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    track_number: u32,
    track_total: u32,
    disc_number: u32,
    disc_total: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tag = Id3v2Tag::default();
    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album.to_string());
    tag.set_track(track_number);
    tag.set_track_total(track_total);
    tag.set_disk(disc_number);
    tag.set_disk_total(disc_total);

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

pub fn embed_cover_art(
    path: &Path,
    image_data: &[u8],
    mime: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tagged_file = lofty::read_from_path(path)?;
    let mut tag: Id3v2Tag = tagged_file
        .tag(TagType::Id3v2)
        .map(|t| t.clone().into())
        .unwrap_or_default();

    let frame_id = FrameId::Valid(Cow::Borrowed("APIC"));
    let mut apic_data = Vec::new();
    apic_data.push(0x00); // UTF-8 encoding
    apic_data.extend_from_slice(mime.as_bytes());
    apic_data.push(0x00); // null terminator for mime
    apic_data.push(0x03); // picture type: front cover
    apic_data.push(0x00); // empty description (null terminator)
    apic_data.extend_from_slice(image_data);

    tag.insert(BinaryFrame::new(frame_id, apic_data).into());
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

pub fn embed_lyrics(
    path: &Path,
    lyrics: &[(String, u32)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tagged_file = lofty::read_from_path(path)?;

    // Build SYLT frame bytes
    let content: Vec<(u32, String)> = lyrics.iter().map(|(t, ms)| (*ms, t.clone())).collect();
    let sylt = SynchronizedTextFrame::new(
        TextEncoding::UTF8,
        *b"eng",
        TimestampFormat::MS,
        SyncTextContentType::Lyrics,
        None,
        content,
    );
    let sylt_bytes = sylt.as_bytes()?;

    // Read existing ID3v2 tag, add SYLT frame, save
    let mut tag = Id3v2Tag::default();
    // Re-read to get Id3v2Tag directly
    if let Some(existing) = tagged_file.tag(TagType::Id3v2) {
        // Copy existing tag items
        tag = existing.clone().into();
    }

    let frame_id = FrameId::Valid(Cow::Borrowed("SYLT"));
    tag.insert(BinaryFrame::new(frame_id, sylt_bytes).into());
    tag.save_to_path(path, WriteOptions::default())?;

    Ok(())
}
