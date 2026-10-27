use image::{imageops, ImageDecoder, ImageReader, RgbaImage};
use std::{fs, io::BufReader, path::Path};
pub const MAX_DECODE_PIXELS: u64 = 50_000_000;
/// A decoded image in raw RGBA8 form.
#[derive(Debug)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// True when the RGB channels are already multiplied by alpha.
    pub premultiplied: bool,
    /// Original file dimensions when ImageIO decodes a reduced representation.
    pub original_size: Option<(u32, u32)>,
}

impl Decoded {
    pub fn from_rgba(rgba: RgbaImage) -> Self {
        let (width, height) = (rgba.width(), rgba.height());
        Self {
            width,
            height,
            rgba: rgba.into_raw(),
            premultiplied: false,
            original_size: None,
        }
    }

    pub fn scaled(&self, max_w: u32, max_h: u32) -> Decoded {
        if self.width == 0 || self.height == 0 {
            return Decoded {
                width: 0,
                height: 0,
                rgba: Vec::new(),
                premultiplied: self.premultiplied,
                original_size: self.original_size,
            };
        }
        let scale = (max_w as f64 / self.width as f64).min(max_h as f64 / self.height as f64);
        let scale = scale.min(1.0);
        let w = (self.width as f64 * scale).round().max(1.0) as u32;
        let h = (self.height as f64 * scale).round().max(1.0) as u32;
        if w == self.width && h == self.height {
            return Decoded {
                width: self.width,
                height: self.height,
                rgba: self.rgba.clone(),
                premultiplied: self.premultiplied,
                original_size: self.original_size,
            };
        }
        let img = RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .expect("rgba buffer size mismatch");
        let out = imageops::thumbnail(&img, w, h);
        let mut scaled = Decoded::from_rgba(out);
        scaled.original_size = self.original_size;
        scaled
    }
}

pub fn decode_with_image(path: &Path) -> Result<Decoded, String> {
    let mut reader = ImageReader::open(path)
        .map_err(|e| format!("open failed: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("format failed: {e}"))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_PIXELS * 8);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(decode_error)?;
    let (width, height) = decoder.dimensions();
    check_decode_dimensions(width, height).map_err(|error| format!("resource limit: {error}"))?;
    let img = image::DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    Ok(Decoded::from_rgba(img.to_rgba8()))
}

fn decode_error(error: image::ImageError) -> String {
    if matches!(error, image::ImageError::Limits(_)) {
        format!("resource limit: {error}")
    } else {
        format!("decode failed: {error}")
    }
}

pub fn check_decode_dimensions(width: u32, height: u32) -> Result<(), String> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0 || height == 0 || pixels > MAX_DECODE_PIXELS {
        return Err(format!(
            "image is too large to decode safely ({width} × {height}; limit is {MAX_DECODE_PIXELS} pixels)"
        ));
    }
    Ok(())
}

pub fn decode_pcx(path: &Path) -> Result<Decoded, String> {
    let mut reader = pcx::Reader::from_file(path).map_err(|e| format!("pcx: {e}"))?;
    let (w, h) = reader.dimensions();
    let (w, h) = (w as u32, h as u32);
    check_decode_dimensions(w, h)?;
    let mut rgb = vec![0u8; (w as usize) * (h as usize) * 3];
    reader
        .read_rgb_pixels(&mut rgb)
        .map_err(|e| format!("pcx: {e}"))?;
    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for &[red, green, blue] in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[red, green, blue, 255]);
    }
    Ok(Decoded {
        width: w,
        height: h,
        rgba,
        premultiplied: false,
        original_size: None,
    })
}

