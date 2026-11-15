//! A small software synthesizer: the instruments the score is played on, and a reverb.
//! Everything is computed from scratch, sample by sample, so the music needs no audio files.

use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instr {
    /// A soft piano: decaying partials with a little hammer noise.
    Piano,
    /// A plucked harp string (Karplus-Strong), bright and ringing.
    Harp,
    /// A plucked lute or guitar, warmer and shorter than the harp.
    Lute,
    /// A breathy flute or recorder with delayed vibrato.
    Flute,
    /// A bowed string section: detuned saws through a low-pass filter.
    Strings,
    /// A slow, warm synth pad.
    Pad,
    /// A round electric bass.
    Bass,
    /// A plucked upright-style bass.
    PluckBass,
    /// A chiptune square wave lead.
    Square,
    /// A thin chiptune pulse, for fast arpeggios.
    Pulse,
    /// A stepped chiptune triangle, for bass.
    Triangle,
    /// An FM bell or celesta.
    Bell,
    /// A sequencer pluck: a filtered saw with a fast decay.
    Seq,
    Kick,
    Snare,
    Hat,
    OpenHat,
    Tom,
    Shaker,
    /// A short tick, like a key on a mechanical keyboard.
    Click,
    /// A chiptune noise snare.
    NoiseSnare,
}

impl Instr {
    /// How much of the voice goes to the reverb.
    pub fn send(self) -> f32 {
        match self {
            Instr::Piano | Instr::Harp | Instr::Bell => 0.45,
            Instr::Flute | Instr::Strings | Instr::Pad => 0.5,
            Instr::Lute | Instr::Seq => 0.3,
            Instr::Bass | Instr::PluckBass | Instr::Triangle | Instr::Kick => 0.05,
            Instr::Square | Instr::Pulse => 0.2,
            _ => 0.15,
        }
    }

    #[cfg(test)]
    pub fn is_drum(self) -> bool {
        matches!(
            self,
            Instr::Kick
                | Instr::Snare
                | Instr::Hat
                | Instr::OpenHat
                | Instr::Tom
                | Instr::Shaker
                | Instr::Click
                | Instr::NoiseSnare
        )
    }
}

