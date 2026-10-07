//! Sounds: a few notes for a guess, a win, a hint and so on.
//!
//! A terminal cannot play anything but its bell, so each sound is made here as a short
//! WAV file and handed to whatever plays audio on the machine: `afplay` on macOS,
//! PipeWire's, PulseAudio's or ALSA's player on Linux. With none of them the game is
//! silent, and nothing else changes. No audio library is linked in: one would need
//! system libraries at build time, and the release binaries are static.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::game::{Mark, Marks};

/// Samples per second. Low is plenty for a few plain notes, and keeps the files small.
const RATE: u32 = 22_050;
/// How loud a single note is, out of 1. Notes that overlap add up, and are clipped at 1.
const LOUDNESS: f32 = 0.22;
/// No more than this many sounds at once: keys held down must not pile up players.
const MAX_PLAYING: usize = 4;

/// Something worth a sound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue {
    /// A guess is turned over: one note per tile, `step` milliseconds apart, high for
    /// green, middle for yellow, low for gray. It is the result, told by ear.
    Reveal(Marks, u32),
    /// A guess was refused.
    Refused,
    Won,
    Lost,
    Hint,
    /// Sound was just switched on.
    On,
}

impl Cue {
    /// The file a cue is written to. One per kind: a new guess overwrites the last.
    fn file(self) -> &'static str {
        match self {
            Cue::Reveal(..) => "reveal.wav",
            Cue::Refused => "refused.wav",
            Cue::Won => "won.wav",
            Cue::Lost => "lost.wav",
            Cue::Hint => "hint.wav",
            Cue::On => "on.wav",
        }
    }

    /// The notes of a cue as (start in ms, frequency in Hz, length in ms).
    fn notes(self) -> Vec<(u32, f32, u32)> {
        match self {
            Cue::Reveal(marks, step) => marks
                .iter()
                .enumerate()
                .map(|(i, mark)| {
                    let (freq, len) = match mark {
                        Mark::Green => (659.3, 170),
                        Mark::Yellow => (440.0, 140),
                        Mark::Gray => (220.0, 110),
                    };
                    (i as u32 * step, freq, len)
                })
                .collect(),
            Cue::Refused => vec![(0, 196.0, 110), (120, 146.8, 160)],
            // C, E, G and the C above, the last one held.
            Cue::Won => vec![(0, 523.3, 140), (110, 659.3, 140), (220, 784.0, 140), (330, 1046.5, 520), (330, 523.3, 520)],
            Cue::Lost => vec![(0, 392.0, 260), (240, 329.6, 420)],
            Cue::Hint => vec![(0, 880.0, 90), (70, 1174.7, 90), (140, 1568.0, 220)],
            Cue::On => vec![(0, 659.3, 120), (90, 880.0, 180)],
        }
    }

    /// The cue as sound: one channel, 16 bits a sample, `RATE` samples a second.
    fn samples(self) -> Vec<i16> {
        let notes = self.notes();
        let at = |ms: u32| (ms as u64 * RATE as u64 / 1000) as usize;
        let total = notes.iter().map(|&(start, _, len)| at(start + len)).max().unwrap_or(0);
        let mut mix = vec![0.0_f32; total];
        for (start, freq, len) in notes {
            let (from, count) = (at(start), at(len));
            for i in 0..count {
                let t = i as f32 / RATE as f32;
                // A quick rise so the note does not click, then it dies away like a bell.
                let rise = (i as f32 / (RATE as f32 * 0.004)).min(1.0);
                let fall = (1.0 - i as f32 / count as f32).powi(2);
                let phase = std::f32::consts::TAU * freq * t;
                // The octave above, quietly, makes it a chime and not a test tone.
                mix[from + i] += LOUDNESS * rise * fall * (phase.sin() + 0.3 * (2.0 * phase).sin());
            }
        }
        mix.into_iter().map(|v| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect()
    }
}

/// Samples as a WAV file.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    // PCM, one channel.
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    // Bytes a second, bytes a sample, bits a sample.
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// The programs that can play a WAV file, in the order they are tried, each with the
/// arguments that go before the file.
const PLAYERS: [(&str, &[&str]); 4] = [("afplay", &[]), ("pw-play", &[]), ("paplay", &[]), ("aplay", &["-q"])];

