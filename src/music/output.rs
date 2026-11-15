//! Sending samples to the sound device.
//!
//! On Linux, ALSA is loaded when the music starts rather than linked, so the game still runs
//! (silently) on machines without it, such as servers reached over SSH. macOS and Windows use
//! cpal. Elsewhere there is no sound.

use super::{Control, Player};

#[cfg(target_os = "linux")]
pub use alsa::run;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::run;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn run(_make: impl FnOnce(u32) -> Player, _ctl: &Control) -> Result<(), String> {
    Err("no sound output on this platform".into())
}

#[cfg(target_os = "linux")]
mod alsa {
    use super::{Control, Player};
    use crate::music::Status;
    use libloading::Library;
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

    type Pcm = *mut c_void;
    type Open = unsafe extern "C" fn(*mut Pcm, *const c_char, c_int, c_int) -> c_int;
    type SetParams =
        unsafe extern "C" fn(Pcm, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
    type Writei = unsafe extern "C" fn(Pcm, *const c_void, c_ulong) -> c_long;
    type Recover = unsafe extern "C" fn(Pcm, c_int, c_int) -> c_int;
    type Close = unsafe extern "C" fn(Pcm) -> c_int;
    type Drop = unsafe extern "C" fn(Pcm) -> c_int;
    type Prepare = unsafe extern "C" fn(Pcm) -> c_int;
    /// ALSA's handler is variadic; the fixed arguments are passed the same way on Linux, and
    /// this one ignores them all.
    type Handler = unsafe extern "C" fn(*const c_char, c_int, *const c_char, c_int, *const c_char);
    type SetHandler = unsafe extern "C" fn(Option<Handler>) -> c_int;

    const PLAYBACK: c_int = 0;
    const FORMAT_S16_LE: c_int = 2;
    const ACCESS_RW_INTERLEAVED: c_int = 3;
    const RATE: u32 = 44_100;
    /// Frames rendered at a time, and the device buffer in microseconds.
    const CHUNK: usize = 1024;
    const LATENCY_US: c_uint = 150_000;

    /// ALSA prints its complaints (no card, unknown device) to stderr, which would land on top
    /// of the game's screen. This keeps it quiet.
    unsafe extern "C" fn quiet(
        _: *const c_char,
        _: c_int,
        _: *const c_char,
        _: c_int,
        _: *const c_char,
    ) {
    }

    /// Looks up a function in the library.
    unsafe fn sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
        // SAFETY: the caller names the function's real type.
        unsafe { lib.get::<T>(name).map(|s| *s).map_err(|e| e.to_string()) }
    }

    pub fn run(make: impl FnOnce(u32) -> Player, ctl: &Control) -> Result<(), String> {
        // SAFETY: libasound is a plain C library with no load-time side effects we depend on.
        let lib = unsafe { Library::new("libasound.so.2") }.map_err(|e| e.to_string())?;
        // SAFETY: the signatures match ALSA's documented C API, and `lib` outlives every call.
        unsafe {
            let open: Open = sym(&lib, b"snd_pcm_open\0")?;
            let set_params: SetParams = sym(&lib, b"snd_pcm_set_params\0")?;
            let writei: Writei = sym(&lib, b"snd_pcm_writei\0")?;
            let recover: Recover = sym(&lib, b"snd_pcm_recover\0")?;
            let close: Close = sym(&lib, b"snd_pcm_close\0")?;
            let drop_pcm: Drop = sym(&lib, b"snd_pcm_drop\0")?;
            let prepare: Prepare = sym(&lib, b"snd_pcm_prepare\0")?;
            if let Ok(set_handler) = sym::<SetHandler>(&lib, b"snd_lib_error_set_handler\0") {
                set_handler(Some(quiet));
            }

            let mut pcm: Pcm = std::ptr::null_mut();
            if open(&mut pcm, c"default".as_ptr(), PLAYBACK, 0) < 0 || pcm.is_null() {
                return Err("no sound device".into());
            }
            if set_params(
                pcm,
                FORMAT_S16_LE,
                ACCESS_RW_INTERLEAVED,
                2,
                RATE,
                1,
                LATENCY_US,
            ) < 0
            {
                close(pcm);
                return Err("the sound device refused 44.1 kHz stereo".into());
            }
            ctl.set_status(Status::Playing);
            let mut player = make(RATE);
            let mut buf = vec![0.0f32; CHUNK * 2];
            let mut pcm16 = vec![0i16; CHUNK * 2];
            let mut resting = false;
            loop {
                if player.silent() {
                    // Switched off: stop the stream rather than feed it silence.
                    if !resting {
                        drop_pcm(pcm);
                        resting = true;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    if !ctl.on.load(std::sync::atomic::Ordering::Relaxed) {
                        continue;
                    }
                }
                if resting {
                    prepare(pcm);
                    resting = false;
                }
                player.render(&mut buf);
                for (o, s) in pcm16.iter_mut().zip(&buf) {
                    *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                }
                let mut done = 0;
                while done < CHUNK {
                    let rest = &pcm16[done * 2..];
                    let n = writei(pcm, rest.as_ptr().cast(), (CHUNK - done) as c_ulong);
                    if n < 0 {
                        // An underrun or a suspended device: try to carry on.
                        if recover(pcm, n as c_int, 1) < 0 {
                            close(pcm);
                            return Err("lost the sound device".into());
                        }
                    } else {
                        done += n as usize;
                    }
                }
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native {
    use super::{Control, Player};
    use crate::music::Status;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    pub fn run(make: impl FnOnce(u32) -> Player, ctl: &Control) -> Result<(), String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no sound device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let config = supported.config();
        let player = make(config.sample_rate);
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, player),
            SampleFormat::I16 => build::<i16>(&device, config, player),
            SampleFormat::I32 => build::<i32>(&device, config, player),
            SampleFormat::U16 => build::<u16>(&device, config, player),
            other => Err(format!("unsupported sample format {other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        ctl.set_status(Status::Playing);
        // The stream plays for as long as it exists, so keep this thread (and it) alive, and
        // pause it while the music is switched off (after giving it time to fade).
        let mut off_since: Option<Instant> = None;
        let mut paused = false;
        loop {
            std::thread::park_timeout(Duration::from_millis(100));
            let on = ctl.on.load(Ordering::Relaxed);
            if on {
                off_since = None;
                if paused && stream.play().is_ok() {
                    paused = false;
                }
            } else {
                let since = *off_since.get_or_insert_with(Instant::now);
                if !paused && since.elapsed() > Duration::from_secs(2) && stream.pause().is_ok() {
                    paused = true;
                }
            }
        }
    }

    fn build<T>(
        device: &cpal::Device,
        config: StreamConfig,
        mut player: Player,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = config.channels.max(1) as usize;
        let mut stereo: Vec<f32> = vec![];
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    let frames = data.len() / channels;
                    stereo.resize(frames * 2, 0.0);
                    player.render(&mut stereo);
                    for (out, s) in data.chunks_mut(channels).zip(stereo.chunks(2)) {
                        for (c, o) in out.iter_mut().enumerate() {
                            let v = match (channels, c) {
                                (1, _) => (s[0] + s[1]) * 0.5,
                                (_, 0) => s[0],
                                (_, 1) => s[1],
                                _ => 0.0,
                            };
                            *o = T::from_sample(v);
                        }
                    }
                },
                |_err| {},
                None,
            )
            .map_err(|e| e.to_string())
    }
}
