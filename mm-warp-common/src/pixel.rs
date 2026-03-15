/// Convert ARGB8888 (Wayland little-endian: [B,G,R,A]) to RGBA.
/// Both `src` and `dst` must be at least `width * height * 4` bytes.
pub fn argb8888_to_rgba(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    let pixel_count = (width as usize) * (height as usize);
    debug_assert!(src.len() >= pixel_count * 4, "src too short: {} < {}", src.len(), pixel_count * 4);
    debug_assert!(dst.len() >= pixel_count * 4, "dst too short: {} < {}", dst.len(), pixel_count * 4);
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

/// Convert RGBA to ARGB8888 (Wayland little-endian: [B,G,R,A]).
/// Both `src` and `dst` must be at least `width * height * 4` bytes.
pub fn rgba_to_argb8888(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    let pixel_count = (width as usize) * (height as usize);
    debug_assert!(src.len() >= pixel_count * 4, "src too short: {} < {}", src.len(), pixel_count * 4);
    debug_assert!(dst.len() >= pixel_count * 4, "dst too short: {} < {}", dst.len(), pixel_count * 4);
    for i in 0..(width * height) as usize {
        let idx = i * 4;
        // RGBA → ARGB8888 (little-endian: [B,G,R,A])
        dst[idx] = src[idx + 2];     // B
        dst[idx + 1] = src[idx + 1]; // G
        dst[idx + 2] = src[idx];     // R
        dst[idx + 3] = src[idx + 3]; // A
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_to_rgba_known_values() {
        // ARGB8888 in memory (little-endian) = [B, G, R, A]
        // Red pixel: A=0xFF, R=0xFF, G=0x00, B=0x00 → memory: [0x00, 0x00, 0xFF, 0xFF]
        let argb = [0x00u8, 0x00, 0xFF, 0xFF]; // B=0, G=0, R=0xFF, A=0xFF
        let mut rgba = [0u8; 4];
        argb8888_to_rgba(&argb, &mut rgba, 1, 1);
        // RGBA: [R, G, B, A] = [0xFF, 0x00, 0x00, 0xFF]
        assert_eq!(rgba, [0xFF, 0x00, 0x00, 0xFF]);
    }

    #[test]
    fn rgba_to_argb_known_values() {
        // RGBA: [R=0xFF, G=0x00, B=0x00, A=0xFF] (red pixel)
        let rgba = [0xFFu8, 0x00, 0x00, 0xFF];
        let mut argb = [0u8; 4];
        rgba_to_argb8888(&rgba, &mut argb, 1, 1);
        // ARGB8888 memory: [B=0x00, G=0x00, R=0xFF, A=0xFF]
        assert_eq!(argb, [0x00, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn round_trip_argb_rgba_argb() {
        // Start with arbitrary ARGB pixel: [B=0x11, G=0x22, R=0x33, A=0x44]
        let original = [0x11u8, 0x22, 0x33, 0x44];
        let mut rgba = [0u8; 4];
        let mut back = [0u8; 4];
        argb8888_to_rgba(&original, &mut rgba, 1, 1);
        rgba_to_argb8888(&rgba, &mut back, 1, 1);
        assert_eq!(original, back);
    }

    #[test]
    fn round_trip_rgba_argb_rgba() {
        // Start with arbitrary RGBA pixel: [R=0xAA, G=0xBB, B=0xCC, A=0xDD]
        let original = [0xAAu8, 0xBB, 0xCC, 0xDD];
        let mut argb = [0u8; 4];
        let mut back = [0u8; 4];
        rgba_to_argb8888(&original, &mut argb, 1, 1);
        argb8888_to_rgba(&argb, &mut back, 1, 1);
        assert_eq!(original, back);
    }

    #[test]
    fn multi_pixel_round_trip() {
        // 2x2 image, 4 different pixels
        let original_rgba: [u8; 16] = [
            0xFF, 0x00, 0x00, 0xFF, // red
            0x00, 0xFF, 0x00, 0xFF, // green
            0x00, 0x00, 0xFF, 0xFF, // blue
            0xFF, 0xFF, 0xFF, 0x80, // white semi-transparent
        ];
        let mut argb = [0u8; 16];
        let mut back = [0u8; 16];
        rgba_to_argb8888(&original_rgba, &mut argb, 2, 2);
        argb8888_to_rgba(&argb, &mut back, 2, 2);
        assert_eq!(original_rgba, back);
    }

    #[test]
    fn all_zeros() {
        let src = [0u8; 4];
        let mut dst = [0xFFu8; 4];
        argb8888_to_rgba(&src, &mut dst, 1, 1);
        assert_eq!(dst, [0, 0, 0, 0]);
    }

    #[test]
    fn all_ones() {
        let src = [0xFFu8; 4];
        let mut dst = [0u8; 4];
        argb8888_to_rgba(&src, &mut dst, 1, 1);
        assert_eq!(dst, [0xFF, 0xFF, 0xFF, 0xFF]);
    }
}
