//! One photo per kept date: decoded, turned the right way up (EXIF), shrunk to
//! fit 1600px and stored as <dir>/<history id>.jpg. Never served from /static.

use std::{io::Cursor, path::PathBuf};

use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageDecoder, ImageReader, Limits};

pub const MAX_UPLOAD_BYTES: usize = 15 * 1024 * 1024;
const MAX_SIDE: u32 = 1600;

pub fn path(dir: &std::path::Path, history_id: i64) -> PathBuf {
    dir.join(format!("{history_id}.jpg"))
}

/// A message safe to show the person when a photo can't be used.
#[derive(Debug)]
pub struct PhotoError(pub &'static str);

/// Decode, orient and shrink; returns JPEG bytes. CPU-heavy: call from spawn_blocking.
pub fn process(bytes: &[u8]) -> Result<Vec<u8>, PhotoError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(PhotoError("That photo is over 15 MB. Try a smaller one."));
    }
    if looks_like_heic(bytes) {
        return Err(PhotoError("That's an HEIC photo, which can't be read here. Choose “Most Compatible” in camera settings, or send a screenshot."));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| PhotoError("That file doesn't look like a photo."))?;
    // Keep decoding inside what a small VM can afford.
    let mut limits = Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| PhotoError("That file doesn't look like a photo."))?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|_| PhotoError("That photo couldn't be read. Try another one."))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    let img = if img.width() > MAX_SIDE || img.height() > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 82)
        .encode_image(&img.to_rgb8())
        .map_err(|_| PhotoError("That photo couldn't be saved. Try another one."))?;
    Ok(out)
}

/// HEIC/HEIF files carry an "ftyp" box with a heic-family brand near the start.
fn looks_like_heic(b: &[u8]) -> bool {
    b.len() > 12 && &b[4..8] == b"ftyp" && matches!(&b[8..12], b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" | b"heim" | b"heis")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};

    #[test]
    fn shrinks_big_photos_to_a_jpeg() {
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::new(3200, 2000)).write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let out = process(&png).unwrap();
        let back = image::load_from_memory(&out).unwrap();
        assert_eq!((back.width(), back.height()), (1600, 1000));
        assert_eq!(image::guess_format(&out).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn says_why_it_cant_use_a_file() {
        let heic = [&[0, 0, 0, 24][..], b"ftypheic", &[0; 16]].concat();
        assert!(process(&heic).unwrap_err().0.contains("HEIC"));
        assert!(process(b"definitely not a photo").is_err());
    }
}
