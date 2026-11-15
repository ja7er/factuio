//! The factuio soundtrack: six original pieces written for the game, with the small toolkit
//! used to write them (keys, chords, melodies in scale degrees, accompaniment patterns).
//!
//! Melodies are written as scale degrees: `5:1.5` is the fifth degree held for a beat and a
//! half; `'` and `,` move an octave up or down, `#` and `b` raise or lower a semitone, `r` is a
//! rest, and `|` marks bars for the reader. A length carries over to the next note when left out.

// Each piece lays out its form as a table of sections; the tuples read better there than a
// struct per piece would.
#![allow(clippy::type_complexity)]

use super::synth::Instr;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// Seconds from the start of the track.
    pub start: f32,
    /// Seconds the key is held.
    pub dur: f32,
    pub midi: f32,
    pub vel: f32,
    pub instr: Instr,
    /// -1 (left) to 1 (right).
    pub pan: f32,
}

pub struct Track {
    pub title: &'static str,
    /// Sorted by start time.
    pub notes: Vec<Note>,
    /// Length including the last notes ringing out.
    pub secs: f32,
    /// Reverb room size and damping.
    pub room: f32,
    pub damp: f32,
    /// Overall level, set so every track peaks about the same.
    pub gain: f32,
}

#[derive(Clone, Copy)]
struct Key {
    /// MIDI note of degree 1 at octave 0.
    tonic: i32,
    steps: [i32; 7],
}

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
const DORIAN: [i32; 7] = [0, 2, 3, 5, 7, 9, 10];

