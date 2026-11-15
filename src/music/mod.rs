//! Background music: the score's tracks, played live through the synthesizer on an audio thread.
//!
//! Nothing here touches the terminal. Without a sound device the music is simply silent.

mod output;
pub mod score;
pub mod synth;

use score::{Note, Track};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize, Ordering};
use synth::{Reverb, Voice};

/// Silence between tracks, in seconds.
const GAP_SECS: f32 = 6.0;
/// The most notes sounding at once. Beyond this the oldest is dropped.
const MAX_VOICES: usize = 72;
/// How long a fade in or out takes, in seconds.
const FADE_SECS: f32 = 0.6;
const NOT_PLAYING: usize = usize::MAX;

/// Whether the audio thread found somewhere to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Music is off, or the device is still opening.
    Idle,
    Playing,
    /// No sound device could be opened.
    NoDevice,
}

/// What the game and the audio thread share.
struct Control {
    on: AtomicBool,
    /// 0 to 100.
    volume: AtomicU32,
    /// The track playing, or `NOT_PLAYING` between tracks.
    playing: AtomicUsize,
    status: AtomicU8,
}

impl Control {
    fn set_status(&self, s: Status) {
        let v = match s {
            Status::Idle => 0,
            Status::Playing => 1,
            Status::NoDevice => 2,
        };
        self.status.store(v, Ordering::Relaxed);
    }

    fn status(&self) -> Status {
        match self.status.load(Ordering::Relaxed) {
            1 => Status::Playing,
            2 => Status::NoDevice,
            _ => Status::Idle,
        }
    }
}

/// A sounding note, with its pan already worked out.
struct Sounding {
    voice: Voice,
    left: f32,
    right: f32,
    send: f32,
}

/// Mixes sounding notes and the reverb into stereo samples.
struct Mixer {
    sr: f32,
    voices: Vec<Sounding>,
    reverb: Reverb,
    room: f32,
    damp: f32,
    seed: u32,
}

impl Mixer {
    fn new(sr: f32) -> Self {
        Mixer {
            sr,
            voices: Vec::with_capacity(MAX_VOICES + 1),
            reverb: Reverb::new(sr),
            room: 0.8,
            damp: 0.4,
            seed: 1,
        }
    }

    fn start(&mut self, n: &Note, gain: f32) {
        self.seed = self.seed.wrapping_add(0x9E37_79B9);
        let voice = Voice::new(n.instr, n.midi, n.vel * gain, n.dur, self.sr, self.seed);
        // Equal-power panning.
        let angle = (n.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Sounding {
            voice,
            left: angle.cos(),
            right: angle.sin(),
            send: n.instr.send(),
        });
    }

    fn frame(&mut self) -> (f32, f32) {
        let (mut l, mut r, mut wet) = (0.0, 0.0, 0.0);
        let mut finished = false;
        for s in &mut self.voices {
            let x = s.voice.next();
            l += x * s.left;
            r += x * s.right;
            wet += x * s.send;
            finished |= s.voice.done;
        }
        if finished {
            self.voices.retain(|s| !s.voice.done);
        }
        let (wl, wr) = self.reverb.process(wet * 0.015, self.room, self.damp);
        (l + wl, r + wr)
    }
}

/// Keeps peaks under 1 without touching quieter material.
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.85;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh())
    }
}

/// Plays the tracks in order, forever, following the shared on/off switch and volume.
struct Player {
    tracks: Vec<Track>,
    ctl: Arc<Control>,
    mixer: Mixer,
    track: usize,
    /// The next note of the track to start.
    next: usize,
    /// Samples since the track began.
    pos: u64,
    /// Samples of silence left before the track begins.
    gap: u64,
    level: f32,
}

impl Player {
    fn new(tracks: Vec<Track>, sr: f32, ctl: Arc<Control>, first: usize) -> Self {
        let mut p = Player {
            track: first % tracks.len(),
            tracks,
            ctl,
            mixer: Mixer::new(sr),
            next: 0,
            pos: 0,
            gap: (sr * 0.5) as u64,
            level: 0.0,
        };
        p.mixer.room = p.tracks[p.track].room;
        p.mixer.damp = p.tracks[p.track].damp;
        p
    }