/// The first of `PLAYERS` found on `PATH`. Looked for, not run: finding out must make
/// no noise.
fn find_player() -> Option<(PathBuf, &'static [&'static str])> {
    // The unit tests must never make a sound on the machine they run on.
    if cfg!(test) {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    PLAYERS.iter().find_map(|&(name, args)| std::env::split_paths(&path).map(|dir| dir.join(name)).find(|file| file.is_file()).map(|file| (file, args)))
}

pub struct Sound {
    /// Whether sounds are wanted. They are heard only if there is also a player.
    pub on: bool,
    player: Option<(PathBuf, &'static [&'static str])>,
    /// A directory of this run's own for the sound files, made when first needed and
    /// removed when the game ends.
    dir: Option<PathBuf>,
    playing: Vec<Child>,
    /// Every cue asked for while sound was on, for the tests.
    #[cfg(test)]
    pub log: Vec<Cue>,
}

impl Sound {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            player: find_player(),
            dir: None,
            playing: Vec::new(),
            #[cfg(test)]
            log: Vec::new(),
        }
    }

    /// Whether this machine has anything to play sounds with.
    pub fn available(&self) -> bool {
        self.player.is_some()
    }

    fn dir(&mut self) -> Option<&Path> {
        if self.dir.is_none() {
            let dir = std::env::temp_dir().join(format!("funwordl-sound-{}", std::process::id()));
            fs::create_dir_all(&dir).ok()?;
            self.dir = Some(dir);
        }
        self.dir.as_deref()
    }

    /// Plays a cue, if sound is on and can be played. It never waits for the sound and
    /// never fails: a game without sound is still the game.
    pub fn play(&mut self, cue: Cue) {
        if !self.on {
            return;
        }
        #[cfg(test)]
        self.log.push(cue);
        // Forget the players that have finished.
        self.playing.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
        let Some((player, args)) = self.player.clone() else { return };
        if self.playing.len() >= MAX_PLAYING {
            return;
        }
        let Some(file) = self.dir().map(|dir| dir.join(cue.file())) else { return };
        if fs::write(&file, wav(&cue.samples())).is_err() {
            return;
        }
        // Nothing of the player's may reach the terminal: the screen is the game's.
        if let Ok(child) = Command::new(player).args(args).arg(&file).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            self.playing.push(child);
        }
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        // A player still going has its file open and plays to the end regardless.
        if let Some(dir) = &self.dir {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Cue; 6] =
        [Cue::Reveal([Mark::Green, Mark::Yellow, Mark::Gray, Mark::Gray, Mark::Green], 190), Cue::Refused, Cue::Won, Cue::Lost, Cue::Hint, Cue::On];

    #[test]
    fn every_cue_is_a_short_sound_that_is_neither_silent_nor_clipped() {
        for cue in ALL {
            let samples = cue.samples();
            let seconds = samples.len() as f32 / RATE as f32;
            assert!((0.1..=1.5).contains(&seconds), "{cue:?} lasts {seconds} s");
            let loudest = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!(loudest > 2000, "{cue:?} is nearly silent");
            assert!(loudest < i16::MAX as u16, "{cue:?} is clipped");
            // It ends quietly, so that it does not click.
            assert!(samples.last().unwrap().unsigned_abs() < 200, "{cue:?} ends abruptly");
        }
    }

    #[test]
    fn a_reveal_plays_one_note_per_tile_in_step_with_the_flip() {
        let marks = [Mark::Gray, Mark::Yellow, Mark::Green, Mark::Gray, Mark::Green];
        let notes = Cue::Reveal(marks, 190).notes();
        assert_eq!(notes.iter().map(|n| n.0).collect::<Vec<_>>(), [0, 190, 380, 570, 760]);
        // The better the tile, the higher the note.
        assert!(notes[2].1 > notes[1].1 && notes[1].1 > notes[0].1);
        assert_eq!(notes[2].1, notes[4].1);
        // Without the animation the notes come close together.
        assert!(Cue::Reveal(marks, 70).samples().len() < Cue::Reveal(marks, 190).samples().len());
    }

    #[test]
    fn writes_a_wav_file_a_player_accepts() {
        let file = wav(&[0, 1, -1, i16::MAX]);
        assert_eq!(file.len(), 44 + 8);
        assert_eq!((&file[0..4], &file[8..16], &file[36..40]), (&b"RIFF"[..], &b"WAVEfmt "[..], &b"data"[..]));
        assert_eq!(u32::from_le_bytes(file[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), RATE);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 8);
        assert_eq!(&file[50..52], &i16::MAX.to_le_bytes());
    }

    #[test]
    fn nothing_is_played_or_written_while_sound_is_off() {
        let mut sound = Sound::new(false);
        sound.play(Cue::Won);
        assert!(sound.log.is_empty() && sound.dir.is_none() && sound.playing.is_empty());
        // Switched on, the cue is taken; the tests have no player, so it stops there.
        sound.on = true;
        sound.play(Cue::Won);
        assert_eq!(sound.log, [Cue::Won]);
        assert!(!sound.available() && sound.dir.is_none());
    }
}
