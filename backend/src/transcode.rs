use std::path::Path;

use crate::state::AppState;

/// Stream FLAC download to a temp file, then transcode to MP3 320k writing directly to `mp3_path`.
pub async fn download_and_transcode(
    state: &AppState,
    url: &str,
    mp3_path: &Path,
    job_id: &str,
    track_index: u32,
    duration_s: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = state
        .http_client
        .get(url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await?;

    let content_length = resp.content_length().unwrap_or(0);
    let tmp_path = mp3_path.with_extension(format!("{track_index}.flac.tmp"));

    // Stream download directly to disk instead of buffering in memory
    let mut downloaded: u64 = 0;
    let mut last_dl_pct: i32 = -1;
    {
        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::io::BufWriter::new(tokio::fs::File::create(&tmp_path).await?);
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if content_length > 0 {
                let pct = (downloaded * 100 / content_length) as i32;
                if pct >= last_dl_pct + 5 {
                    last_dl_pct = pct;
                    state
                        .emit(
                            "track_progress",
                            serde_json::json!({
                                "job_id": job_id, "index": track_index,
                                "phase": "download", "pct": pct,
                            }),
                        )
                        .await;
                }
            }
        }
        file.flush().await?;
    }

    tracing::info!(job_id, "Track {track_index}: downloaded {downloaded} bytes, transcoding");

    let mp3_path = mp3_path.to_path_buf();
    let tmp_path_clone = tmp_path.clone();
    let job_id_owned = job_id.to_string();
    let state_clone = state.clone();
    let rt = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        transcode_flac_to_mp3(&tmp_path_clone, &mp3_path, duration_s, &job_id_owned, track_index, &state_clone, &rt)
    })
    .await?;

    let _ = tokio::fs::remove_file(&tmp_path).await;
    result
}

fn transcode_flac_to_mp3(
    flac_path: &Path,
    mp3_path: &Path,
    duration_s: u64,
    job_id: &str,
    track_index: u32,
    state: &AppState,
    rt: &tokio::runtime::Handle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ffmpeg_the_third as ffmpeg;
    ffmpeg::init().map_err(|e| format!("ffmpeg init: {e}"))?;

    let result = do_transcode(flac_path, mp3_path, duration_s, job_id, track_index, state, rt);
    result
}

/// Per-plane FIFO for planar audio formats.
/// Each channel gets its own Vec<u8> buffer.
struct PlanarFifo {
    planes: Vec<Vec<u8>>,
    bytes_per_sample: usize,
}

impl PlanarFifo {
    fn new(channels: usize, bytes_per_sample: usize) -> Self {
        Self {
            planes: (0..channels).map(|_| Vec::new()).collect(),
            bytes_per_sample,
        }
    }

    /// Number of complete samples available (per channel).
    fn samples(&self) -> usize {
        self.planes
            .first()
            .map(|p| p.len() / self.bytes_per_sample)
            .unwrap_or(0)
    }

    /// Append decoded planar frame data into the FIFO.
    fn push(&mut self, frame: &ffmpeg_the_third::frame::Audio) {
        let n = frame.samples();
        if n == 0 {
            return;
        }
        let byte_len = n * self.bytes_per_sample;
        unsafe {
            let ptr = frame.as_ptr();
            for (i, plane) in self.planes.iter_mut().enumerate() {
                let src = (*ptr).data[i] as *const u8;
                plane.extend_from_slice(std::slice::from_raw_parts(src, byte_len));
            }
        }
    }

    /// Drain `count` samples from each plane, returning per-plane byte vecs.
    fn drain(&mut self, count: usize) -> Vec<Vec<u8>> {
        let byte_len = count * self.bytes_per_sample;
        self.planes
            .iter_mut()
            .map(|plane| plane.drain(..byte_len).collect())
            .collect()
    }
}