    /// Switched off and fully faded out: the device can rest until the music comes back on.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn silent(&self) -> bool {
        self.level == 0.0 && !self.ctl.on.load(Ordering::Relaxed)
    }

    /// Fills interleaved stereo samples.
    fn render(&mut self, out: &mut [f32]) {
        let sr = self.mixer.sr;
        let on = self.ctl.on.load(Ordering::Relaxed);
        let target = if on {
            let v = self.ctl.volume.load(Ordering::Relaxed).min(100) as f32 / 100.0;
            // Loudness is heard roughly logarithmically; squaring makes the steps feel even.
            v * v
        } else {
            0.0
        };
        let ease = 1.0 - (-5.0 / (FADE_SECS * sr)).exp();
        for frame in out.chunks_mut(2) {
            self.level += (target - self.level) * ease;
            if !on && self.level < 1e-3 {
                // Faded out: hold still, so the music picks up where it stopped.
                self.level = 0.0;
                frame.fill(0.0);
                continue;
            }
            self.advance();
            let (l, r) = self.mixer.frame();
            frame[0] = soft_clip(l * self.level);
            if let Some(x) = frame.get_mut(1) {
                *x = soft_clip(r * self.level);
            }
        }
    }

    fn advance(&mut self) {
        if self.gap > 0 {
            self.gap -= 1;
            if self.gap == 0 {
                let t = &self.tracks[self.track];
                self.mixer.room = t.room;
                self.mixer.damp = t.damp;
                self.ctl.playing.store(self.track, Ordering::Relaxed);
            }
            return;
        }
        let track = &self.tracks[self.track];
        let t = self.pos as f32 / self.mixer.sr;
        while let Some(n) = track.notes.get(self.next) {
            if n.start > t {
                break;
            }
            self.mixer.start(n, track.gain);
            self.next += 1;
        }
        self.pos += 1;
        if t >= track.secs {
            self.track = (self.track + 1) % self.tracks.len();
            self.next = 0;
            self.pos = 0;
            self.gap = (GAP_SECS * self.mixer.sr) as u64;
            self.ctl.playing.store(NOT_PLAYING, Ordering::Relaxed);
        }
    }
}

/// The game's handle on the music.
pub struct Music {
    ctl: Arc<Control>,
    /// Whether this handle may open a sound device (off in tests and with `--no-music`).
    live: bool,
    started: bool,
    /// The track last reported by `take_now_playing`.
    reported: usize,
    titles: Vec<&'static str>,
}

impl Default for Music {
    /// Music that never makes a sound: the switch and volume work, but no device is opened.
    fn default() -> Self {
        Music {
            ctl: Arc::new(Control {
                on: AtomicBool::new(false),
                volume: AtomicU32::new(70),
                playing: AtomicUsize::new(NOT_PLAYING),
                status: AtomicU8::new(0),
            }),
            live: false,
            started: false,
            reported: NOT_PLAYING,
            titles: TITLES.to_vec(),
        }
    }
}

/// Track titles in playing order, without building the score.
pub const TITLES: [&str; 6] = [
    "Morning Shift",
    "Ore and Ember",
    "Conveyor Waltz",
    "Night Grid",
    "Kingdom of Cogs",
    "Terminal Dreams",
];

impl Music {
    /// Music that plays through the sound device once switched on.
    pub fn live() -> Self {
        Music {
            live: true,
            ..Music::default()
        }
    }

    /// A handle that acts live (switching on is remembered and reported) but never opens a
    /// device, for tests.
    #[cfg(test)]
    pub fn live_without_sound() -> Self {
        Music {
            live: true,
            started: true,
            ..Music::default()
        }
    }

    /// Whether switching on can make a sound at all in this session.
    pub fn is_live(&self) -> bool {
        self.live
    }

    pub fn on(&self) -> bool {
        self.ctl.on.load(Ordering::Relaxed)
    }

    pub fn set_on(&mut self, on: bool) {
        self.ctl.on.store(on, Ordering::Relaxed);
        if on && self.live && !self.started {
            self.started = true;
            self.spawn();
        }
    }

    pub fn volume(&self) -> u32 {
        self.ctl.volume.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, v: u32) {
        self.ctl.volume.store(v.min(100), Ordering::Relaxed);
    }

    pub fn status(&self) -> Status {
        self.ctl.status()
    }

    /// The title of the track playing now, if any.
    pub fn now_playing(&self) -> Option<&'static str> {
        if !self.on() {
            return None;
        }
        self.titles
            .get(self.ctl.playing.load(Ordering::Relaxed))
            .copied()
    }

    /// The title of a track that started since the last call.
    pub fn take_now_playing(&mut self) -> Option<&'static str> {
        let now = self.ctl.playing.load(Ordering::Relaxed);
        if now == self.reported {
            return None;
        }
        self.reported = now;
        if self.on() {
            self.titles.get(now).copied()
        } else {
            None
        }
    }

    fn spawn(&self) {
        let ctl = Arc::clone(&self.ctl);
        let first = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(0);
        let spawned = std::thread::Builder::new()
            .name("music".into())
            .spawn(move || {
                // Composing the score takes a moment; do it here, off the game's thread.
                let tracks = score::tracks();
                let first = first % tracks.len();
                let make = |sr: u32| Player::new(tracks, sr as f32, Arc::clone(&ctl), first);
                if output::run(make, &ctl).is_err() {
                    ctl.set_status(Status::NoDevice);
                }
            });
        if spawned.is_err() {
            self.ctl.set_status(Status::NoDevice);
        }
    }
}

