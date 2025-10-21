use anyhow::Result;

fn main() -> Result<()> {
    println!("=== H.264 Encoder Debug (Multiple Frames) ===\n");

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
    encoder.set_gop(1); // Keyframe every frame
    encoder.set_max_b_frames(0);

    let mut encoder = encoder.open_as(codec)?;
    println!("✅ Encoder ready (GOP=1, no B-frames)\n");

    // Helper to receive packets
    let mut try_receive = |encoder: &mut ffmpeg_next::encoder::Video| -> usize {
        let mut packet = ffmpeg_next::Packet::empty();
        let mut total = 0;
        while encoder.receive_packet(&mut packet).is_ok() {
            let size = packet.data().map(|d| d.len()).unwrap_or(0);
            total += size;
        }
        total
    };

    println!("Sending 5 frames sequentially...\n");

    for frame_num in 0..5 {
        // Create and allocate frame
        let mut frame = ffmpeg_next::frame::Video::empty();
        frame.set_width(320);
        frame.set_height(240);
        frame.set_format(ffmpeg_next::format::Pixel::YUV420P);
        unsafe {
            ffmpeg_next::sys::av_frame_get_buffer(frame.as_mut_ptr(), 0);
        }

        // Fill with varying brightness
        let brightness = (frame_num * 50 + 50) as u8;
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

        // Send frame
        encoder.send_frame(&frame)?;
        println!("Frame {}: Sent (brightness={})", frame_num + 1, brightness);

        // Try to receive immediately
        let bytes = try_receive(&mut encoder);
        if bytes > 0 {
            println!("Frame {}: ✅ Got {} bytes output!", frame_num + 1, bytes);
        } else {
            println!("Frame {}: Buffered (0 bytes out)", frame_num + 1);
        }
    }

    println!("\n--- Flushing encoder (send EOF) ---\n");
    encoder.send_eof()?;

    let flushed_bytes = try_receive(&mut encoder);
    println!("Flushed: {} bytes", flushed_bytes);

    println!("\n=== RESULT ===");
    if flushed_bytes > 0 {
        println!("✅ SUCCESS! Encoder works after flush");
        println!("\nConclusion: Encoder buffers frames, need EOF to get output");
    } else {
        println!("❌ Even flush didn't work - deeper problem");
    }

    Ok(())
}
