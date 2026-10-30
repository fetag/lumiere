//! libmpv 播放引擎驗證：軟體渲染、暫停及 seek；非正式播放器。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mpv_engine::{Engine, PlaybackEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args().nth(1).ok_or("missing video file")?;
    // SSH session 預設使用 null；傳入 --audio 才使用實際音效輸出。
    let audible = std::env::args().skip(2).any(|arg| arg == "--audio");
    let builder = Engine::video();
    let engine = if audible {
        builder.build()?
    } else {
        builder.property("ao", "null").build()?
    };
    let ready = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ready);
    engine.attach_sw_render(move || flag.store(true, Ordering::SeqCst))?;
    ready.store(false, Ordering::SeqCst);
    eprintln!("Loading media");
    engine.load(&source)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut loaded = false;
    while Instant::now() < deadline {
        for event in engine.pump_events() {
            match event {
                PlaybackEvent::Loaded => loaded = true,
                PlaybackEvent::Failed { code, message } => {
                    return Err(format!("playback failed: {code}: {message}").into())
                }
                _ => {}
            }
        }
        if loaded && ready.load(Ordering::SeqCst) && engine.render_update() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !loaded {
        return Err("media did not load".into());
    }
    eprintln!("Rendering frame");
    let mut rgba = Vec::new();
    engine.render_sw(320, 240, &mut rgba)?;
    if rgba.len() != 320 * 240 * 4 || !rgba.chunks_exact(4).any(|p| p[..3].iter().any(|&x| x != 0))
    {
        return Err("no non-black rendered frame".into());
    }
    eprintln!("Pausing playback");
    engine.set_paused(true)?;
    if !engine.is_paused() {
        return Err("pause did not take effect".into());
    }
    let duration = engine.duration().ok_or("missing duration")?;
    engine.seek_absolute(duration / 2.0)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut position = engine.position().unwrap_or(0.0);
    while Instant::now() < deadline && (position - duration / 2.0).abs() > 0.3 {
        let _ = engine.pump_events();
        std::thread::sleep(Duration::from_millis(20));
        position = engine.position().unwrap_or(0.0);
    }
    if (position - duration / 2.0).abs() > 0.3 {
        return Err(format!("seek did not reach target: {position}/{duration}").into());
    }
    let audio = engine
        .get_property::<String>("audio-codec-name")
        .unwrap_or_else(|_| "none".to_owned());
    eprintln!("Resuming playback");
    engine.set_paused(false)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut resumed = position;
    while Instant::now() < deadline && resumed <= position + 0.1 {
        let _ = engine.pump_events();
        std::thread::sleep(Duration::from_millis(20));
        resumed = engine.position().unwrap_or(position);
    }
    if resumed <= position + 0.1 {
        return Err("playback did not advance after resume".into());
    }
    let audio_output = engine
        .get_property::<String>("current-ao")
        .unwrap_or_else(|_| "none".to_owned());
    let sample_rate = engine
        .get_property::<i64>("audio-out-params/samplerate")
        .unwrap_or(0);
    if audible && (audio == "none" || audio_output == "none" || sample_rate <= 0) {
        return Err(format!(
            "audio output not initialized: ao={audio_output}, sample_rate={sample_rate}"
        )
        .into());
    }
    engine.detach_render();
    println!("RGBA 320x240, pause/resume OK, duration={duration:.2}s, seek={position:.2}s, resumed={resumed:.2}s, audio codec={audio}, output={audio_output}, sample_rate={sample_rate}; audible={audible}");
    Ok(())
}