/// Renders one track on its own, with its reverb tail.
fn render_track(t: &Track, sr: u32) -> Vec<f32> {
    let mut m = Mixer::new(sr as f32);
    m.room = t.room;
    m.damp = t.damp;
    let frames = (t.secs * sr as f32) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    let mut next = 0;
    for i in 0..frames {
        let now = i as f32 / sr as f32;
        while let Some(n) = t.notes.get(next) {
            if n.start > now {
                break;
            }
            m.start(n, t.gain);
            next += 1;
        }
        let (l, r) = m.frame();
        out.push(soft_clip(l));
        out.push(soft_clip(r));
    }
    // Fade the last half second so the file ends in silence.
    let tail = (sr as usize / 2).min(frames);
    for k in 0..tail {
        let g = k as f32 / tail as f32;
        let i = (frames - 1 - k) * 2;
        out[i] *= g;
        out[i + 1] *= g;
    }
    out
}

fn write_wav(path: &Path, samples: &[f32], sr: u32) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&sr.to_le_bytes());
    b.extend_from_slice(&(sr * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, b)
}

/// One exported track.
pub struct Exported {
    pub title: &'static str,
    pub path: PathBuf,
    pub secs: f32,
    pub peak: f32,
    /// Root-mean-square level, for matching loudness between tracks.
    pub rms: f32,
}

/// Writes every track to `dir` as a 44.1 kHz, 16-bit stereo WAV file.
pub fn export(dir: &Path) -> std::io::Result<Vec<Exported>> {
    const SR: u32 = 44_100;
    std::fs::create_dir_all(dir)?;
    let mut done = vec![];
    for (i, t) in score::tracks().iter().enumerate() {
        let samples = render_track(t, SR);
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
        let name = t.title.to_ascii_lowercase().replace(' ', "-");
        let path = dir.join(format!("{:02}-{name}.wav", i + 1));
        write_wav(&path, &samples, SR)?;
        done.push(Exported {
            title: t.title,
            path,
            secs: t.secs,
            peak,
            rms,
        });
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short_track() -> Track {
        let mut notes = vec![];
        for k in 0..8 {
            notes.push(Note {
                start: k as f32 * 0.1,
                dur: 0.1,
                midi: 60.0 + k as f32,
                vel: 0.5,
                instr: synth::Instr::Harp,
                pan: 0.0,
            });
        }
        Track {
            title: "Test",
            notes,
            secs: 1.0,
            room: 0.7,
            damp: 0.4,
            gain: 1.0,
        }
    }

    fn control(on: bool) -> Arc<Control> {
        Arc::new(Control {
            on: AtomicBool::new(on),
            volume: AtomicU32::new(100),
            playing: AtomicUsize::new(NOT_PLAYING),
            status: AtomicU8::new(0),
        })
    }

    #[test]
    fn the_player_plays_fades_and_moves_to_the_next_track() {
        let sr = 8_000;
        let ctl = control(true);
        let mut p = Player::new(
            vec![short_track(), short_track()],
            sr as f32,
            Arc::clone(&ctl),
            0,
        );
        let mut buf = vec![0.0; sr * 2];
        p.render(&mut buf);
        assert_eq!(ctl.playing.load(Ordering::Relaxed), 0);
        assert!(
            buf.iter().any(|s| s.abs() > 0.01),
            "the first second is silent"
        );
        assert!(buf.iter().all(|s| s.abs() <= 1.0));
        // The track ends at 1.5 s, then a 6 s gap, then the second track.
        p.render(&mut buf);
        assert_eq!(ctl.playing.load(Ordering::Relaxed), NOT_PLAYING);
        for _ in 0..6 {
            p.render(&mut buf);
        }
        assert_eq!(ctl.playing.load(Ordering::Relaxed), 1);

        // Switched off, it fades to silence and stops moving.
        ctl.on.store(false, Ordering::Relaxed);
        p.render(&mut buf);
        p.render(&mut buf);
        let (pos, next) = (p.pos, p.next);
        p.render(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0));
        assert_eq!((p.pos, p.next), (pos, next));
        assert!(p.silent());
        ctl.on.store(true, Ordering::Relaxed);
        assert!(!p.silent());
    }

    #[test]
    fn a_handle_without_a_device_reports_tracks_only_when_on() {
        let mut m = Music::default();
        assert!(!m.is_live());
        m.set_on(true);
        assert_eq!(m.status(), Status::Idle);
        m.ctl.playing.store(2, Ordering::Relaxed);
        assert_eq!(m.take_now_playing(), Some("Conveyor Waltz"));
        assert_eq!(m.take_now_playing(), None);
        assert_eq!(m.now_playing(), Some("Conveyor Waltz"));
        m.set_on(false);
        assert_eq!(m.now_playing(), None);
        m.set_volume(250);
        assert_eq!(m.volume(), 100);
    }

    #[test]
    fn titles_match_the_score() {
        let titles: Vec<&str> = score::tracks().iter().map(|t| t.title).collect();
        assert_eq!(titles, TITLES);
    }

    #[test]
    fn wav_files_have_a_valid_header() {
        let dir = std::env::temp_dir().join(format!("factuio-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        let samples = render_track(&short_track(), 8_000);
        write_wav(&path, &samples, 8_000).unwrap();
        let b = std::fs::read(&path).unwrap();
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(b.len(), 44 + samples.len() * 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