impl Key {
    fn pitch(&self, degree: i32, accidental: i32, octave: i32) -> i32 {
        let d = degree - 1;
        self.tonic
            + self.steps[d.rem_euclid(7) as usize]
            + 12 * (d.div_euclid(7) + octave)
            + accidental
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Chord {
    /// Pitch classes, root first.
    tones: Vec<i32>,
    root: i32,
    bass: i32,
}

fn pitch_class(s: &str) -> (i32, &str) {
    let mut chars = s.chars();
    let base = match chars.next() {
        Some('C') => 0,
        Some('D') => 2,
        Some('E') => 4,
        Some('F') => 5,
        Some('G') => 7,
        Some('A') => 9,
        Some('B') => 11,
        _ => panic!("bad chord {s}"),
    };
    let rest = chars.as_str();
    match rest.chars().next() {
        Some('#') => (base + 1, &rest[1..]),
        Some('b') => (base + 11, &rest[1..]),
        _ => (base, rest),
    }
}

/// Parses a chord symbol such as `Am7`, `Bbmaj7`, `C/E` or `Asus4`.
fn chord(sym: &str) -> Chord {
    let (name, bass) = match sym.split_once('/') {
        Some((n, b)) => (n, Some(pitch_class(b).0 % 12)),
        None => (sym, None),
    };
    let (root, quality) = pitch_class(name);
    let root = root % 12;
    let iv: &[i32] = match quality {
        "" => &[0, 4, 7],
        "m" => &[0, 3, 7],
        "7" => &[0, 4, 7, 10],
        "maj7" => &[0, 4, 7, 11],
        "m7" => &[0, 3, 7, 10],
        "maj9" => &[0, 4, 7, 11, 14],
        "m9" => &[0, 3, 7, 10, 14],
        "add9" => &[0, 4, 7, 14],
        "sus2" => &[0, 2, 7],
        "sus4" => &[0, 5, 7],
        "7sus4" => &[0, 5, 7, 10],
        "dim" => &[0, 3, 6],
        "6" => &[0, 4, 7, 9],
        _ => panic!("unknown chord quality in {sym}"),
    };
    Chord {
        tones: iv.iter().map(|i| (root + i) % 12).collect(),
        root,
        bass: bass.unwrap_or(root),
    }
}

/// The pitch with class `pc` in `lo..lo + 12`.
fn place(pc: i32, lo: i32) -> i32 {
    lo + (pc - lo).rem_euclid(12)
}

impl Chord {
    /// Close voicing: every chord tone near `center`, except that a tone a semitone below
    /// another moves up an octave (so a major seventh sits above the root, not under it).
    fn voicing(&self, center: i32) -> Vec<i32> {
        let mut v: Vec<i32> = self.tones.iter().map(|&pc| place(pc, center - 6)).collect();
        v.sort();
        v.dedup();
        for _ in 0..v.len() {
            let Some(k) = v.windows(2).position(|w| w[1] - w[0] == 1) else {
                break;
            };
            v[k] += 12;
            v.sort();
        }
        v
    }

    /// Chord tones climbing from `low`, for arpeggios.
    fn ladder(&self, low: i32, n: usize) -> Vec<i32> {
        (low..low + 48)
            .filter(|p| self.tones.contains(&p.rem_euclid(12)))
            .take(n)
            .collect()
    }
}

/// Where a bass pattern's note falls relative to the chord.
#[derive(Clone, Copy)]
enum B {
    /// The bass note (the slash note, or the root).
    Low,
    Fifth,
    /// The bass note an octave up.
    High,
}

/// Composition state for one track.
struct Comp {
    bpm: f32,
    beats_per_bar: f32,
    notes: Vec<Note>,
    rng: u32,
    /// Largest timing nudge in seconds, and velocity spread, for a played-by-hand feel.
    jitter: f32,
    spread: f32,
}

impl Comp {
    fn new(bpm: f32, beats_per_bar: f32, seed: u32) -> Self {
        Comp {
            bpm,
            beats_per_bar,
            notes: vec![],
            rng: seed | 1,
            jitter: 0.0,
            spread: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f32 / u32::MAX as f32
    }

    fn secs(&self, beat: f32) -> f32 {
        beat * 60.0 / self.bpm
    }

    /// The first beat of bar `bar` (counted from 0).
    fn bar(&self, bar: usize) -> f32 {
        bar as f32 * self.beats_per_bar
    }

    #[allow(clippy::too_many_arguments)]
    fn note(
        &mut self,
        beat: f32,
        beats: f32,
        midi: i32,
        vel: f32,
        instr: Instr,
        pan: f32,
        legato: f32,
    ) {
        let nudge = (self.rand() * 2.0 - 1.0) * self.jitter;
        let v = vel * (1.0 + (self.rand() * 2.0 - 1.0) * self.spread);
        self.notes.push(Note {
            start: (self.secs(beat) + nudge).max(0.0),
            dur: (self.secs(beats) * legato).max(0.02),
            midi: midi as f32,
            vel: v.clamp(0.0, 1.0),
            instr,
            pan,
        });
    }

    /// Plays a melody written in scale degrees from `beat`. `shift` moves every degree, so -2
    /// gives a harmony a third below. Returns the beats it took.
    #[allow(clippy::too_many_arguments)]
    fn melody(
        &mut self,
        beat: f32,
        text: &str,
        key: Key,
        octave: i32,
        shift: i32,
        instr: Instr,
        vel: f32,
        pan: f32,
    ) -> f32 {
        let mut at = beat;
        let mut len = 1.0;
        for tok in text.split_whitespace() {
            if tok == "|" {
                continue;
            }
            let (head, dur) = match tok.split_once(':') {
                Some((h, d)) => (h, Some(d)),
                None => (tok, None),
            };
            if let Some(d) = dur {
                len = match d.split_once('/') {
                    Some((a, b)) => a.parse::<f32>().unwrap() / b.parse::<f32>().unwrap(),
                    None => d.parse().unwrap_or_else(|_| panic!("bad length in {tok}")),
                };
            }
            if head == "r" {
                at += len;
                continue;
            }
            let mut chars = head.chars();
            let degree = chars.next().and_then(|c| c.to_digit(10)).expect("degree") as i32;
            let (mut acc, mut oct) = (0, 0);
            for c in chars {
                match c {
                    '#' => acc += 1,
                    'b' => acc -= 1,
                    '\'' => oct += 1,
                    ',' => oct -= 1,
                    _ => panic!("bad note {tok}"),
                }
            }
            let p = key.pitch(degree + shift, acc, octave + oct);
            // Accent the downbeats a little.
            let accent = if (at % self.beats_per_bar).abs() < 0.01 {
                1.1
            } else {
                1.0
            };
            self.note(at, len, p, vel * accent, instr, pan, 0.95);
            at += len;
        }
        at - beat
    }

    /// A second voice under `lead`'s notes in the bars from `beat`, one chord per bar: under
    /// each note, the highest chord tone at least `below` semitones down.
    #[allow(clippy::too_many_arguments)]
    fn harmonize(
        &mut self,
        beat: f32,
        bars: &[Chord],
        lead: Instr,
        below: i32,
        instr: Instr,
        vel: f32,
        pan: f32,
    ) {
        let from = self.secs(beat);
        let bar_secs = self.secs(self.beats_per_bar);
        let to = from + bar_secs * bars.len() as f32;
        let under: Vec<Note> = self
            .notes
            .iter()
            .filter(|n| n.instr == lead && n.start >= from - 0.05 && n.start < to - 0.05)
            .copied()
            .collect();
        for n in under {
            let bar = (((n.start - from) / bar_secs) + 0.01).floor().max(0.0) as usize;
            let c = &bars[bar.min(bars.len() - 1)];
            let top = n.midi as i32 - below;
            let p = (top - 11..=top)
                .rev()
                .find(|p| c.tones.contains(&p.rem_euclid(12)))
                .unwrap_or(top);
            self.notes.push(Note {
                midi: p as f32,
                vel: n.vel / 0.48 * vel,
                instr,
                pan,
                ..n
            });
        }
    }

    /// The chord held as a block, voiced around `center`.
    #[allow(clippy::too_many_arguments)]
    fn block(
        &mut self,
        beat: f32,
        beats: f32,
        c: &Chord,
        center: i32,
        instr: Instr,
        vel: f32,
        pan: f32,
        legato: f32,
    ) {
        for p in c.voicing(center) {
            self.note(beat, beats, p, vel, instr, pan, legato);
        }
    }

    /// The chord strummed upward, a few milliseconds per string.
    #[allow(clippy::too_many_arguments)]
    fn strum(
        &mut self,
        beat: f32,
        beats: f32,
        c: &Chord,
        center: i32,
        instr: Instr,
        vel: f32,
        pan: f32,
    ) {
        let gap = 0.014 * self.bpm / 60.0;
        for (k, p) in c.voicing(center).into_iter().enumerate() {
            self.note(
                beat + k as f32 * gap,
                beats,
                p,
                vel * (1.0 - 0.06 * k as f32),
                instr,
                pan,
                0.9,
            );
        }
    }

    /// An arpeggio over `beats`: `pattern` indexes the chord's tones climbing from `low`.
    #[allow(clippy::too_many_arguments)]
    fn arp(
        &mut self,
        beat: f32,
        beats: f32,
        c: &Chord,
        low: i32,
        pattern: &[usize],
        step: f32,
        instr: Instr,
        vel: f32,
        pan: f32,
        legato: f32,
    ) {
        let ladder = c.ladder(low, 12);
        let n = (beats / step).round() as usize;
        for k in 0..n {
            let p = ladder[pattern[k % pattern.len()] % ladder.len()];
            let accent = if k % pattern.len() == 0 { 1.15 } else { 1.0 };
            self.note(
                beat + k as f32 * step,
                step,
                p,
                vel * accent,
                instr,
                pan,
                legato,
            );
        }
    }

    /// A bass line: (beat offset, beats, which note).
    #[allow(clippy::too_many_arguments)]
    fn bass(
        &mut self,
        beat: f32,
        c: &Chord,
        pattern: &[(f32, f32, B)],
        low: i32,
        instr: Instr,
        vel: f32,
    ) {
        for &(off, len, which) in pattern {
            let p = match which {
                B::Low => place(c.bass, low),
                B::Fifth => place(c.root, low) + 7,
                B::High => place(c.bass, low) + 12,
            };
            self.note(beat + off, len, p, vel, instr, 0.0, 0.92);
        }
    }

    /// Drum hits from a step pattern: k kick, s snare, n noise snare, h hat, o open hat,
    /// t tom, x shaker, c click; upper case is accented, `.` is a rest.
    fn drums(&mut self, beat: f32, pattern: &str, step: f32, vel: f32) {
        for (k, ch) in pattern.chars().enumerate() {
            let (instr, midi, pan) = match ch.to_ascii_lowercase() {
                'k' => (Instr::Kick, 36, 0.0),
                's' => (Instr::Snare, 38, 0.05),
                'n' => (Instr::NoiseSnare, 38, 0.05),
                'h' => (Instr::Hat, 42, 0.25),
                'o' => (Instr::OpenHat, 46, 0.25),
                't' => (Instr::Tom, 45, -0.1),
                'x' => (Instr::Shaker, 70, -0.3),
                'c' => (Instr::Click, 96, 0.3),
                _ => continue,
            };
            let accent = if ch.is_ascii_uppercase() { 1.3 } else { 1.0 };
            self.note(
                beat + k as f32 * step,
                step,
                midi,
                vel * accent,
                instr,
                pan,
                1.0,
            );
        }
    }

    fn finish(mut self, title: &'static str, room: f32, damp: f32, gain: f32) -> Track {
        self.notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        let end = self
            .notes
            .iter()
            .map(|n| n.start + n.dur)
            .fold(0.0, f32::max);
        Track {
            title,
            notes: self.notes,
            secs: end + 4.0,
            room,
            damp,
            gain,
        }
    }
}

fn chords(syms: &[&str]) -> Vec<Chord> {
    syms.iter().map(|s| chord(s)).collect()
}

/// Pastoral and bright: harp, flute and celesta on a spring morning in F major.
fn morning_shift() -> Track {
    let mut c = Comp::new(96.0, 4.0, 11);
    c.jitter = 0.008;
    c.spread = 0.08;
    let key = Key {
        tonic: 65,
        steps: MAJOR,
    };
    let intro = chords(&["F", "Bbmaj7", "F", "C7"]);
    let a = chords(&["F", "C/E", "Gm7", "Bbmaj7", "F", "Am7", "Gm7", "C7"]);
    let a2 = chords(&["F", "C/E", "Gm7", "Bbmaj7", "Gm7", "C7", "F", "F"]);
    let b = chords(&["Bbadd9", "C", "Am7", "Dm7", "Gm7", "Am7", "Bbadd9", "C7"]);
    let outro = chords(&["F", "Bbmaj7", "Fadd9", "F"]);
    let mel_a = "5:1.5 3:0.5 5:1 1':1 | 7:0.5 1':0.5 6:1 5:2 | 4:1.5 3:0.5 4:1 6:1 | 5:3 r:1 |
                 5:1.5 3:0.5 5:1 1':1 | 3':1 2':0.5 1':0.5 7:1 5:1 | 6:1 5:0.5 4:0.5 3:1 4:1 | 2:1 3:0.5 4:0.5 5:2";
    let mel_a2 = "5:1.5 3:0.5 5:1 1':1 | 7:0.5 1':0.5 6:1 5:2 | 4:1.5 3:0.5 4:1 6:1 | 5:3 r:1 |
                  4:1 6:1 1':1 6:0.5 5:0.5 | 7:1.5 1':0.5 2':1 7:1 | 1':3 r:1 | r:4";
    let mel_b = "6:1 5:1 4:1 6:1 | 5:2 3:1 2:1 | 3:1 5:1 7:1 5:1 | 6:3 r:1 |
                 4:1 2:1 4:1 6:1 | 5:1 3:1 5:1 7:1 | 1':1.5 7:0.5 6:1 4:1 | 5:2 2:1 7,:1";
    // (chords, melody, layers) per section: melody on flute or celesta, strings, percussion.
    struct Sec<'a> {
        ch: &'a [Chord],
        mel: Option<(&'a str, Instr, i32)>,
        strings: bool,
        perc: bool,
        bells: bool,
    }
    let form = [
        Sec {
            ch: &intro,
            mel: None,
            strings: false,
            perc: false,
            bells: false,
        },
        Sec {
            ch: &a,
            mel: Some((mel_a, Instr::Flute, 0)),
            strings: false,
            perc: false,
            bells: false,
        },
        Sec {
            ch: &a2,
            mel: Some((mel_a2, Instr::Flute, 0)),
            strings: true,
            perc: true,
            bells: false,
        },
        Sec {
            ch: &b,
            mel: Some((mel_b, Instr::Bell, 1)),
            strings: true,
            perc: false,
            bells: false,
        },
        Sec {
            ch: &a,
            mel: Some((mel_a, Instr::Flute, 0)),
            strings: true,
            perc: true,
            bells: true,
        },
        Sec {
            ch: &a2,
            mel: Some((mel_a2, Instr::Flute, 0)),
            strings: true,
            perc: true,
            bells: true,
        },
        Sec {
            ch: &outro,
            mel: None,
            strings: true,
            perc: false,
            bells: false,
        },
    ];
    let mut bar = 0;
    for (si, s) in form.iter().enumerate() {
        let start = c.bar(bar);
        if let Some((m, instr, oct)) = s.mel {
            let vel = if instr == Instr::Bell { 0.38 } else { 0.5 };
            c.melody(start, m, key, oct, 0, instr, vel, 0.15);
        }
        let last = si == form.len() - 1;
        for (i, ch) in s.ch.iter().enumerate() {
            let at = c.bar(bar + i);
            if last && i == s.ch.len() - 1 {
                // The final chord, strummed on the harp under a high celesta F.
                c.strum(at, 4.0, ch, 60, Instr::Harp, 0.4, -0.2);
                c.note(at + 0.5, 3.0, 89, 0.25, Instr::Bell, 0.3, 1.0);
                c.note(
                    at,
                    4.0,
                    place(ch.bass, 41),
                    0.45,
                    Instr::PluckBass,
                    0.0,
                    1.0,
                );
                continue;
            }
            let step = if last { 1.0 } else { 0.5 };
            c.arp(
                at,
                4.0,
                ch,
                53,
                &[0, 1, 2, 3, 4, 3, 2, 1],
                step,
                Instr::Harp,
                0.3,
                -0.2,
                1.0,
            );
            if si > 0 || i >= 2 {
                c.bass(
                    at,
                    ch,
                    &[(0.0, 1.5, B::Low), (1.5, 0.5, B::Low), (2.0, 2.0, B::Fifth)],
                    41,
                    Instr::PluckBass,
                    0.5,
                );
            }
            if s.strings {
                c.block(at, 4.0, ch, 62, Instr::Strings, 0.13, 0.25, 1.0);
            }
            if s.perc {
                c.drums(at, ".x.x.x.X", 0.5, 0.25);
                c.drums(at, "k...k...", 0.5, 0.35);
            }
            if s.bells {
                let third = place(ch.tones[1], 76);
                let fifth = place(ch.tones[2 % ch.tones.len()], 76);
                c.note(at, 2.0, third, 0.16, Instr::Bell, 0.35, 1.0);
                c.note(at + 2.0, 2.0, fifth, 0.13, Instr::Bell, 0.35, 1.0);
            }
        }
        bar += s.ch.len();
    }
    c.finish("Morning Shift", 0.8, 0.35, 0.73)
}

/// Quiet and spacious: a slow piano under the stars, in D major with a Lydian lift.
fn ore_and_ember() -> Track {
    let mut c = Comp::new(70.0, 4.0, 23);
    c.jitter = 0.014;
    c.spread = 0.12;
    let key = Key {
        tonic: 74,
        steps: MAJOR,
    };
    // Each chord lasts two bars.
    let intro = chords(&["Dmaj9", "Gmaj7"]);
    let a = chords(&["Dmaj9", "Gmaj7", "Em7", "Asus4"]);
    let b = chords(&["Bm7", "Gmaj7", "Em7", "Asus4"]);
    let end = chords(&["Dmaj9", "Gmaj7", "Asus4", "Dadd9"]);
    let seg1 = "r:1 3:1 5:1 3:1 2:3 r:1";
    let seg2 = "r:1 2:1 3:1 7,:1 6,:3 r:1";
    let seg3 = "r:1 5,:1 6,:1 2:2 r:1 7,:2";
    let seg4 = "1:2 7,:1 6,:1 7,:3 r:1";
    let seg4b = "2:1 1:1 7,:1 5,:1 5,:4";
    let b1 = "3:2 2:1 1:1 6,:4";
    let b2 = "7,:2 6,:1 5,:1 3,:4";
    let b3 = "5,:1 4,:1 2,:1 4,:1 5,:2 r:2";
    let b4 = "6,:1 5,:1 4,:2 2,:4";
    let a_mel = [seg1, seg2, seg3, seg4];
    let a2_mel = [seg1, seg2, seg3, seg4b];
    let b_mel = [b1, b2, b3, b4];
    let end_mel = [seg1, seg2, seg4b, "3:4 1:4"];
    struct Sec<'a> {
        ch: &'a [Chord],
        mel: Option<&'a [&'a str]>,
        pad: bool,
        bell: bool,
    }
    let form = [
        Sec {
            ch: &intro,
            mel: None,
            pad: false,
            bell: false,
        },
        Sec {
            ch: &a,
            mel: Some(&a_mel),
            pad: false,
            bell: false,
        },
        Sec {
            ch: &a,
            mel: Some(&a2_mel),
            pad: true,
            bell: true,
        },
        Sec {
            ch: &b,
            mel: Some(&b_mel),
            pad: true,
            bell: false,
        },
        Sec {
            ch: &end,
            mel: Some(&end_mel),
            pad: true,
            bell: false,
        },
    ];
    let mut bar = 0;
    for s in &form {
        for (i, ch) in s.ch.iter().enumerate() {
            let at = c.bar(bar + 2 * i);
            // The suspended chord resolves in its second bar.
            let (first, second) = if ch.tones == chord("Asus4").tones {
                (chord("Asus4"), chord("A"))
            } else {
                (ch.clone(), ch.clone())
            };
            for (half, h) in [&first, &second].into_iter().enumerate() {
                let hb = at + 4.0 * half as f32;
                let r = place(h.root, 38);
                let third = h.tones[1];
                let top = h.tones.get(3).copied().unwrap_or(h.tones[2]);
                let lh = [
                    (0.0, r),
                    (1.0, r + 7),
                    (2.0, place(third, r + 12)),
                    (3.0, place(top, r + 12)),
                ];
                // Held over into the next bar only while the harmony stays the same.
                let ring = if half == 0 && first.tones == second.tones {
                    1.0
                } else {
                    0.1
                };
                for (off, p) in lh {
                    c.note(
                        hb + off,
                        4.0 - off + ring,
                        p,
                        0.32 - off * 0.03,
                        Instr::Piano,
                        -0.15,
                        1.0,
                    );
                }
            }
            if let Some(m) = s.mel {
                c.melody(at, m[i], key, 0, 0, Instr::Piano, 0.42, 0.1);
                if s.bell {
                    c.melody(at, m[i], key, 1, 0, Instr::Bell, 0.12, 0.3);
                }
            }
            if s.pad {
                // The pad leaves the root to the piano, so it never sits a semitone off the
                // seventh in the left hand, and follows a suspension as it resolves.
                let halves: &[&Chord] = if first.tones == second.tones {
                    &[&first]
                } else {
                    &[&first, &second]
                };
                let len = 8.0 / halves.len() as f32;
                for (k, h) in halves.iter().enumerate() {
                    let upper = Chord {
                        tones: h.tones[1..].to_vec(),
                        ..(*h).clone()
                    };
                    c.block(
                        at + len * k as f32,
                        len,
                        &upper,
                        62,
                        Instr::Pad,
                        0.11,
                        0.0,
                        1.0,
                    );
                }
            }
        }
        bar += 2 * s.ch.len();
    }
    // A last high D, left to ring.
    let at = c.bar(bar);
    c.note(at - 4.0, 6.0, 86, 0.18, Instr::Bell, 0.3, 1.0);
    c.finish("Ore and Ember", 0.88, 0.3, 1.11)
}

/// A waltz in A minor with a sunnier C major middle: strings, flute and an oom-pah-pah piano.
fn conveyor_waltz() -> Track {
    let mut c = Comp::new(144.0, 3.0, 37);
    c.jitter = 0.006;
    c.spread = 0.07;
    let am = Key {
        tonic: 57,
        steps: MINOR,
    };
    let cmaj = Key {
        tonic: 60,
        steps: MAJOR,
    };
    let a = chords(&[
        "Am", "Am", "E7", "E7", "E7", "E7", "Am", "Am", "Dm", "Dm", "Am", "Am", "B7", "E7", "Am",
        "Am",
    ]);
    let b = chords(&[
        "C", "C", "G7", "G7", "G7", "G7", "C", "C", "F", "F", "C", "Am", "Dm7", "G7", "C", "C",
    ]);
    let intro = chords(&["Am", "Am", "Am", "E7"]);
    let turn = chords(&["E7", "E7", "E7", "E7"]);
    let coda = chords(&["Am", "Dm", "E7", "Am", "Am", "Am"]);
    let mel_a =
        "5:1 1':1 3':1 | 2':2 1':1 | 7#:1 2':1 4':1 | 3':2 2':1 | 2':1 7#:1 5:1 | 4':2 3':1 |
                 3':1 2':1 1':1 | 5:3 | 6:1 1':1 4':1 | 6':2 4':1 | 3':1 1':1 5:1 | 1':2 r:1 |
                 4#':1 2':1 1':1 | 7#:1 2':1 4':1 | 3':1 7#:1 2':1 | 1':3";
    let mel_b = "3:2 5:1 | 1':2 7:1 | 2':1 7:1 5:1 | 4':2 3':1 | 2':1 4':1 7:1 | 6:2 5:1 |
                 3':1 2':1 1':1 | 5:3 | 6:1 1':1 4':1 | 3':2 2':1 | 1':1 5:1 3:1 | 6:2 1':1 |
                 2':1 4':1 6':1 | 5':1 4':1 2':1 | 3':1 2':1 7:1 | 1':3";
    let mel_turn = "7#,:1 2:1 4:1 | 7#:1 2':1 4':1 | 3':3 | 2':3";
    let mel_coda = "1':1 3':1 5':1 | 6':2 4':1 | 2':1 7#:1 5:1 | 1':3 | r:3 | r:3";
    // (chords, key, melody, lead instrument, octave, strings pad underneath)
    let form: [(&[Chord], Key, Option<&str>, Instr, i32, bool); 7] = [
        (&intro, am, None, Instr::Strings, 0, false),
        (&a, am, Some(mel_a), Instr::Strings, 0, false),
        (&a, am, Some(mel_a), Instr::Flute, 1, true),
        (&b, cmaj, Some(mel_b), Instr::Strings, 0, false),
        (&b, cmaj, Some(mel_b), Instr::Flute, 0, true),
        (&turn, am, Some(mel_turn), Instr::Strings, 0, false),
        (&a, am, Some(mel_a), Instr::Strings, 0, true),
    ];
    let mut bar = 0;
    for (k, &(ch, key, mel, lead, oct, pad)) in form.iter().enumerate() {
        let start = c.bar(bar);
        if let Some(m) = mel {
            let vel = if lead == Instr::Flute { 0.5 } else { 0.42 };
            c.melody(start, m, key, oct, 0, lead, vel, 0.2);
            // The last time through, the flute doubles the strings an octave up.
            if k == form.len() - 1 {
                c.melody(start, m, key, 1, 0, Instr::Flute, 0.32, -0.15);
            }
        }
        for (i, h) in ch.iter().enumerate() {
            let at = c.bar(bar + i);
            let low = if i % 2 == 0 { B::Low } else { B::Fifth };
            c.bass(at, h, &[(0.0, 1.0, low)], 40, Instr::PluckBass, 0.5);
            for beat in [1.0, 2.0] {
                c.block(at + beat, 1.0, h, 62, Instr::Piano, 0.17, -0.2, 0.5);
            }
            if pad {
                c.block(at, 3.0, h, 57, Instr::Strings, 0.09, -0.3, 1.0);
            }
            // Pizzicato harp answers on the third beat in the major section.
            if key.tonic == 60 && lead == Instr::Strings {
                c.arp(
                    at + 2.0,
                    1.0,
                    h,
                    67,
                    &[0, 2],
                    0.5,
                    Instr::Harp,
                    0.2,
                    0.35,
                    1.0,
                );
            }
        }
        bar += ch.len();
    }
    let start = c.bar(bar);
    c.melody(start, mel_coda, am, 0, 0, Instr::Strings, 0.42, 0.2);
    for (i, h) in coda.iter().enumerate() {
        let at = c.bar(bar + i);
        if i < 4 {
            c.bass(at, h, &[(0.0, 1.0, B::Low)], 40, Instr::PluckBass, 0.5);
            for beat in [1.0, 2.0] {
                c.block(at + beat, 1.0, h, 62, Instr::Piano, 0.17, -0.2, 0.5);
            }
        }
        if i == 3 {
            c.block(at, 9.0, h, 60, Instr::Strings, 0.2, -0.2, 1.0);
            c.strum(at, 6.0, h, 64, Instr::Harp, 0.35, 0.3);
            c.note(at, 9.0, 45, 0.45, Instr::PluckBass, 0.0, 1.0);
        }
    }
    c.finish("Conveyor Waltz", 0.82, 0.4, 0.78)
}

/// Night shift at the factory: a pulsing sequencer, steady drums and a slow synth melody in
/// E minor, building up and falling away.
fn night_grid() -> Track {
    let mut c = Comp::new(118.0, 4.0, 41);
    let key = Key {
        tonic: 64,
        steps: MINOR,
    };
    let p1 = chords(&["Em", "Em", "Cadd9", "Cadd9", "G", "G", "D", "D"]);
    let p2 = chords(&["Am7", "Am7", "Em", "Em", "Cmaj7", "Cmaj7", "B", "B"]);
    let lead_a1 = "5:2 3:1 5:1 | 1':3 7:1 | 7:2 6:1 5:1 | 3:4 | 5:1 3:1 5:1 2':1 | 1':2 7:2 | 7:2 2':1 1':1 | 7:4";
    let lead_a2 = "5:2 3:1 5:1 | 1':2 2':1 3':1 | 3':2 2':1 1':1 | 7:2 5:2 | 2':2 1':1 7:1 | 5:4 | 6:1 7:1 2':2 | 5:4";
    let lead_b = "3':2 1':1 6:1 | 1':4 | 7:1 1':1 7:1 5:1 | 3:4 | 5:1 7:1 2':1 3':1 | 2':2 7:2 | 2':2 7#:2 | 5:4";
    // Layers: arp, pad, bass, hats, full drums, lead, and how loud overall.
    struct Sec<'a> {
        ch: &'a [Chord],
        lead: Option<(&'a str, i32)>,
        bass: bool,
        hats: bool,
        drums: bool,
        level: f32,
    }
    let form = [
        Sec {
            ch: &p1,
            lead: None,
            bass: false,
            hats: false,
            drums: false,
            level: 0.8,
        },
        Sec {
            ch: &p1,
            lead: None,
            bass: true,
            hats: true,
            drums: false,
            level: 0.9,
        },
        Sec {
            ch: &p1,
            lead: Some((lead_a1, 0)),
            bass: true,
            hats: true,
            drums: true,
            level: 1.0,
        },
        Sec {
            ch: &p1,
            lead: Some((lead_a2, 0)),
            bass: true,
            hats: true,
            drums: true,
            level: 1.0,
        },
        Sec {
            ch: &p2,
            lead: None,
            bass: false,
            hats: false,
            drums: false,
            level: 0.85,
        },
        Sec {
            ch: &p2,
            lead: Some((lead_b, 0)),
            bass: true,
            hats: true,
            drums: true,
            level: 1.0,
        },
        Sec {
            ch: &p2,
            lead: Some((lead_b, 1)),
            bass: true,
            hats: true,
            drums: true,
            level: 1.0,
        },
        Sec {
            ch: &p1,
            lead: None,
            bass: true,
            hats: false,
            drums: false,
            level: 0.7,
        },
    ];
    let arp = [0, 2, 1, 3, 2, 4, 3, 1, 0, 2, 4, 5, 4, 2, 3, 1];
    let mut bar = 0;
    for (si, s) in form.iter().enumerate() {
        let start = c.bar(bar);
        if let Some((m, oct)) = s.lead {
            c.melody(start, m, key, oct, 0, Instr::Strings, 0.42, 0.15);
            c.melody(
                start,
                m,
                key,
                oct + 1,
                0,
                Instr::Bell,
                0.07 + 0.05 * oct as f32,
                -0.25,
            );
        }
        let n = s.ch.len();
        for (i, h) in s.ch.iter().enumerate() {
            let at = c.bar(bar + i);
            // The last section fades out bar by bar.
            let fade = if si == form.len() - 1 {
                1.0 - i as f32 / n as f32
            } else {
                1.0
            };
            let lv = s.level * fade;
            let ladder = h.ladder(52, 8);
            for (k, &step) in arp.iter().enumerate() {
                let p = ladder[step % 8];
                let accent = if [0, 3, 6, 10, 13].contains(&k) {
                    1.0
                } else {
                    0.62
                };
                c.note(
                    at + k as f32 * 0.25,
                    0.25,
                    p,
                    0.3 * accent * lv,
                    Instr::Seq,
                    if k % 2 == 0 { -0.3 } else { 0.3 },
                    0.8,
                );
            }
            if i % 2 == 0 {
                c.block(at, 8.0, h, 64, Instr::Pad, 0.16 * lv, 0.0, 1.0);
            }
            if s.bass {
                let pat: Vec<(f32, f32, B)> = (0..8)
                    .map(|k| {
                        (
                            k as f32 * 0.5,
                            0.5,
                            if k == 3 || k == 7 { B::High } else { B::Low },
                        )
                    })
                    .collect();
                c.bass(at, h, &pat, 36, Instr::Bass, 0.32 * lv);
            }
            if s.hats {
                c.drums(at, "hhHhhhHhhhHhhhHh", 0.25, 0.22 * lv);
            }
            if s.drums {
                c.drums(at, "k...k...k...k...", 0.25, 0.6 * lv);
                c.drums(at, "....s.......s...", 0.25, 0.35 * lv);
                if si >= 5 {
                    c.drums(at, "..o...o...o...o.", 0.25, 0.12 * lv);
                }
            } else if s.hats {
                c.drums(at, "k.......k.......", 0.25, 0.45 * lv);
            }
        }
        bar += n;
    }
    let at = c.bar(bar);
    c.block(at, 6.0, &chord("Em"), 64, Instr::Pad, 0.14, 0.0, 1.0);
    c.note(at, 4.0, 40, 0.4, Instr::Bass, 0.0, 1.0);
    c.finish("Night Grid", 0.75, 0.45, 0.71)
}

/// A modal folk march, as if from an old kingdom: recorder, lute and drum in D Dorian.
fn kingdom_of_cogs() -> Track {
    let mut c = Comp::new(104.0, 4.0, 53);
    c.jitter = 0.007;
    c.spread = 0.1;
    let key = Key {
        tonic: 74,
        steps: DORIAN,
    };
    let a = chords(&["Dm", "C", "Dm", "Am", "F", "G", "Dm", "Dm"]);
    let b = chords(&["F", "C", "G", "Dm", "F", "Dm7", "Am", "Dm"]);
    let intro = chords(&["Dm", "Dm", "C", "Dm"]);
    let mel_a = "1:1 3:0.5 4:0.5 5:1 5:1 | 4:0.5 3:0.5 2:1 1:2 | 1:1 3:0.5 4:0.5 5:1 1':1 | 7:1.5 6:0.5 5:2 |
                 3':1 1':1 7:0.5 6:0.5 5:1 | 4:1 5:0.5 4:0.5 3:1 2:1 | 1:0.5 2:0.5 3:1 2:1 7,:1 | 1:3 r:1";
    let mel_b = "5:1 1':1 3':1.5 2':0.5 | 2':1 1':0.5 7:0.5 5:2 | 6:1 5:0.5 6:0.5 1':2 | 5:1.5 4:0.5 3:1 1:1 |
                 5:1 1':1 3':1.5 4':0.5 | 3':1 2':1 1':1 7:1 | 1':1 7:1 6:0.5 7:0.5 5:1 | 1:3 r:1";
    // (chords, melody, harmony below on the harp, bowed chords, full drums)
    let form: [(&[Chord], Option<&str>, bool, bool, bool); 7] = [
        (&intro, None, false, false, false),
        (&a, Some(mel_a), false, false, false),
        (&a, Some(mel_a), true, false, true),
        (&b, Some(mel_b), false, true, true),
        (&b, Some(mel_b), true, true, true),
        (&a, Some(mel_a), true, true, true),
        (&a, Some(mel_a), true, true, true),
    ];
    let mut bar = 0;
    for (k, &(ch, mel, harm, bowed, full)) in form.iter().enumerate() {
        let start = c.bar(bar);
        if let Some(m) = mel {
            c.melody(start, m, key, 0, 0, Instr::Flute, 0.48, 0.15);
            if harm {
                c.harmonize(c.bar(bar), ch, Instr::Flute, 15, Instr::Harp, 0.28, -0.3);
            }
            if k == form.len() - 1 {
                c.melody(start, m, key, 1, 0, Instr::Bell, 0.1, 0.35);
            }
        }
        for (i, h) in ch.iter().enumerate() {
            let at = c.bar(bar + i);
            for (off, len) in [(0.0, 1.5), (1.5, 0.5), (2.0, 1.0), (3.0, 1.0)] {
                let v = if off == 0.0 { 0.36 } else { 0.27 };
                c.strum(at + off, len, h, 58, Instr::Lute, v, -0.15);
            }
            c.bass(
                at,
                h,
                &[(0.0, 2.0, B::Low), (2.0, 2.0, B::Fifth)],
                38,
                Instr::PluckBass,
                0.45,
            );
            if bowed {
                c.block(at, 4.0, h, 55, Instr::Strings, 0.12, 0.3, 1.0);
            } else {
                // A drone on D and A, like a hurdy-gurdy.
                c.note(at, 4.0, 50, 0.1, Instr::Strings, 0.3, 1.0);
                c.note(at, 4.0, 57, 0.08, Instr::Strings, 0.3, 1.0);
            }
            let fill = i == ch.len() - 1;
            c.drums(
                at,
                if fill {
                    "t...t...t.t.tttt"
                } else {
                    "t.......t...t.t."
                },
                0.25,
                0.3,
            );
            if full {
                c.drums(
                    at,
                    if fill {
                        "....s...s.s.ssss"
                    } else {
                        "....s.......s..s"
                    },
                    0.25,
                    0.16,
                );
                c.drums(at, "..x...x...x...x.", 0.25, 0.12);
            }
        }
        bar += ch.len();
    }
    // Outro: the drone and lute settle on D.
    let d = chord("Dm");
    for i in 0..3 {
        let at = c.bar(bar + i);
        c.strum(at, 2.0, &d, 58, Instr::Lute, 0.26 - 0.05 * i as f32, -0.15);
        c.drums(at, "t.......t.......", 0.25, 0.35 - 0.08 * i as f32);
    }
    let at = c.bar(bar);
    c.note(at, 12.0, 50, 0.12, Instr::Strings, 0.3, 1.0);
    c.note(at, 12.0, 57, 0.1, Instr::Strings, 0.3, 1.0);
    c.note(at + 8.0, 4.0, 74, 0.35, Instr::Flute, 0.15, 1.0);
    c.finish("Kingdom of Cogs", 0.7, 0.4, 0.67)
}

/// Chiptune for the terminal: square lead, fast pulse arpeggios, a stepped triangle bass,
/// noise drums and the tapping of keys, in C minor.
fn terminal_dreams() -> Track {
    let mut c = Comp::new(136.0, 4.0, 67);
    let key = Key {
        tonic: 60,
        steps: MINOR,
    };
    let a = chords(&["Cm", "Ab", "Eb", "Bb", "Cm", "Ab", "Fm", "G"]);
    let b = chords(&["Ab", "Bb", "Gm", "Cm", "Fm", "Bb", "Eb", "G7"]);
    let bridge = chords(&["Ab", "Bb", "Cm", "Cm", "Ab", "Bb", "G", "G"]);
    let intro = chords(&["Cm", "Cm", "Ab", "G"]);
    let mel_a = "1':1/2 5:1/2 3:1/2 5:1/2 1':1 2':1/2 3':1/2 | 3':1 1':1 6:1 r:1/2 6:1/2 |
                 7:1/2 1':1/2 7:1/2 5:1/2 3:1 5:1 | 4:3/2 2:1/2 4:1 7:1 |
                 1':1/2 5:1/2 3:1/2 5:1/2 1':1 2':1/2 3':1/2 | 4':1 3':1 1':1 6:1 |
                 6:1 1':1 4':1 3':1/2 2':1/2 | 7#:2 2':1 5:1";
    let mel_b = "1':3/2 3':1/2 1':1 6:1 | 2':3/2 4':1/2 2':1 7:1 | 7:1 2':1 5':1 4':1/2 2':1/2 | 3':2 r:1 1':1 |
                 6:1 1':1 4':3/2 3':1/2 | 2':1 4':1 7':2 | 5':1 3':1 1':1/2 3':1/2 5':1 | 4':1 2':1 7#:2";
    // (chords, melody and octave, drums, echoing pulse copy of the lead)
    let form: [(&[Chord], Option<(&str, i32)>, bool, bool); 9] = [
        (&intro, None, false, false),
        (&a, Some((mel_a, 0)), false, false),
        (&a, Some((mel_a, 0)), true, false),
        (&b, Some((mel_b, 0)), true, false),
        (&b, Some((mel_b, 0)), true, true),
        (&a, Some((mel_a, 1)), true, true),
        (&bridge, None, false, false),
        (&b, Some((mel_b, 0)), true, true),
        (&a, Some((mel_a, 1)), true, true),
    ];
    let mut bar = 0;
    for (k, &(ch, mel, drums, echo)) in form.iter().enumerate() {
        let start = c.bar(bar);
        let is_bridge = k == 6;
        if let Some((m, oct)) = mel {
            c.melody(start, m, key, oct, 0, Instr::Square, 0.55, 0.1);
            if echo {
                c.melody(start + 0.75, m, key, oct, 0, Instr::Pulse, 0.2, -0.4);
            }
        }
        for (i, h) in ch.iter().enumerate() {
            let at = c.bar(bar + i);
            if is_bridge {
                // Half time: slow arpeggios with an echo, and long bass notes.
                for s in 0..8 {
                    let p = h.ladder(67, 6)[[0, 1, 2, 3, 2, 1, 2, 4][s] % 6];
                    let t = at + s as f32 * 0.5;
                    c.note(t, 0.5, p, 0.3, Instr::Pulse, -0.2, 0.7);
                    c.note(t + 0.75, 0.5, p, 0.12, Instr::Pulse, 0.4, 0.7);
                }
                c.note(at, 4.0, place(h.bass, 36), 0.5, Instr::Triangle, 0.0, 0.95);
            } else {
                c.arp(
                    at,
                    4.0,
                    h,
                    72,
                    &[0, 1, 2],
                    0.125,
                    Instr::Pulse,
                    0.17,
                    -0.2,
                    0.9,
                );
                let pat: Vec<(f32, f32, B)> = (0..8)
                    .map(|k| {
                        (
                            k as f32 * 0.5,
                            0.5,
                            if k % 2 == 1 { B::High } else { B::Low },
                        )
                    })
                    .collect();
                c.bass(at, h, &pat, 36, Instr::Triangle, 0.4);
            }
            if drums {
                c.drums(at, "k.....k...k.....", 0.25, 0.55);
                c.drums(at, "....n.......n..n", 0.25, 0.4);
                c.drums(at, "h.h.h.h.h.h.h.h.", 0.25, 0.14);
            } else if is_bridge {
                c.drums(at, "k...............", 0.25, 0.45);
                c.drums(at, "........n.......", 0.25, 0.3);
            }
            // Keys tapping away: a few clicks a bar on a pentatonic scale.
            if !drums {
                for s in 0..16 {
                    if c.rand() < 0.3 {
                        let p = [84, 87, 89, 91, 94, 96][(c.rand() * 6.0) as usize % 6];
                        let vel = 0.15 + 0.1 * c.rand();
                        c.note(at + s as f32 * 0.25, 0.25, p, vel, Instr::Click, 0.35, 1.0);
                    }
                }
            }
        }
        bar += ch.len();
    }
    // Outro: the arpeggio on C minor, slowing to a last chord.
    let cm = chord("Cm");
    for i in 0..3 {
        let at = c.bar(bar + i);
        let step = [0.125, 0.25, 0.5][i];
        c.arp(
            at,
            4.0,
            &cm,
            72,
            &[0, 1, 2],
            step,
            Instr::Pulse,
            0.17 - 0.04 * i as f32,
            -0.2,
            0.9,
        );
        c.note(at, 4.0, 36, 0.45, Instr::Triangle, 0.0, 0.95);
    }
    let at = c.bar(bar + 3);
    c.block(at, 3.0, &cm, 72, Instr::Square, 0.25, 0.0, 1.0);
    c.note(at, 3.0, 36, 0.45, Instr::Triangle, 0.0, 1.0);
    c.finish("Terminal Dreams", 0.6, 0.5, 0.71)
}

/// Every track, in playing order.
pub fn tracks() -> Vec<Track> {
    vec![
        morning_shift(),
        ore_and_ember(),
        conveyor_waltz(),
        night_grid(),
        kingdom_of_cogs(),
        terminal_dreams(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_and_keys_spell_correctly() {
        assert_eq!(chord("Am7").tones, vec![9, 0, 4, 7]);
        assert_eq!(chord("Bbmaj7").tones, vec![10, 2, 5, 9]);
        assert_eq!(chord("C/E").bass, 4);
        assert_eq!(chord("F#7").tones, vec![6, 10, 1, 4]);
        let f = Key {
            tonic: 65,
            steps: MAJOR,
        };
        assert_eq!(f.pitch(1, 0, 0), 65);
        assert_eq!(f.pitch(8, 0, 0), 77);
        assert_eq!(f.pitch(4, 0, 0), 70);
        assert_eq!(f.pitch(7, 0, -1), 64 - 12 + 12);
        assert_eq!(chord("Dm7").voicing(62), vec![57, 60, 62, 65]);
        assert_eq!(chord("Bbmaj7").voicing(62), vec![58, 62, 65, 69]);
        assert_eq!(chord("Cmaj7").voicing(64), vec![60, 64, 67, 71]);
    }

    #[test]
    fn melodies_fill_their_bars() {
        let mut c = Comp::new(120.0, 4.0, 1);
        let key = Key {
            tonic: 60,
            steps: MAJOR,
        };
        let beats = c.melody(
            0.0,
            "1:1 2:1/2 3:1/2 r:2 | 5' 4#,",
            key,
            0,
            0,
            Instr::Flute,
            0.5,
            0.0,
        );
        // The rest's two beats carry over to the last two notes.
        assert_eq!(beats, 8.0);
        let pitches: Vec<f32> = c.notes.iter().map(|n| n.midi).collect();
        assert_eq!(pitches, vec![60.0, 62.0, 64.0, 79.0, 54.0]);
    }

    #[test]
    fn the_soundtrack_runs_over_ten_minutes() {
        let all = tracks();
        let total: f32 = all.iter().map(|t| t.secs).sum();
        assert!(total > 600.0, "only {total:.0} s");
        for t in &all {
            assert!(t.secs > 90.0, "{} is {:.0} s", t.title, t.secs);
            assert!(t.notes.windows(2).all(|w| w[0].start <= w[1].start));
            assert!(
                t.notes
                    .iter()
                    .all(|n| n.vel > 0.0 && n.vel <= 1.0 && n.dur > 0.0)
            );
            assert!(t.notes.iter().all(|n| (20.0..110.0).contains(&n.midi)));
        }
    }
}
