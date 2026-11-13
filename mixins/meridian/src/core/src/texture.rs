//! Mean colours of block textures, for the "accurate" block colour style.
//!
//! The textures are the player's own: the PNG files of the game's prepared
//! resources (Meridian ships none). Animated textures are strips of square
//! frames; only the first frame counts. Clear pixels are left out, partly
//! clear ones count by their opacity.

/// The mean colour (0xRRGGBB) of a PNG texture, or `None` when it cannot be
/// read or has no visible pixel.
pub fn mean_color(png_bytes: &[u8]) -> Option<u32> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    // The first square frame of an animation strip.
    let rows = h.min(w);
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return None,
    };
    let (mut r, mut g, mut b, mut weight) = (0u64, 0u64, 0u64, 0u64);
    for row in buf[..info.line_size * rows].chunks_exact(info.line_size) {
        for px in row[..w * channels].chunks_exact(channels) {
            let (pr, pg, pb, a) = match channels {
                4 => (px[0], px[1], px[2], px[3]),
                3 => (px[0], px[1], px[2], 255),
                2 => (px[0], px[0], px[0], px[1]),
                _ => (px[0], px[0], px[0], 255),
            };
            let a = a as u64;
            r += pr as u64 * a;
            g += pg as u64 * a;
            b += pb as u64 * a;
            weight += a;
        }
    }
    if weight == 0 {
        return None;
    }
    let c = |v: u64| ((v + weight / 2) / weight) as u32;
    Some(c(r) << 16 | c(g) << 8 | c(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn averages_visible_pixels() {
        // Two pixels: red opaque, blue clear: only red counts.
        let bytes = png(2, 1, png::ColorType::Rgba, &[255, 0, 0, 255, 0, 0, 255, 0]);
        assert_eq!(mean_color(&bytes), Some(0xFF0000));
        let half = png(2, 1, png::ColorType::Rgb, &[200, 100, 0, 0, 100, 200]);
        assert_eq!(mean_color(&half), Some(0x646464));
        let clear = png(1, 1, png::ColorType::Rgba, &[9, 9, 9, 0]);
        assert_eq!(mean_color(&clear), None);
        assert_eq!(mean_color(b"not a png"), None);
    }

    #[test]
    fn animation_strips_use_the_first_frame() {
        // 1x2 strip: frame 1 white, frame 2 black.
        let strip = png(1, 2, png::ColorType::Grayscale, &[255, 0]);
        assert_eq!(mean_color(&strip), Some(0xFFFFFF));
    }
}
