//! The listening side: tap what the MouseTail daemon plays, with Core Audio host timestamps.
//!
//! Writes to `dir`:
//! - `capture.f32`: the tap, interleaved stereo f32 at the tap's rate
//! - `chunks.txt`: per IO callback, `host_ns first_frame frames`
//! - `local.txt`: our own falling chirps, `n render_host_ns cpal_latency_ns`
//! - `meta.txt`: the tap's rate, wall clock minus host time, the output device's latencies

use std::ffi::{CStr, c_void};
use std::fmt::Write as _;
use std::io::Write as _;
use std::mem::{MaybeUninit, size_of};
use std::ptr::{self, NonNull};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use objc2::AnyThread;
use objc2_core_audio::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap,
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectPropertyScope, AudioObjectPropertySelector, CATapDescription, CATapMuteBehavior,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyLatency,
    kAudioDevicePropertySafetyOffset, kAudioDevicePropertyStreams,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyTranslatePIDToProcessObject,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject, kAudioStreamPropertyLatency,
    kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
    kAudioTapPropertyUID,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp,
};
use objc2_foundation::{NSArray, NSNumber, NSString};

use crate::{PERIOD, chirp};

unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut [u32; 2]) -> i32;
}

fn host_ns(ticks: u64) -> i64 {
    let mut tb = [0u32; 2];
    unsafe { mach_timebase_info(&mut tb) };
    (ticks as u128 * tb[0] as u128 / tb[1] as u128) as i64
}

fn wall_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64
}

/// Wall clock minus host time, from the tightest of a few readings.
fn wall_minus_host() -> i64 {
    (0..50)
        .map(|_| {
            let a = host_ns(unsafe { mach_absolute_time() });
            let w = wall_ns();
            let b = host_ns(unsafe { mach_absolute_time() });
            (b - a, w - (a + b) / 2)
        })
        .min()
        .unwrap()
        .1
}

struct Chunk {
    host_ns: i64,
    samples: Vec<f32>,
}

