//! 驗證 FFmpeg 解碼與 RGBA 轉換；不是正式播放器。

use ffmpeg::format::{self, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context as Scaler, flag::Flags};
use ffmpeg::util::frame::video::Video;
use ffmpeg_next as ffmpeg;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;
    let path = std::env::args().nth(1).ok_or("missing input file")?;
    let mut input = format::input(&path)?;
    let stream = input
        .streams()
        .best(Type::Video)
        .ok_or("missing video stream")?;
    let index = stream.index();
    let mut decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;
    let mut scaler = Scaler::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGBA,
        decoder.width(),
        decoder.height(),
        Flags::BILINEAR,
    )?;
    let mut frames = 0usize;
    let mut drain = |decoder: &mut ffmpeg::decoder::Video| -> Result<(), ffmpeg::Error> {
        let mut decoded = Video::empty();
        while decoder.receive_frame(&mut decoded).is_ok() {
            let mut rgba = Video::empty();
            scaler.run(&decoded, &mut rgba)?;
            assert!(rgba.stride(0) >= rgba.width() as usize * 4);
            frames += 1;
        }
        Ok(())
    };
    for (stream, packet) in input.packets() {
        if stream.index() == index {
            decoder.send_packet(&packet)?;
            drain(&mut decoder)?;
        }
    }
    decoder.send_eof()?;
    drain(&mut decoder)?;
    println!(
        "{path}: {}x{} frames={frames}",
        decoder.width(),
        decoder.height()
    );
    Ok(())
}
