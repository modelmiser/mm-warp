use anyhow::Result;

/// Debug tool for H.264 encoder behavior.
///
/// Modes:
///   single   - encode one frame, check output
///   multi    - encode 5 frames with varying brightness, check buffering
///   realtime - encode 10 frames with zerolatency tune, check per-frame output
fn main() -> Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "realtime".to_string());

    ffmpeg_next::init()?;
    let codec = ffmpeg_next::encoder::find(ffmpeg_next::codec::Id::H264)
        .ok_or_else(|| anyhow::anyhow!("H.264 codec not found"))?;
    println!("✅ H.264 codec: {:?}\n", codec.name());

    match mode.as_str() {
        "single" => run_single(codec),
        "multi" => run_multi(codec),
        "realtime" => run_realtime(codec),
        other => {
            eprintln!("Unknown mode: {other}. Use: single, multi, realtime");
            std::process::exit(1);
        }
    }
}

fn make_frame(width: u32, height: u32, brightness: u8, pts: i64) -> ffmpeg_next::frame::Video {
    let mut frame = ffmpeg_next::frame::Video::empty();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(ffmpeg_next::format::Pixel::YUV420P);
    unsafe { ffmpeg_next::sys::av_frame_get_buffer(frame.as_mut_ptr(), 0); }
    for b in frame.data_mut(0) { *b = brightness; }
    for b in frame.data_mut(1) { *b = 128; }
    for b in frame.data_mut(2) { *b = 128; }
    frame.set_pts(Some(pts));
    frame
}

fn receive_all(encoder: &mut ffmpeg_next::encoder::Video) -> (usize, usize) {
    let mut packet = ffmpeg_next::Packet::empty();
    let (mut count, mut bytes) = (0, 0);
    while encoder.receive_packet(&mut packet).is_ok() {
        bytes += packet.data().map(|d| d.len()).unwrap_or(0);
        count += 1;
    }
    (count, bytes)
}

fn run_single(codec: ffmpeg_next::Codec) -> Result<()> {
    println!("=== Single Frame Test ===\n");
    let mut enc = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder().video()?;
    enc.set_width(320); enc.set_height(240);
    enc.set_format(ffmpeg_next::format::Pixel::YUV420P);
    enc.set_time_base((1, 30)); enc.set_frame_rate(Some((30, 1)));
    enc.set_gop(1); enc.set_max_b_frames(0);
    let mut enc = enc.open_as(codec)?;

    let frame = make_frame(320, 240, 128, 0);
    enc.send_frame(&frame)?;
    let (_, bytes) = receive_all(&mut enc);
    println!("{}", if bytes > 0 { format!("✅ Got {bytes} bytes") } else { "❌ Zero bytes (try flush)".into() });
    Ok(())
}

fn run_multi(codec: ffmpeg_next::Codec) -> Result<()> {
    println!("=== Multi Frame + Flush Test ===\n");
    let mut enc = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder().video()?;
    enc.set_width(320); enc.set_height(240);
    enc.set_format(ffmpeg_next::format::Pixel::YUV420P);
    enc.set_time_base((1, 30)); enc.set_frame_rate(Some((30, 1)));
    enc.set_gop(1); enc.set_max_b_frames(0);
    let mut enc = enc.open_as(codec)?;

    for i in 0..5i64 {
        let frame = make_frame(320, 240, (i * 50 + 50) as u8, i);
        enc.send_frame(&frame)?;
        let (_, bytes) = receive_all(&mut enc);
        println!("Frame {}: {}", i + 1, if bytes > 0 { format!("✅ {bytes} bytes") } else { "buffered".into() });
    }
    enc.send_eof()?;
    let (_, flushed) = receive_all(&mut enc);
    println!("\nFlushed: {flushed} bytes");
    Ok(())
}

fn run_realtime(codec: ffmpeg_next::Codec) -> Result<()> {
    println!("=== Realtime (zerolatency) Test ===\n");
    let mut enc = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder().video()?;
    enc.set_width(320); enc.set_height(240);
    enc.set_format(ffmpeg_next::format::Pixel::YUV420P);
    enc.set_time_base((1, 30)); enc.set_frame_rate(Some((30, 1)));
    enc.set_gop(10); enc.set_max_b_frames(0);
    let mut opts = ffmpeg_next::Dictionary::new();
    opts.set("preset", "ultrafast");
    opts.set("tune", "zerolatency");
    opts.set("intra-refresh", "1");
    opts.set("rc-lookahead", "0");
    let mut enc = enc.open_with(opts)?;

    for i in 0..10i64 {
        let frame = make_frame(320, 240, (i * 25 + 50) as u8, i);
        enc.send_frame(&frame)?;
        let (count, bytes) = receive_all(&mut enc);
        println!("Frame {:2}: {}", i + 1,
            if bytes > 0 { format!("✅ {count} pkts, {bytes} bytes") } else { "buffered".into() });
    }
    Ok(())
}