pub fn midi_hz(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

/// A playing note.
pub struct Voice {
    pub instr: Instr,
    freq: f32,
    vel: f32,
    /// Seconds the key is held.
    gate: f32,
    sr: f32,
    /// Samples since the note started.
    n: u32,
    phase: [f32; 4],
    lp: [f32; 2],
    hp: f32,
    rng: u32,
    ks: Vec<f32>,
    ks_pos: usize,
    /// Allpass coefficient that tunes the plucked string between whole samples.
    ks_tune: f32,
    /// Set by the instrument when it has rung out; the voice then fades for a few ms.
    ended: bool,
    fade_left: u32,
    pub done: bool,
}

/// Samples a finished voice takes to fade, so cutting it off never clicks.
const END_FADE: u32 = 256;

/// The fast PolyBLEP correction that keeps saws and squares from aliasing.
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

fn saw(phase: f32, dt: f32) -> f32 {
    2.0 * phase - 1.0 - blep(phase, dt)
}

fn pulse(phase: f32, dt: f32, duty: f32) -> f32 {
    let mut v = if phase < duty { 1.0 } else { -1.0 };
    v += blep(phase, dt);
    v -= blep((phase + 1.0 - duty) % 1.0, dt);
    v
}

/// Attack-decay-sustain-release level at `t` seconds for a note held `gate` seconds.
fn adsr(t: f32, gate: f32, a: f32, d: f32, s: f32, r: f32) -> f32 {
    let held = |t: f32| {
        if t < a {
            t / a
        } else {
            s + (1.0 - s) * (-(t - a) / d).exp()
        }
    };
    if t < gate {
        held(t)
    } else {
        held(gate) * (-(t - gate) / r).exp()
    }
}

impl Voice {
    pub fn new(instr: Instr, midi: f32, vel: f32, gate: f32, sr: f32, seed: u32) -> Self {
        let freq = midi_hz(midi);
        let mut v = Voice {
            instr,
            freq,
            vel,
            gate: gate.max(0.01),
            sr,
            n: 0,
            phase: [0.0; 4],
            lp: [0.0; 2],
            hp: 0.0,
            rng: seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453) | 1,
            ks: vec![],
            ks_pos: 0,
            ks_tune: 0.0,
            ended: false,
            fade_left: END_FADE,
            done: false,
        };
        if matches!(instr, Instr::Harp | Instr::Lute | Instr::PluckBass) {
            // Karplus-Strong: a delay line one period long, filled with a filtered noise burst.
            // Each new sample averages the two oldest in the line, half a sample less than its
            // length; a first-order allpass adds the fraction that makes up one whole period.
            let want = (sr / freq + 0.5).max(2.0);
            let mut len = want.floor();
            let mut frac = want - len;
            if frac < 0.1 && len > 2.0 {
                len -= 1.0;
                frac += 1.0;
            }
            v.ks_tune = (1.0 - frac) / (1.0 + frac);
            let len = len as usize;
            let bright = match instr {
                Instr::Harp => 0.65,
                Instr::Lute => 0.45,
                _ => 0.25,
            };
            let mut prev = 0.0;
            v.ks = (0..len)
                .map(|_| {
                    let x = v.noise();
                    prev += (x - prev) * bright;
                    prev
                })
                .collect();
            // Remove the burst's DC offset so the string rings around zero.
            let mean = v.ks.iter().sum::<f32>() / len as f32;
            for s in &mut v.ks {
                *s -= mean;
            }
        }
        // Stagger detuned oscillators so chords don't start phase-aligned.
        for k in 0..4 {
            v.phase[k] = (v.noise() * 0.5 + 0.5).fract();
        }
        v
    }

    fn noise(&mut self) -> f32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn osc(&mut self, k: usize, hz: f32) -> f32 {
        let dt = hz / self.sr;
        let p = self.phase[k];
        self.phase[k] = (p + dt).fract();
        p
    }

    fn lowpass(&mut self, k: usize, x: f32, hz: f32) -> f32 {
        let a = 1.0 - (-TAU * hz / self.sr).exp();
        self.lp[k] += (x - self.lp[k]) * a;
        self.lp[k]
    }

    /// The next mono sample.
    pub fn next(&mut self) -> f32 {
        if self.done {
            return 0.0;
        }
        let x = self.sound();
        if !self.ended {
            return x;
        }
        self.fade_left = self.fade_left.saturating_sub(1);
        if self.fade_left == 0 {
            self.done = true;
        }
        x * self.fade_left as f32 / END_FADE as f32
    }

    fn sound(&mut self) -> f32 {
        let t = self.n as f32 / self.sr;
        self.n += 1;
        let f = self.freq;
        let g = self.gate;
        let out = match self.instr {
            Instr::Piano => {
                let mut s = 0.0;
                let decay = 0.5 + f / 500.0;
                for k in 1..=6 {
                    let kf = k as f32;
                    let hz = f * kf * (1.0 + 0.0004 * kf * kf).sqrt();
                    if hz > self.sr * 0.45 {
                        break;
                    }
                    let amp = (-t * decay * (1.0 + 0.7 * (kf - 1.0))).exp() / kf.powf(1.4);
                    s += (TAU * hz * t).sin() * amp;
                }
                let hammer = if t < 0.01 {
                    self.noise() * (1.0 - t / 0.01) * 0.15
                } else {
                    0.0
                };
                let env = if t < 0.004 { t / 0.004 } else { 1.0 };
                let damp = if t > g { (-(t - g) / 0.25).exp() } else { 1.0 };
                if t > g + 1.5 || t > 9.0 {
                    self.ended = true;
                }
                (s * 0.55 + hammer) * env * damp
            }
            Instr::Harp | Instr::Lute | Instr::PluckBass => {
                let len = self.ks.len();
                let i = self.ks_pos;
                let j = (i + 1) % len;
                let out = self.ks[i];
                let (keep, after): (f32, f32) = match self.instr {
                    Instr::Harp => (0.9985, 0.6),
                    Instr::Lute => (0.996, 0.12),
                    _ => (0.997, 0.08),
                };
                let damp = if t > g {
                    (-(t - g) / after).exp().max(0.0)
                } else {
                    1.0
                };
                // Higher strings lose energy faster, as real ones do.
                let keep = keep.powf(1.0 + f / 1500.0) * (0.98 + 0.02 * damp);
                let avg = (self.ks[i] + self.ks[j]) * 0.5 * keep;
                // Fractional-delay allpass; lp[0] and lp[1] hold its last input and output.
                let tuned = self.ks_tune * avg + self.lp[0] - self.ks_tune * self.lp[1];
                self.lp[0] = avg;
                self.lp[1] = tuned;
                self.ks[i] = tuned;
                self.ks_pos = j;
                if t > g + 4.0 * after + 0.05 || t > 8.0 {
                    self.ended = true;
                }
                let body = if self.instr == Instr::PluckBass {
                    1.4
                } else {
                    1.0
                };
                out * body * damp.sqrt()
            }
            Instr::Flute => {
                let vib_amt = ((t - 0.25) / 0.4).clamp(0.0, 1.0) * 0.006;
                let hz = f * (1.0 + vib_amt * (TAU * 5.2 * t).sin());
                let p = self.osc(0, hz);
                let tone =
                    (TAU * p).sin() + 0.18 * (2.0 * TAU * p).sin() + 0.06 * (3.0 * TAU * p).sin();
                let n = self.noise();
                let breath = self.lowpass(0, n, 2500.0) * 0.25;
                let env = adsr(t, g, 0.06, 0.3, 0.85, 0.12);
                if t > g + 0.8 {
                    self.ended = true;
                }
                (tone + breath) * env * 0.6
            }
            Instr::Strings => {
                let vib = 1.0 + 0.003 * (TAU * 5.0 * t + self.phase[3]).sin();
                let mut s = 0.0;
                for (k, det) in [0.9975f32, 1.0, 1.0025].into_iter().enumerate() {
                    let hz = f * det * vib;
                    let p = self.osc(k, hz);
                    s += saw(p, hz / self.sr);
                }
                let cut = (f * 5.0).min(3200.0);
                let s = self.lowpass(0, s / 3.0, cut);
                let s = self.lowpass(1, s, cut * 1.5);
                // Short notes speak quickly; long ones swell in.
                let env = adsr(t, g, (g * 0.25).clamp(0.03, 0.35), 1.0, 1.0, 0.6);
                if t > g + 3.5 {
                    self.ended = true;
                }
                s * env * 0.9
            }
            Instr::Pad => {
                let mut s = 0.0;
                for (k, det) in [0.996f32, 1.004].into_iter().enumerate() {
                    let p = self.osc(k, f * det);
                    s += 1.0 - 4.0 * (p - 0.5).abs();
                }
                let p = self.osc(2, f * 0.5);
                s = s * 0.5 + 0.35 * (TAU * p).sin();
                let s = self.lowpass(0, s, 1400.0);
                let env = adsr(t, g, 1.2, 2.0, 1.0, 1.5);
                if t > g + 8.0 {
                    self.ended = true;
                }
                s * env * 0.8
            }
            Instr::Bass => {
                let p = self.osc(0, f);
                let s = (TAU * p).sin()
                    + 0.35 * (1.0 - 4.0 * (p - 0.5).abs())
                    + 0.15 * (2.0 * TAU * p).sin();
                let s = self.lowpass(0, s, 900.0);
                let env = adsr(t, g, 0.008, 0.5, 0.7, 0.08);
                if t > g + 0.6 {
                    self.ended = true;
                }
                s * env * 0.8
            }
            Instr::Square | Instr::Pulse => {
                let duty = if self.instr == Instr::Square {
                    0.25
                } else {
                    0.125
                };
                let vib = if self.instr == Instr::Square {
                    1.0 + ((t - 0.3) / 0.3).clamp(0.0, 1.0) * 0.008 * (TAU * 6.0 * t).sin()
                } else {
                    1.0
                };
                let hz = f * vib;
                let p = self.osc(0, hz);
                let s = pulse(p, hz / self.sr, duty);
                let s = self.lowpass(0, s, 7000.0);
                let env = if self.instr == Instr::Square {
                    adsr(t, g, 0.004, 0.2, 0.6, 0.05)
                } else {
                    adsr(t, g, 0.002, 0.08, 0.3, 0.03)
                };
                if t > g + 0.4 {
                    self.ended = true;
                }
                s * env * 0.32
            }
            Instr::Triangle => {
                let p = self.osc(0, f);
                // Stepped in 16 levels, like an old console's triangle channel.
                let tri = 1.0 - 4.0 * (p - 0.5).abs();
                let s = (tri * 7.5).round() / 7.5;
                let env = adsr(t, g, 0.002, 1.0, 1.0, 0.02);
                if t > g + 0.2 {
                    self.ended = true;
                }
                s * env * 0.75
            }
            Instr::Bell => {
                let (ratio, index, idecay, adecay) = (3.5, 2.2, 2.5, 1.1);
                let m = (TAU * f * ratio * t).sin() * index * (-t * idecay).exp();
                let s = (TAU * f * t + m).sin();
                let env = if t < 0.002 { t / 0.002 } else { 1.0 } * (-t * adecay).exp();
                let damp = if t > g { (-(t - g) / 0.6).exp() } else { 1.0 };
                if t > 0.01 && env * damp < 0.0005 {
                    self.ended = true;
                }
                s * env * damp * 0.55
            }
            Instr::Seq => {
                let p = self.osc(0, f);
                let s = saw(p, f / self.sr) * 0.6
                    + pulse(self.osc(1, f * 1.003), f / self.sr, 0.5) * 0.4;
                let cut = 300.0 + 3500.0 * (-t * 14.0).exp();
                let s = self.lowpass(0, s, cut);
                let s = self.lowpass(1, s, cut);
                let env = adsr(t, g, 0.003, 0.18, 0.15, 0.06);
                if t > g + 0.4 {
                    self.ended = true;
                }
                s * env * 0.7
            }
            Instr::Kick => {
                let hz = 42.0 + 110.0 * (-t * 32.0).exp();
                let p = self.osc(0, hz);
                let click = if t < 0.003 { self.noise() * 0.3 } else { 0.0 };
                if t > 0.6 {
                    self.ended = true;
                }
                ((TAU * p).sin() * (-t * 7.0).exp() + click) * 0.95
            }
            Instr::Snare | Instr::NoiseSnare => {
                let n = self.noise();
                let x = if self.instr == Instr::NoiseSnare {
                    // Coarse, sample-and-hold noise for an 8-bit feel.
                    if self.n % 6 == 1 {
                        self.hp = n;
                    }
                    self.hp
                } else {
                    let low = self.lowpass(0, n, 1200.0);
                    n - low
                };
                let p = self.osc(0, 185.0);
                let tone = (TAU * p).sin() * (-t * 30.0).exp() * 0.5;
                if t > 0.35 {
                    self.ended = true;
                }
                (x * (-t * 14.0).exp() + tone) * 1.0
            }
            Instr::Hat | Instr::OpenHat => {
                let n = self.noise();
                let low = self.lowpass(0, n, 6000.0);
                let x = n - low;
                let decay = if self.instr == Instr::Hat { 45.0 } else { 9.0 };
                if t > 5.0 / decay + 0.02 {
                    self.ended = true;
                }
                x * (-t * decay).exp() * 0.85
            }
            Instr::Tom => {
                let hz = f * (1.0 + 0.6 * (-t * 25.0).exp());
                let p = self.osc(0, hz);
                let n = self.noise();
                let thump = self.lowpass(0, n, 600.0) * (-t * 30.0).exp();
                if t > 0.8 {
                    self.ended = true;
                }
                ((TAU * p).sin() * (-t * 6.0).exp() + thump * 0.8) * 0.8
            }
            Instr::Shaker => {
                let n = self.noise();
                let low = self.lowpass(0, n, 3500.0);
                let band = self.lowpass(1, n - low, 9000.0);
                let env = if t < 0.012 {
                    t / 0.012
                } else {
                    (-(t - 0.012) * 28.0).exp()
                };
                if t > 0.25 {
                    self.ended = true;
                }
                band * env * 1.0
            }
            Instr::Click => {
                let p = self.osc(0, f);
                if t > 0.03 {
                    self.ended = true;
                }
                (if p < 0.5 { 1.0 } else { -1.0 }) * (-t * 160.0).exp() * 0.25
            }
        };
        out * self.vel
    }
}

