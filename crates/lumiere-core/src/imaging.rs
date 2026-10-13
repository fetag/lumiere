use image::{imageops, ImageDecoder, ImageReader, RgbaImage};
use std::{fs, io::BufReader, path::Path};
pub const MAX_DECODE_PIXELS: u64 = 50_000_000;
/// A decoded image in raw RGBA8 form.
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
    for chunk in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
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
    let image = match orientation {
        2 => {
            imageops::flip_horizontal(&image);
            image
        }
        3 => imageops::rotate180(&image),
        4 => {
            imageops::flip_vertical(&image);
            image
        }
        5 => {
            imageops::flip_horizontal(&image);
            imageops::rotate90(&image)
        }
        6 => imageops::rotate90(&image),
        7 => {
            imageops::flip_horizontal(&image);
            imageops::rotate270(&image)
        }
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
}
