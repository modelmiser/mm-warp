use anyhow::Result;

fn main() -> Result<()> {
    println!("=== H.264 Encoder Debug ===\n");

    // Initialize ffmpeg
    ffmpeg_next::init()?;
    println!("✅ FFmpeg initialized\n");

    // Find codec
    let codec = ffmpeg_next::encoder::find(ffmpeg_next::codec::Id::H264)
        .ok_or_else(|| anyhow::anyhow!("H.264 codec not found"))?;
    println!("✅ H.264 codec found: {:?}\n", codec.name());

    // Create encoder context
    let mut encoder = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()?;

    // Configure encoder
    encoder.set_width(320);
    encoder.set_height(240);
    encoder.set_format(ffmpeg_next::format::Pixel::YUV420P);
    encoder.set_time_base((1, 30));
    encoder.set_frame_rate(Some((30, 1)));
    encoder.set_gop(1); // Keyframe every frame
    encoder.set_max_b_frames(0);

    println!("Encoder config:");
    println!("  Size: {}x{}", encoder.width(), encoder.height());
    println!("  Format: {:?}", encoder.format());
    println!("  Time base: {:?}", encoder.time_base());
    println!("  GOP: 1, B-frames: 0\n");

    // Open encoder
    let mut encoder = encoder.open_as(codec)?;
    println!("✅ Encoder opened\n");

    // Create a frame
    let width = 320;
    let height = 240;
    let y_size = width * height;
    let uv_size = y_size / 4;

    println!("Creating frame:");
    println!("  Y plane: {} bytes", y_size);
    println!("  U plane: {} bytes", uv_size);
    println!("  V plane: {} bytes\n", uv_size);

    // Allocate frame properly
    let mut frame = ffmpeg_next::frame::Video::empty();
    frame.set_width(width as u32);
    frame.set_height(height as u32);
    frame.set_format(ffmpeg_next::format::Pixel::YUV420P);

    // THIS IS KEY - must alloc before using data_mut!
    unsafe {
        ffmpeg_next::sys::av_frame_get_buffer(frame.as_mut_ptr(), 0);
    }

    println!("Frame after allocation:");
    println!("  Data[0] len: {}", frame.data(0).len());
    println!("  Data[1] len: {}", frame.data(1).len());
    println!("  Data[2] len: {}\n", frame.data(2).len());

    // Fill with test pattern (gray)
    for y_byte in frame.data_mut(0) {
        *y_byte = 128; // Mid-gray
    }
    for u_byte in frame.data_mut(1) {
        *u_byte = 128; // Neutral chroma
    }
    for v_byte in frame.data_mut(2) {
        *v_byte = 128; // Neutral chroma
    }

    // Set PTS
    frame.set_pts(Some(0));
    println!("✅ Frame filled and PTS set\n");

    // Try to encode
    println!("Sending frame to encoder...");
    encoder.send_frame(&frame)?;
    println!("✅ Frame sent\n");

    // Try to receive packet
    println!("Receiving packets...");
    let mut packet = ffmpeg_next::Packet::empty();
    let mut total_bytes = 0;
    let mut packet_count = 0;

    while encoder.receive_packet(&mut packet).is_ok() {
        let size = packet.data().map(|d| d.len()).unwrap_or(0);
        println!("  Packet {}: {} bytes", packet_count + 1, size);
        total_bytes += size;
        packet_count += 1;
    }

    println!("\n=== RESULT ===");
    println!("Packets received: {}", packet_count);
    println!("Total bytes: {}", total_bytes);

    if total_bytes > 0 {
        println!("\n✅ SUCCESS! Encoder is working!");
    } else {
        println!("\n❌ PROBLEM: Encoder produced zero bytes");
        println!("\nPossible issues:");
        println!("- GOP settings require more frames before output");
        println!("- Need to flush encoder (send EOF)");
        println!("- Frame allocation not correct");
    }

    Ok(())
}
