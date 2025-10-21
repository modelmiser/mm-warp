use anyhow::Result;

fn main() -> Result<()> {
    println!("=== H.264 Encoder Debug (Realtime Config) ===\n");

    ffmpeg_next::init()?;
    let codec = ffmpeg_next::encoder::find(ffmpeg_next::codec::Id::H264).unwrap();

    let mut encoder = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()?;

    encoder.set_width(320);
    encoder.set_height(240);
    encoder.set_format(ffmpeg_next::format::Pixel::YUV420P);
    encoder.set_time_base((1, 30));
    encoder.set_frame_rate(Some((30, 1)));
    encoder.set_gop(10); // Reasonable GOP for streaming
    encoder.set_max_b_frames(0); // No B-frames (low latency)

    // Try x264-specific options for low latency
    let mut opts = ffmpeg_next::Dictionary::new();
    opts.set("preset", "ultrafast");
    opts.set("tune", "zerolatency");
    opts.set("intra-refresh", "1"); // Use intra-refresh instead of I-frames
   opts.set("rc-lookahead", "0"); // No lookahead (instant encoding)

    let mut encoder = encoder.open_with(opts)?;
    println!("✅ Encoder ready (zerolatency tune)\n");

    let mut try_receive = |encoder: &mut ffmpeg_next::encoder::Video| -> (usize, usize) {
        let mut packet = ffmpeg_next::Packet::empty();
        let mut total = 0;
        let mut count = 0;
        while encoder.receive_packet(&mut packet).is_ok() {
            let size = packet.data().map(|d| d.len()).unwrap_or(0);
            total += size;
            count += 1;
        }
        (count, total)
    };

    println!("Sending frames one at a time (like real streaming)...\n");

    for frame_num in 0..10 {
        let mut frame = ffmpeg_next::frame::Video::empty();
        frame.set_width(320);
        frame.set_height(240);
        frame.set_format(ffmpeg_next::format::Pixel::YUV420P);
        unsafe {
            ffmpeg_next::sys::av_frame_get_buffer(frame.as_mut_ptr(), 0);
        }

        let brightness = (frame_num * 25 + 50) as u8;
        for y_byte in frame.data_mut(0) {
            *y_byte = brightness;
        }
        for u_byte in frame.data_mut(1) {
            *u_byte = 128;
        }
        for v_byte in frame.data_mut(2) {
            *v_byte = 128;
        }

        frame.set_pts(Some(frame_num as i64));

        encoder.send_frame(&frame)?;
        let (count, bytes) = try_receive(&mut encoder);

        if bytes > 0 {
            println!("Frame {:2}: ✅ {} packets, {} bytes", frame_num + 1, count, bytes);
        } else {
            println!("Frame {:2}: Buffered (0 bytes)", frame_num + 1);
        }
    }

    println!("\n=== RESULT ===");
    println!("If most frames produced output immediately, streaming works!");

    Ok(())
}
