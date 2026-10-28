use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut images = Vec::new();
    for size in sizes {
        let path = format!("../../assets/linux-icons/{size}x{size}/apps/lumiere.png");
        println!("cargo:rerun-if-changed={path}");
        images.push(fs::read(path)?);
    }

    // ICO 容器可直接保存 PNG，讓檔案總管依顯示尺寸選擇合適的圖示。
    let mut icon = Vec::new();
    icon.extend_from_slice(&0u16.to_le_bytes());
    icon.extend_from_slice(&1u16.to_le_bytes());
    icon.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, image) in sizes.into_iter().zip(&images) {
        icon.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        icon.extend_from_slice(&1u16.to_le_bytes());
        icon.extend_from_slice(&32u16.to_le_bytes());
        icon.extend_from_slice(&(image.len() as u32).to_le_bytes());
        icon.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        icon.extend_from_slice(&image);
    }
    let path = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR 未設定")).join("lumiere.ico");
    fs::write(&path, icon)?;
    winresource::WindowsResource::new()
        .set_icon(path.to_str().expect("圖示路徑不是 UTF-8"))
        .compile()
}
