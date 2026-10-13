//! Decoding via Apple's ImageIO (HEIC / HEIF / AVIF and anything else
//! the pure-Rust decoders do not understand).

use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use objc2_core_foundation::{
    CFDictionary, CFIndex, CFNumber, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize, CFURL,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext, CGImage,
};
use objc2_image_io::{kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth, CGImageSource};

use super::{check_decode_dimensions, Decoded};

const CG_ALPHA_PREMULTIPLIED_LAST: u32 = 1;
const CG_BITMAP_BYTE_ORDER_32_BIG: u32 = 1 << 10;

pub fn decode(path: &Path) -> Result<Decoded, String> {
    let bytes = path.as_os_str().as_bytes();

    unsafe {
        let url = CFURL::from_file_system_representation_relative_to_base(
            None,
            bytes.as_ptr(),
            bytes.len() as CFIndex,
            false,
            None,
        )
        .ok_or_else(|| "invalid URL".to_string())?;

        let source = CGImageSource::with_url(&url, None).ok_or("ImageIO could not open file")?;
        let properties = source
            .properties_at_index(0, None)
            .ok_or("ImageIO could not read image dimensions")?;
        // ImageIO property dictionaries have CFString keys and CFType values.
        let properties: CFRetained<CFDictionary<CFString, CFType>> =
            CFRetained::cast_unchecked(properties);
        let dimension = |key| -> Result<u32, String> {
            properties
                .get(key)
                .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i64))
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| "ImageIO returned invalid image dimensions".to_owned())
        };
        check_decode_dimensions(
            dimension(kCGImagePropertyPixelWidth)?,
            dimension(kCGImagePropertyPixelHeight)?,
        )?;
        let image = source
            .image_at_index(0, None)
            .ok_or("ImageIO could not decode file")?;

        let width = CGImage::width(Some(&image)) as u32;
        let height = CGImage::height(Some(&image)) as u32;
        if width == 0 || height == 0 {
            return Err("ImageIO returned an empty image".to_string());
        }
        check_decode_dimensions(width, height)?;

        let space =
            CGColorSpace::new_device_rgb().ok_or_else(|| "no RGB colorspace".to_string())?;
        let bytes_per_row = (width as usize) * 4;
        let context = CGBitmapContextCreate(
            std::ptr::null_mut(),
            width as usize,
            height as usize,
            8,
            bytes_per_row,
            Some(&space),
            CG_ALPHA_PREMULTIPLIED_LAST | CG_BITMAP_BYTE_ORDER_32_BIG,
        )
        .ok_or_else(|| "could not create bitmap context".to_string())?;

        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContext::draw_image(Some(&context), rect, Some(&image));

        let ptr = CGBitmapContextGetData(Some(&context)) as *const u8;
        if ptr.is_null() {
            return Err("bitmap context has no data".to_string());
        }
        let len = bytes_per_row * height as usize;
        let rgba = std::slice::from_raw_parts(ptr, len).to_vec();

        Ok(Decoded {
            width,
            height,
            rgba,
            premultiplied: true,
        })
    }
}