/// Rotate / flip so that the stored EXIF orientation is baked into the pixels.
pub fn apply_orientation(path: &Path, img: Decoded) -> Decoded {
    // Reduced ImageIO thumbnails already include the EXIF transform.
    if img.original_size.is_some() {
        return img;
    }
    let orientation = read_orientation(path).unwrap_or(1);
    if !(2..=8).contains(&orientation) {
        return img;
    }
    let Some(image) = RgbaImage::from_raw(img.width, img.height, img.rgba.clone()) else {
        return img;
    };
    // Corrections per the EXIF 2.3 spec ("where the 0th row / column of the
    // stored image appears in the displayed one"), matching PIL's
    // exif_transpose table: 5 is the main-diagonal transpose, 7 the
    // anti-diagonal one.
    let image = match orientation {
        2 => imageops::flip_horizontal(&image),
        3 => imageops::rotate180(&image),
        4 => imageops::flip_vertical(&image),
        5 => imageops::rotate90(&imageops::flip_vertical(&image)),
        6 => imageops::rotate90(&image),
        7 => imageops::rotate90(&imageops::flip_horizontal(&image)),
        8 => imageops::rotate270(&image),
        _ => image,
    };
    Decoded::from_rgba(image)
}

fn read_orientation(path: &Path) -> Option<u32> {
    use exif::{In, Reader, Tag, Value};
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = Reader::new().read_from_container(&mut reader).ok()?;
    let field = exif.get_field(Tag::Orientation, In::PRIMARY)?;
    match &field.value {
        Value::Short(values) => values.first().map(|v| u32::from(*v)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_decode_bomb_before_allocating_pixels() {
        assert!(check_decode_dimensions(5_001, 10_000).is_err());
        assert!(check_decode_dimensions(5_000, 10_000).is_ok());
    }

    #[test]
    fn rejects_zero_dimensions_and_saturating_overflows() {
        assert!(check_decode_dimensions(0, 100).is_err());
        assert!(check_decode_dimensions(100, 0).is_err());
        assert!(check_decode_dimensions(u32::MAX, u32::MAX).is_err());
    }

    fn solid(width: u32, height: u32) -> Decoded {
        Decoded::from_rgba(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([1, 2, 3, 4]),
        ))
    }

    #[test]
    fn from_rgba_copies_pixels_and_defaults_flags() {
        let decoded = solid(2, 1);
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.rgba, vec![1, 2, 3, 4, 1, 2, 3, 4]);
        assert!(!decoded.premultiplied);
        assert_eq!(decoded.original_size, None);
    }

    #[test]
    fn scaled_never_upscales() {
        let scaled = solid(10, 10).scaled(100, 100);
        assert_eq!((scaled.width, scaled.height), (10, 10));
    }

    #[test]
    fn scaled_downscales_preserving_aspect() {
        let scaled = solid(200, 100).scaled(50, 50);
        assert_eq!((scaled.width, scaled.height), (50, 25));
    }

    #[test]
    fn scaled_zero_size_passthrough_preserves_flags() {
        let decoded = Decoded {
            width: 0,
            height: 0,
            rgba: Vec::new(),
            premultiplied: true,
            original_size: Some((9, 9)),
        };
        let scaled = decoded.scaled(10, 10);
        assert_eq!((scaled.width, scaled.height), (0, 0));
        assert!(scaled.premultiplied);
        assert_eq!(scaled.original_size, Some((9, 9)));
    }

    #[test]
    fn scaled_preserves_original_size() {
        let mut decoded = solid(200, 100);
        decoded.original_size = Some((4000, 2000));
        assert_eq!(decoded.scaled(50, 50).original_size, Some((4000, 2000)));
    }

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("lumiere-core-{}-{name}", std::process::id()))
    }

    #[test]
    fn decodes_png_pixels() {
        let path = test_path("png.png");
        image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .expect("write png");
        let decoded = decode_with_image(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.rgba, vec![255, 0, 0, 255, 255, 0, 0, 255]);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn decode_reports_missing_file() {
        let err = decode_with_image(&test_path("missing.png")).unwrap_err();
        assert!(err.contains("open failed"), "{err}");
    }

    #[test]
    fn rejects_oversized_png_before_decoding() {
        let path = test_path("bomb.png");
        image::RgbaImage::from_pixel(10_000, 5_001, image::Rgba([0; 4]))
            .save(&path)
            .expect("write png");
        let err = decode_with_image(&path).unwrap_err();
        assert!(err.contains("resource limit"), "{err}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn decodes_pcx_with_opaque_alpha() {
        let path = test_path("img.pcx");
        let mut writer = pcx::WriterRgb::create_file(&path, (2, 1), (72, 72)).expect("create pcx");
        writer
            .write_row(&[255, 0, 0, 0, 255, 255])
            .expect("write row");
        writer.finish().expect("finish pcx");
        let decoded = decode_pcx(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.rgba, vec![255, 0, 0, 255, 0, 255, 255, 255]);
        let _ = fs::remove_file(&path);
    }

    /// Writes a real JPEG with an APP1/EXIF block whose only tag is the
    /// requested Orientation value.
    fn write_orientation_jpeg(path: &Path, img: &image::RgbImage, orientation: u16) {
        img.save(path).expect("write jpeg");
        let jpeg = fs::read(path).expect("read jpeg");

        // Minimal TIFF: one IFD holding only the Orientation tag (0x0112).
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        let mut entry = [0u8; 12];
        entry[0..2].copy_from_slice(&0x0112u16.to_le_bytes());
        entry[2..4].copy_from_slice(&3u16.to_le_bytes());
        entry[4..8].copy_from_slice(&1u32.to_le_bytes());
        entry[8..10].copy_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&entry);
        tiff.extend_from_slice(&0u32.to_le_bytes());

        let payload = [b"Exif\0\0".to_vec(), tiff].concat();
        let mut segment = vec![0xFF, 0xE1];
        segment.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        segment.extend_from_slice(&payload);

        let mut out = vec![0xFF, 0xD8];
        out.extend_from_slice(&segment);
        out.extend_from_slice(&jpeg[2..]);
        fs::write(path, out).expect("write jpeg with exif");
    }

    #[test]
    fn orientation_is_baked_into_pixels() {
        let path = test_path("orient6.jpg");
        write_orientation_jpeg(
            &path,
            &image::RgbImage::from_pixel(4, 2, image::Rgb([9, 99, 199])),
            6,
        );
        let decoded = decode_with_image(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (4, 2));
        let oriented = apply_orientation(&path, decoded);
        assert_eq!((oriented.width, oriented.height), (2, 4));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn orientation_two_flips_horizontally() {
        // JPEG is lossy and chroma is subsampled 2x, so give each color a pair
        // of pixels (its own chroma sample) and check which side is reddish /
        // bluish rather than exact bytes.
        let mut img = image::RgbImage::new(4, 1);
        for x in 0..4u32 {
            let color = if x < 2 { [255, 0, 0] } else { [0, 0, 255] };
            img.put_pixel(x, 0, image::Rgb(color));
        }
        let path = test_path("orient2.jpg");
        write_orientation_jpeg(&path, &img, 2);
        let decoded = decode_with_image(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (4, 1));
        let oriented = apply_orientation(&path, decoded);
        assert_eq!((oriented.width, oriented.height), (4, 1));
        let (r0, g0, b0) = channels(&oriented.rgba, 0);
        let (r3, g3, b3) = channels(&oriented.rgba, 3);
        assert!(
            b0 > r0 && b0 > g0,
            "first pixel should be bluish after the flip"
        );
        assert!(
            r3 > b3 && r3 > g3,
            "last pixel should be reddish after the flip"
        );
        let _ = fs::remove_file(&path);
    }

    fn channels(rgba: &[u8], index: usize) -> (u8, u8, u8) {
        let p = &rgba[index * 4..index * 4 + 3];
        (p[0], p[1], p[2])
    }

    /// A 4x2 image built from 2x2 color blocks so JPEG chroma subsampling
    /// keeps each block's color intact: `[R R | G G]` over `[B B | W W]`.
    fn block_image() -> image::RgbImage {
        let colors = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];
        let mut img = image::RgbImage::new(4, 2);
        for y in 0..2u32 {
            for x in 0..4u32 {
                let block = (y as usize) * 2 + (x / 2) as usize;
                img.put_pixel(x, y, image::Rgb(colors[block]));
            }
        }
        img
    }

    #[test]
    fn orientation_four_flips_vertically() {
        let mut img = image::RgbImage::new(2, 4);
        for y in 0..4u32 {
            let color = if y < 2 { [255, 0, 0] } else { [0, 0, 255] };
            for x in 0..2u32 {
                img.put_pixel(x, y, image::Rgb(color));
            }
        }
        let path = test_path("orient4.jpg");
        write_orientation_jpeg(&path, &img, 4);
        let decoded = decode_with_image(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (2, 4));
        let oriented = apply_orientation(&path, decoded);
        assert_eq!((oriented.width, oriented.height), (2, 4));
        let (r0, g0, b0) = channels(&oriented.rgba, 0);
        let (r6, g6, b6) = channels(&oriented.rgba, 6);
        assert!(
            b0 > r0 && b0 > g0,
            "top pixel should be bluish after the flip"
        );
        assert!(
            r6 > b6 && r6 > g6,
            "bottom pixel should be reddish after the flip"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn orientation_five_is_main_diagonal_transpose() {
        let img = block_image();
        let path = test_path("orient5.jpg");
        write_orientation_jpeg(&path, &img, 5);
        let decoded = decode_with_image(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (4, 2));
        let oriented = apply_orientation(&path, decoded);
        assert_eq!((oriented.width, oriented.height), (2, 4));
        // V(r, c) = S(c, r): [R B] [R B] [G W] [G W].
        let (r, g, b) = channels(&oriented.rgba, 0);
        assert!(r > g && r > b, "top-left should be reddish");
        let (r, g, b) = channels(&oriented.rgba, 1);
        assert!(b > r && b > g, "top-right should be bluish");
        let (r, g, b) = channels(&oriented.rgba, 4);
        assert!(g > r && g > b, "row 2 left should be greenish");
        let (r, g, b) = channels(&oriented.rgba, 7);
        assert!(
            r > 200 && g > 200 && b > 200,
            "row 3 right should be whitish"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn orientation_seven_is_anti_diagonal_transpose() {
        let img = block_image();
        let path = test_path("orient7.jpg");
        write_orientation_jpeg(&path, &img, 7);
        let decoded = decode_with_image(&path).expect("decode");
        assert_eq!((decoded.width, decoded.height), (4, 2));
        let oriented = apply_orientation(&path, decoded);
        assert_eq!((oriented.width, oriented.height), (2, 4));
        // Anti-diagonal transpose: [W G] [W G] [B R] [B R].
        let (r, g, b) = channels(&oriented.rgba, 0);
        assert!(r > 200 && g > 200 && b > 200, "top-left should be whitish");
        let (r, g, b) = channels(&oriented.rgba, 4);
        assert!(b > r && b > g, "row 2 left should be bluish");
        let (r, g, b) = channels(&oriented.rgba, 7);
        assert!(r > g && r > b, "row 3 right should be reddish");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn images_without_exif_orientation_are_unchanged() {
        let mut img = image::RgbaImage::new(4, 2);
        for (x, y) in (0..4u32).flat_map(|x| (0..2u32).map(move |y| (x, y))) {
            img.put_pixel(x, y, image::Rgba([(x * 60) as u8, (y * 120) as u8, 5, 255]));
        }
        let path = test_path("noexif.png");
        img.save(&path).expect("write png");
        let decoded = decode_with_image(&path).expect("decode");
        let oriented = apply_orientation(&path, decoded);
        assert_eq!(oriented.rgba, img.into_raw());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reduced_representations_skip_orientation() {
        let path = test_path("orient6-reduced.jpg");
        write_orientation_jpeg(
            &path,
            &image::RgbImage::from_pixel(4, 2, image::Rgb([9, 99, 199])),
            6,
        );
        let mut decoded = decode_with_image(&path).expect("decode");
        // ImageIO downsampled representations already include the EXIF
        // transform, so apply_orientation must leave them alone.
        decoded.original_size = Some((8000, 4000));
        let oriented = apply_orientation(&path, decoded);
        assert_eq!((oriented.width, oriented.height), (4, 2));
        let _ = fs::remove_file(&path);
    }
}
