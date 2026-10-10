//! Renders the app icon PNG. Used by `bundle.sh`:
//!   cargo run --example make_icon -- build/icon-1024.png

use image::{ImageBuffer, Rgba, RgbaImage};
use std::path::PathBuf;

fn blend(px: &mut Rgba<u8>, color: Rgba<u8>) {
    let a = color[3] as u32;
    if a == 0 {
        return;
    }
    for channel in 0..3 {
        let out = (color[channel] as u32 * a + px[channel] as u32 * (255 - a)) / 255;
        px[channel] = out as u8;
    }
    px[3] = (a + (px[3] as u32 * (255 - a)) / 255) as u8;
}

fn fill(img: &mut RgbaImage, rect: (u32, u32, u32, u32), color: Rgba<u8>) {
    let (x, y, w, h) = rect;
    for row in y..(y + h).min(img.height()) {
        for column in x..(x + w).min(img.width()) {
            blend(img.get_pixel_mut(column, row), color);
        }
    }
}

fn triangle(img: &mut RgbaImage, points: [(u32, u32); 3], color: Rgba<u8>) {
    let min_x = points.iter().map(|p| p.0).min().unwrap();
    let max_x = points.iter().map(|p| p.0).max().unwrap();
    let min_y = points.iter().map(|p| p.1).min().unwrap();
    let max_y = points.iter().map(|p| p.1).max().unwrap();
    let [a, b, c] = points;
    let edge = |p: (u32, u32), u: (u32, u32), v: (u32, u32)| {
        (p.0 as i64 - u.0 as i64) * (v.1 as i64 - u.1 as i64)
            - (p.1 as i64 - u.1 as i64) * (v.0 as i64 - u.0 as i64)
    };
    for row in min_y..=max_y {
        for column in min_x..=max_x {
            let p = (column, row);
            let (s1, s2, s3) = (edge(p, a, b), edge(p, b, c), edge(p, c, a));
            let inside = (s1 >= 0 && s2 >= 0 && s3 >= 0) || (s1 <= 0 && s2 <= 0 && s3 <= 0);
            if inside {
                blend(img.get_pixel_mut(column, row), color);
            }
        }
    }
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("icon-1024.png"));

    let size = 1024u32;
    let mut img = ImageBuffer::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    let unit = size as f32 / 64.0;
    let u = |v: f32| (v * unit) as u32;

    let manila = Rgba([246, 215, 135, 255]);
    let manila_dark = Rgba([196, 160, 80, 255]);
    let white = Rgba([255, 255, 255, 255]);
    let outline = Rgba([90, 90, 90, 255]);
    let sky = Rgba([70, 130, 220, 255]);
    let grass = Rgba([70, 175, 90, 255]);
    let sun = Rgba([250, 210, 70, 255]);

    // Folder behind the photo.
    fill(&mut img, (u(6.0), u(12.0), u(30.0), u(6.0)), manila_dark);
    fill(&mut img, (u(4.0), u(17.0), u(56.0), u(40.0)), manila);
    fill(&mut img, (u(4.0), u(17.0), u(56.0), u(2.0)), outline);

    // Photo card.
    fill(&mut img, (u(14.0), u(22.0), u(40.0), u(34.0)), outline);
    fill(&mut img, (u(16.0), u(24.0), u(36.0), u(30.0)), white);
    fill(&mut img, (u(18.0), u(26.0), u(32.0), u(26.0)), sky);
    triangle(
        &mut img,
        [(u(18.0), u(52.0)), (u(30.0), u(34.0)), (u(42.0), u(52.0))],
        grass,
    );
    fill(&mut img, (u(38.0), u(29.0), u(7.0), u(7.0)), sun);

    img.save(&out).expect("write icon png");
    println!("wrote {}", out.display());
}
