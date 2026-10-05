//! The sending side: chirps into the default PipeWire output, timestamped.

use std::thread;
use std::time::Duration;

use anyhow::Context;
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;

use crate::{PERIOD, RATE, chirp};

struct State {
    frame: u64,
    chirp: Vec<f32>,
    noise: u32,
}

fn clock(id: libc::clockid_t) -> i64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(id, &mut t) };
    t.tv_sec * 1_000_000_000 + t.tv_nsec
}

pub fn send(secs: u64) -> anyhow::Result<()> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::NODE_NAME => "latency-probe",
        *pw::keys::APP_NAME => "Latency probe",
        // The same quantum MouseTail's speaker asks for, so the graph runs as it normally does.
        *pw::keys::NODE_LATENCY => "512/48000",
    };
    let stream = pw::stream::StreamBox::new(&core, "latency-probe", props)?;
    let state = State {
        frame: 0,
        chirp: chirp(true, 0.25),
        noise: 1,
    };
    println!("# chirp n emitted_realtime_ns cycle_realtime_ns delay_frames");
    let _listener = stream
        .add_local_listener_with_user_data(state)
        .process(|stream, s| {
            let realtime = clock(libc::CLOCK_REALTIME);
            let monotonic = clock(libc::CLOCK_MONOTONIC);
            let time = stream.time().ok();
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let requested = buffer.requested() as usize;
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            let stride = 2 * size_of::<f32>();
            let mut frames = 0;
            // When this graph cycle started, on the wall clock.
            let cycle = match &time {
                Some(t) if t.now() > 0 => realtime - (monotonic - t.now()),
                _ => realtime,
            };
            let delay = time.as_ref().map_or(0, |t| t.delay());
            if let Some(bytes) = data.data() {
                frames = bytes.len() / stride;
                if requested > 0 {
                    frames = frames.min(requested);
                }
                for (i, out) in bytes[..frames * stride].chunks_mut(stride).enumerate() {
                    let pos = (s.frame % PERIOD) as usize;
                    if pos == 0 {
                        let at = cycle + (i as i64 * 1_000_000_000 / RATE as i64);
                        println!("chirp {} {at} {cycle} {delay}", s.frame / PERIOD);
                    }
                    // Quiet noise (about -40 dBFS) keeps MouseTail sending, and playing, between chirps.
                    s.noise ^= s.noise << 13;
                    s.noise ^= s.noise >> 17;
                    s.noise ^= s.noise << 5;
                    let noise = (s.noise as f32 / u32::MAX as f32 - 0.5) * 0.02;
                    let v = noise + s.chirp.get(pos).copied().unwrap_or(0.0);
                    out[..4].copy_from_slice(&v.to_le_bytes());
                    out[4..].copy_from_slice(&v.to_le_bytes());
                    s.frame += 1;
                }
            }
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = stride as i32;
            *chunk.size_mut() = (frames * stride) as u32;
        })
        .register()?;

    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(RATE);
    info.set_channels(2);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(position);
    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| anyhow::anyhow!("format: {e:?}"))?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&bytes).context("format pod")?];
    stream.connect(
        spa::utils::Direction::Output,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    let (quit_tx, quit_rx) = pw::channel::channel::<()>();
    let ml = mainloop.clone();
    let _quit = quit_rx.attach(mainloop.loop_(), move |()| ml.quit());
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(secs));
        let _ = quit_tx.send(());
    });
    mainloop.run();
    Ok(())
}
