/// Convert ARGB8888 (Wayland little-endian: [B,G,R,A]) to RGBA
pub fn argb8888_to_rgba(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    for i in 0..(width * height) as usize {
        let idx = i * 4;
        // ARGB: [B, G, R, A] in memory (little-endian)
        // RGBA: [R, G, B, A]
        dst[idx] = src[idx + 2];     // R
        dst[idx + 1] = src[idx + 1]; // G
        dst[idx + 2] = src[idx];     // B
        dst[idx + 3] = src[idx + 3]; // A
    }
}

/// Convert RGBA to ARGB8888 (Wayland little-endian: [B,G,R,A])
pub fn rgba_to_argb8888(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    for i in 0..(width * height) as usize {
        let idx = i * 4;
        // RGBA → ARGB8888 (little-endian: [B,G,R,A])
        dst[idx] = src[idx + 2];     // B
        dst[idx + 1] = src[idx + 1]; // G
        dst[idx + 2] = src[idx];     // R
        dst[idx + 3] = src[idx + 3]; // A
    }
}