pub fn listen(secs: u64, dir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    // The app's daemon, or a development build.
    let pid: i32 = ["mousetaild", "mousetail"]
        .iter()
        .find_map(|name| {
            let out = std::process::Command::new("pgrep")
                .args(["-x", name])
                .output()
                .ok()?;
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()?
                .trim()
                .parse()
                .ok()
        })
        .context("MouseTail isn't running")?;
    let daemon = process_object(pid).context("the daemon has no audio process object")?;
    let me = process_object(std::process::id() as i32);
    println!("tapping MouseTail (pid {pid}, object {daemon}), self {me:?}");

    let mut meta = String::new();
    describe_output(&mut meta)?;

    // Our own falling chirps half a second after each of the sender's, to calibrate the tap.
    let (local_tx, local_rx) = mpsc::channel::<(u64, i64, i64)>();
    let device = cpal::default_host()
        .default_output_device()
        .context("no output")?;
    let supported = device.default_output_config()?;
    let mut config = supported.config();
    // The same buffer the daemon asks for (`PROBE_FRAMES`, default 256), so the calibration
    // matches it.
    let frames = std::env::var("PROBE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256);
    if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
        config.buffer_size = cpal::BufferSize::Fixed(u32::clamp(frames, min, max));
    }
    let (channels, rate) = (config.channels as usize, config.sample_rate);
    writeln!(
        meta,
        "local_rate {rate} local_channels {channels} local_buffer {:?}",
        config.buffer_size
    )?;
    let down = chirp(false, 0.1);
    let period = PERIOD * rate as u64 / crate::RATE as u64;
    let mut frame = 0u64;
    let local = device.build_output_stream(
        config,
        move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
            let now = host_ns(unsafe { mach_absolute_time() });
            let ts = info.timestamp();
            let latency = ts.playback.duration_since(ts.callback).as_nanos() as i64;
            for (i, f) in data.chunks_mut(channels).enumerate() {
                let pos = ((frame + period / 2) % period) as usize;
                if pos == 0 {
                    let at = now + i as i64 * 1_000_000_000 / rate as i64;
                    let _ = local_tx.send((frame / period, at, latency));
                }
                let v = down.get(pos).copied().unwrap_or(0.0);
                f.fill(v);
                frame += 1;
            }
        },
        |e| eprintln!("local output: {e}"),
        None,
    )?;
    local.play()?;
    // Give Core Audio a moment to give us a process object.
    std::thread::sleep(Duration::from_millis(300));
    let me = me.or_else(|| process_object(std::process::id() as i32));

    let mut objects = vec![NSNumber::new_u32(daemon)];
    objects.extend(me.map(NSNumber::new_u32));
    let mut tap: AudioObjectID = 0;
    unsafe {
        let desc = CATapDescription::initStereoMixdownOfProcesses(
            CATapDescription::alloc(),
            &NSArray::from_retained_slice(&objects),
        );
        desc.setName(&NSString::from_str("latency probe"));
        desc.setPrivate(true);
        desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
        check(
            AudioHardwareCreateProcessTap(Some(&desc), &mut tap),
            "create tap",
        )?;
    }
    let tap_uid: CFStringRef = unsafe {
        property(
            tap,
            kAudioTapPropertyUID,
            kAudioObjectPropertyScopeGlobal,
            None,
        )?
    };
    let tap_uid = unsafe { CFString::wrap_under_create_rule(tap_uid) };
    let format: AudioStreamBasicDescription = unsafe {
        property(
            tap,
            kAudioTapPropertyFormat,
            kAudioObjectPropertyScopeGlobal,
            None,
        )?
    };
    writeln!(
        meta,
        "tap_rate {} tap_channels {}",
        format.mSampleRate, format.mChannelsPerFrame
    )?;

    let key = |k: &CStr| CFString::new(k.to_str().unwrap_or_default());
    let tap_list = CFArray::from_CFTypes(&[CFDictionary::from_CFType_pairs(&[
        (key(kAudioSubTapUIDKey), tap_uid.as_CFType()),
        (
            key(kAudioSubTapDriftCompensationKey),
            CFBoolean::true_value().as_CFType(),
        ),
    ])]);
    let agg = CFDictionary::from_CFType_pairs(&[
        (
            key(kAudioAggregateDeviceNameKey),
            CFString::new("latency probe").as_CFType(),
        ),
        (
            key(kAudioAggregateDeviceUIDKey),
            CFString::new(&format!("latency-probe.{tap_uid}")).as_CFType(),
        ),
        (
            key(kAudioAggregateDeviceIsPrivateKey),
            CFBoolean::true_value().as_CFType(),
        ),
        (
            key(kAudioAggregateDeviceTapAutoStartKey),
            CFBoolean::true_value().as_CFType(),
        ),
        (key(kAudioAggregateDeviceTapListKey), tap_list.as_CFType()),
    ]);
    let mut device_id: AudioObjectID = 0;
    unsafe {
        let agg = &*(agg.as_concrete_TypeRef() as *const objc2_core_foundation::CFDictionary);
        check(
            AudioHardwareCreateAggregateDevice(agg, NonNull::from(&mut device_id)),
            "create aggregate",
        )?;
    }

    let (tx, rx) = mpsc::channel::<Chunk>();
    let tx = Box::into_raw(Box::new(tx));
    let mut io: AudioDeviceIOProcID = None;
    unsafe {
        check(
            AudioDeviceCreateIOProcID(device_id, Some(io_proc), tx.cast(), NonNull::from(&mut io)),
            "io proc",
        )?;
        check(AudioDeviceStart(device_id, io), "start")?;
    }
    writeln!(meta, "wall_minus_host_ns {}", wall_minus_host())?;
    println!("listening for {secs} s");
    std::thread::sleep(Duration::from_secs(secs));
    writeln!(meta, "wall_minus_host_ns_end {}", wall_minus_host())?;
    unsafe {
        AudioDeviceStop(device_id, io);
        AudioDeviceDestroyIOProcID(device_id, io);
        AudioHardwareDestroyAggregateDevice(device_id);
        AudioHardwareDestroyProcessTap(tap);
        drop(Box::from_raw(tx));
    }
    drop(local);

    let mut capture = std::io::BufWriter::new(std::fs::File::create(format!("{dir}/capture.f32"))?);
    let mut chunks = String::new();
    let mut at = 0usize;
    let mut peak = 0f32;
    for c in rx.try_iter() {
        let frames = c.samples.len() / 2;
        writeln!(chunks, "{} {at} {frames}", c.host_ns)?;
        for s in &c.samples {
            capture.write_all(&s.to_le_bytes())?;
            peak = peak.max(s.abs());
        }
        at += frames;
    }
    capture.flush()?;
    std::fs::write(format!("{dir}/chunks.txt"), chunks)?;
    let local: String = local_rx
        .try_iter()
        .map(|(n, at, l)| format!("{n} {at} {l}\n"))
        .collect();
    std::fs::write(format!("{dir}/local.txt"), local)?;
    std::fs::write(format!("{dir}/meta.txt"), &meta)?;
    print!("{meta}");
    println!("captured {at} frames, peak {peak:.3}");
    Ok(())
}

unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    let tx = unsafe { &*client.cast::<mpsc::Sender<Chunk>>() };
    let list = input.as_ptr();
    let buffers: &[AudioBuffer] = unsafe {
        std::slice::from_raw_parts(
            (&raw const (*list).mBuffers).cast(),
            (*list).mNumberBuffers as usize,
        )
    };
    let Some(b) = buffers.first().filter(|b| !b.mData.is_null()) else {
        return 0;
    };
    let samples =
        unsafe { std::slice::from_raw_parts(b.mData.cast::<f32>(), b.mDataByteSize as usize / 4) };
    // The tap is a stereo mixdown, interleaved.
    let samples = if b.mNumberChannels == 2 {
        samples.to_vec()
    } else {
        samples.iter().flat_map(|s| [*s, *s]).collect()
    };
    let host = host_ns(unsafe { input_time.as_ref().mHostTime });
    let _ = tx.send(Chunk {
        host_ns: host,
        samples,
    });
    0
}

fn process_object(pid: i32) -> Option<AudioObjectID> {
    let o: AudioObjectID = unsafe {
        property(
            kAudioObjectSystemObject as AudioObjectID,
            kAudioHardwarePropertyTranslatePIDToProcessObject,
            kAudioObjectPropertyScopeGlobal,
            Some(pid),
        )
        .ok()?
    };
    (o != 0).then_some(o)
}

/// The default output device's own idea of its latency, in frames.
fn describe_output(meta: &mut String) -> anyhow::Result<()> {
    let out = kAudioObjectPropertyScopeOutput;
    unsafe {
        let device: AudioObjectID = property(
            kAudioObjectSystemObject as AudioObjectID,
            kAudioHardwarePropertyDefaultOutputDevice,
            kAudioObjectPropertyScopeGlobal,
            None,
        )?;
        let latency: u32 = property(device, kAudioDevicePropertyLatency, out, None)?;
        let safety: u32 = property(device, kAudioDevicePropertySafetyOffset, out, None)?;
        let buffer: u32 = property(device, kAudioDevicePropertyBufferFrameSize, out, None)?;
        // The first output stream's latency.
        let stream: AudioObjectID =
            property(device, kAudioDevicePropertyStreams, out, None).unwrap_or(0);
        let stream_latency: u32 = if stream != 0 {
            property(
                stream,
                kAudioStreamPropertyLatency,
                kAudioObjectPropertyScopeGlobal,
                None,
            )
            .unwrap_or(0)
        } else {
            0
        };
        writeln!(
            meta,
            "device_latency_frames {latency} safety_offset_frames {safety} buffer_frames {buffer} stream_latency_frames {stream_latency}"
        )?;
    }
    Ok(())
}

unsafe fn property<T>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    pid: Option<i32>,
) -> anyhow::Result<T> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value = MaybeUninit::<T>::zeroed();
    let mut size = size_of::<T>() as u32;
    let (qs, q) = match &pid {
        Some(pid) => (size_of::<i32>() as u32, ptr::from_ref(pid).cast::<c_void>()),
        None => (0, ptr::null()),
    };
    unsafe {
        check(
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&address),
                qs,
                q,
                NonNull::from(&mut size),
                NonNull::new_unchecked(value.as_mut_ptr().cast()),
            ),
            "property",
        )?;
        Ok(value.assume_init())
    }
}

fn check(status: i32, what: &str) -> anyhow::Result<()> {
    anyhow::ensure!(status == 0, "{what} (error {status})");
    Ok(())
}