/// A plain Schroeder-Moorer reverb (the classic "Freeverb" layout): eight damped combs and four
/// all-passes per side, the right side slightly detuned for width.
pub struct Reverb {
    combs: [Vec<Comb>; 2],
    alls: [Vec<AllPass>; 2],
}

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

struct AllPass {
    buf: Vec<f32>,
    pos: usize,
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLS: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

impl Reverb {
    pub fn new(sr: f32) -> Self {
        let scale = sr / 44_100.0;
        let len = |n: usize| ((n as f32 * scale) as usize).max(1);
        let side = |extra: usize| {
            (
                COMBS
                    .iter()
                    .map(|&n| Comb {
                        buf: vec![0.0; len(n + extra)],
                        pos: 0,
                        store: 0.0,
                    })
                    .collect::<Vec<_>>(),
                ALLS.iter()
                    .map(|&n| AllPass {
                        buf: vec![0.0; len(n + extra)],
                        pos: 0,
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let (cl, al) = side(0);
        let (cr, ar) = side(SPREAD);
        Reverb {
            combs: [cl, cr],
            alls: [al, ar],
        }
    }

    /// Feeds one mono sample in; returns the stereo wet signal.
    pub fn process(&mut self, x: f32, room: f32, damp: f32) -> (f32, f32) {
        let mut out = [0.0f32; 2];
        for (side, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for c in &mut self.combs[side] {
                let y = c.buf[c.pos];
                c.store = y * (1.0 - damp) + c.store * damp;
                c.buf[c.pos] = x + c.store * room;
                c.pos = (c.pos + 1) % c.buf.len();
                acc += y;
            }
            for a in &mut self.alls[side] {
                let b = a.buf[a.pos];
                a.buf[a.pos] = acc + b * 0.5;
                a.pos = (a.pos + 1) % a.buf.len();
                acc = b - acc;
            }
            *o = acc;
        }
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_sounds_and_stops() {
        let all = [
            Instr::Piano,
            Instr::Harp,
            Instr::Lute,
            Instr::Flute,
            Instr::Strings,
            Instr::Pad,
            Instr::Bass,
            Instr::PluckBass,
            Instr::Square,
            Instr::Pulse,
            Instr::Triangle,
            Instr::Bell,
            Instr::Seq,
            Instr::Kick,
            Instr::Snare,
            Instr::Hat,
            Instr::OpenHat,
            Instr::Tom,
            Instr::Shaker,
            Instr::Click,
            Instr::NoiseSnare,
        ];
        let sr = 22_050.0;
        for i in all {
            let mut v = Voice::new(i, 60.0, 1.0, 0.3, sr, 7);
            let mut peak = 0.0f32;
            let mut n = 0;
            while !v.done && n < sr as usize * 12 {
                let s = v.next();
                assert!(s.is_finite(), "{i:?}");
                peak = peak.max(s.abs());
                n += 1;
            }
            assert!(v.done, "{i:?} never stops");
            if !i.is_drum() {
                assert!(n as f32 > sr * 0.25, "{i:?} stops after {n} samples");
            }
            assert!(peak > 0.05, "{i:?} is silent ({peak})");
            assert!(peak < 1.6, "{i:?} is too loud ({peak})");
        }
    }

    /// The pitch of a steady tone, from the strongest autocorrelation near the expected period.
    fn measured_hz(x: &[f32], sr: f32, expect: f32) -> f32 {
        let p = sr / expect;
        let corr = |lag: usize| -> f32 { x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum() };
        let lo = (p * 0.9) as usize;
        let hi = (p * 1.1) as usize + 1;
        let best = (lo..=hi)
            .max_by(|&a, &b| corr(a).total_cmp(&corr(b)))
            .unwrap();
        // Parabolic interpolation between neighbouring lags.
        let (a, b, c) = (corr(best - 1), corr(best), corr(best + 1));
        let shift = 0.5 * (a - c) / (a - 2.0 * b + c);
        sr / (best as f32 + shift)
    }

    #[test]
    fn plucked_strings_are_in_tune() {
        let sr = 44_100.0;
        for (instr, midi) in [
            (Instr::Harp, 69.0),
            (Instr::Harp, 84.0),
            (Instr::Lute, 76.0),
            (Instr::PluckBass, 40.0),
        ] {
            let mut v = Voice::new(instr, midi, 1.0, 2.0, sr, 3);
            let x: Vec<f32> = (0..(sr as usize / 2))
                .map(|_| v.next())
                .skip(2000)
                .collect();
            let want = midi_hz(midi);
            let got = measured_hz(&x, sr, want);
            let cents = 1200.0 * (got / want).log2();
            assert!(
                cents.abs() < 5.0,
                "{instr:?} {midi}: {got:.1} Hz, {cents:.1} cents off"
            );
        }
    }
}