fn do_transcode(
    flac_path: &Path,
    mp3_path: &Path,
    duration_s: u64,
    job_id: &str,
    track_index: u32,
    state: &AppState,
    rt: &tokio::runtime::Handle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ffmpeg_the_third as ffmpeg;

    let mut ictx = ffmpeg::format::input(flac_path)?;

    let audio_stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or("No audio stream in FLAC")?;
    let audio_stream_index = audio_stream.index();

    // Decoder
    let decoder_codec =
        ffmpeg::codec::decoder::find(ffmpeg::codec::Id::FLAC).ok_or("FLAC decoder not found")?;
    let mut dec_ctx = ffmpeg::codec::Context::new_with_codec(decoder_codec);
    dec_ctx.set_parameters(audio_stream.parameters())?;
    let mut decoder = dec_ctx.decoder().audio()?;

    // Encoder
    let encoder_codec =
        ffmpeg::codec::encoder::find(ffmpeg::codec::Id::MP3).ok_or("MP3 encoder not found")?;
    let enc_format = encoder_codec
        .audio()
        .ok_or("not audio codec")?
        .formats()
        .ok_or("no formats")?
        .next()
        .ok_or("no format")?;

    let enc_ctx = ffmpeg::codec::Context::new_with_codec(encoder_codec);
    let mut enc_audio = enc_ctx.encoder().audio()?;
    enc_audio.set_rate(decoder.rate() as i32);
    enc_audio.set_bit_rate(320_000);
    enc_audio.set_format(enc_format);
    enc_audio.set_ch_layout(decoder.ch_layout());
    let mut encoder = enc_audio.open_as(encoder_codec)?;

    let enc_frame_size = encoder.frame_size() as usize;
    let enc_rate = encoder.rate();
    let enc_channels = encoder.ch_layout().channels() as usize;
    let bytes_per_sample = enc_format.bytes() as usize;

    // Output muxer
    let mut octx = ffmpeg::format::output(mp3_path)?;
    let out_stream = octx.add_stream(encoder_codec)?;
    let out_stream_index = out_stream.index();
    unsafe {
        let stream_ptr = *(*octx.as_mut_ptr()).streams.add(out_stream_index);
        ffmpeg::sys::avcodec_parameters_from_context((*stream_ptr).codecpar, encoder.as_ptr());
    }
    octx.write_header()?;
    let out_time_base = octx.stream(out_stream_index).unwrap().time_base();

    // Resampler: FLAC (s16/s32 interleaved) → MP3 (s32p/fltp planar) + sample rate match
    let mut resampler = ffmpeg::software::resampling::Context::get2(
        decoder.format(),
        decoder.ch_layout(),
        decoder.rate(),
        enc_format,
        encoder.ch_layout(),
        enc_rate,
    )?;

    let duration_us = duration_s * 1_000_000;
    let mut last_transcode_pct: i32 = -1;
    let mut fifo = PlanarFifo::new(enc_channels, bytes_per_sample);
    let mut pts_counter: i64 = 0;
    let mut decoded_frame = ffmpeg::frame::Audio::empty();
    let mut encoded_packet = ffmpeg::Packet::empty();
    let enc_time_base = ffmpeg::Rational::new(1, enc_rate as i32);

    // Process all input packets
    for result in ictx.packets() {
        let (stream, packet) = result?;
        if stream.index() != audio_stream_index {
            continue;
        }
        decoder.send_packet(&packet)?;
        while decoder.receive_frame(&mut decoded_frame).is_ok() {
            let mut resampled = ffmpeg::frame::Audio::empty();
            resampler.run(&decoded_frame, &mut resampled)?;
            fifo.push(&resampled);
            encode_from_fifo(
                &mut fifo, &mut encoder, &mut encoded_packet, &mut octx,
                &mut pts_counter, enc_frame_size, enc_format, enc_rate,
                enc_time_base, out_time_base, out_stream_index,
                duration_us, &mut last_transcode_pct, state, job_id, track_index, rt, false,
            )?;
        }
    }

    // Flush decoder
    decoder.send_eof()?;
    while decoder.receive_frame(&mut decoded_frame).is_ok() {
        let mut resampled = ffmpeg::frame::Audio::empty();
        resampler.run(&decoded_frame, &mut resampled)?;
        fifo.push(&resampled);
        encode_from_fifo(
            &mut fifo, &mut encoder, &mut encoded_packet, &mut octx,
            &mut pts_counter, enc_frame_size, enc_format, enc_rate,
            enc_time_base, out_time_base, out_stream_index,
            duration_us, &mut last_transcode_pct, state, job_id, track_index, rt, false,
        )?;
    }

    // Flush remaining FIFO
    if fifo.samples() > 0 {
        encode_from_fifo(
            &mut fifo, &mut encoder, &mut encoded_packet, &mut octx,
            &mut pts_counter, enc_frame_size, enc_format, enc_rate,
            enc_time_base, out_time_base, out_stream_index,
            duration_us, &mut last_transcode_pct, state, job_id, track_index, rt, true,
        )?;
    }

    // Flush encoder
    encoder.send_eof()?;
    while encoder.receive_packet(&mut encoded_packet).is_ok() {
        encoded_packet.set_stream(out_stream_index);
        encoded_packet.rescale_ts(enc_time_base, out_time_base);
        encoded_packet.write_interleaved(&mut octx)?;
    }

    octx.write_trailer()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_from_fifo(
    fifo: &mut PlanarFifo,
    encoder: &mut ffmpeg_the_third::codec::encoder::audio::Encoder,
    encoded_packet: &mut ffmpeg_the_third::Packet,
    octx: &mut ffmpeg_the_third::format::context::Output,
    pts_counter: &mut i64,
    enc_frame_size: usize,
    enc_format: ffmpeg_the_third::format::Sample,
    enc_rate: u32,
    enc_time_base: ffmpeg_the_third::Rational,
    out_time_base: ffmpeg_the_third::Rational,
    out_stream_index: usize,
    duration_us: u64,
    last_pct: &mut i32,
    state: &AppState,
    job_id: &str,
    track_index: u32,
    rt: &tokio::runtime::Handle,
    flush: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use ffmpeg_the_third as ffmpeg;

    loop {
        let available = fifo.samples();
        let chunk_samples = if flush && available > 0 && available < enc_frame_size {
            available
        } else if available >= enc_frame_size {
            enc_frame_size
        } else {
            break;
        };

        let plane_data = fifo.drain(chunk_samples);

        // Build a planar frame — set properties, allocate buffer, copy per-plane
        let mut frame = ffmpeg::frame::Audio::empty();
        frame.set_format(enc_format);
        frame.set_samples(chunk_samples);
        frame.set_rate(enc_rate);
        frame.set_pts(Some(*pts_counter));
        *pts_counter += chunk_samples as i64;

        unsafe {
            let enc_ptr = encoder.as_ptr();
            let frame_ptr = frame.as_mut_ptr();
            ffmpeg::sys::av_channel_layout_copy(
                &mut (*frame_ptr).ch_layout,
                &(*enc_ptr).ch_layout,
            );
            ffmpeg::sys::av_frame_get_buffer(frame_ptr, 0);

            // Copy each plane separately
            for (i, data) in plane_data.iter().enumerate() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), (*frame_ptr).data[i], data.len());
            }
        }

        encoder.send_frame(&frame)?;
        while encoder.receive_packet(encoded_packet).is_ok() {
            encoded_packet.set_stream(out_stream_index);
            encoded_packet.rescale_ts(enc_time_base, out_time_base);

            if let Some(pts) = encoded_packet.pts() {
                if duration_us > 0 {
                    let out_us = pts * out_time_base.numerator() as i64 * 1_000_000
                        / out_time_base.denominator() as i64;
                    if out_us >= 0 {
                        let pct = std::cmp::min(100, (out_us as u64 * 100 / duration_us) as i32);
                        if pct >= *last_pct + 5 {
                            *last_pct = pct;
                            let state = state.clone();
                            let job_id = job_id.to_string();
                            rt.block_on(state.emit(
                                "track_progress",
                                serde_json::json!({
                                    "job_id": job_id,
                                    "index": track_index,
                                    "phase": "transcode",
                                    "pct": pct,
                                }),
                            ));
                        }
                    }
                }
            }

            encoded_packet.write_interleaved(octx)?;
        }

        if flush && fifo.samples() < enc_frame_size {
            break;
        }
    }
    Ok(())
}
