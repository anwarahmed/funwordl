//! The state of the running game and everything keys and clicks do to it.
//! `ui.rs` only reads this; nothing here draws.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use std::collections::BTreeMap;

use crate::game::{self, Game, Mark, Mode, Status, Word};
use crate::layout;
use crate::level::{self, Level, MAX_HINTS};
use crate::sound::{Cue, Sound};
use crate::store::{self, Stats};
use crate::theme::{self, Theme};
use crate::{ui, words};

/// How long one tile takes to flip over when a guess is revealed.
pub const FLIP: Duration = Duration::from_millis(190);
/// One step of the row's shake after a refused guess, and how many steps there are.
pub const SHAKE_STEP: Duration = Duration::from_millis(35);
pub const SHAKE_STEPS: u32 = 7;
/// One step of the flash that runs along a winning row.
pub const FLASH_STEP: Duration = Duration::from_millis(70);
/// How long confetti falls after a word is solved.
pub const CONFETTI: Duration = Duration::from_millis(2600);
/// The pause between the end of a game and the statistics opening by themselves.
const STATS_DELAY: Duration = Duration::from_millis(1300);

/// The day number (days since 1970-01-01, local time) of the day before puzzle #1.
pub const DAILY_BASE: i64 = 20732;

/// The daily word. Not wordl's word for the day: someone who plays both games would
/// otherwise be handed the answer by the first. Changing the answer list changes it.
pub fn daily_answer(day: i64) -> Word {
    let list = words::answers();
    let index = (day * 6151 + 51_287).rem_euclid(list.len() as i64) as usize;
    game::word(list[index]).expect("answers are five letters")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modal {
    None,
    Help,
    Stats,
    /// "Show the word?", asked before a game is ended on purpose.
    GiveUp,
    /// The hints taken for this word.
    Hint,
    /// The words met so far, one at a time, each with its meaning.
    Words,
}

/// A key of the on-screen keyboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyId {
    Letter(u8),
    Enter,
    Back,
}

/// Something a key or a click asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Key(KeyId),
    Help,
    New,
    Daily,
    Stats,
    Theme,
    Difficulty,
    /// Switch sound on or off.
    Sound,
    /// Show the hints; the first time, and from inside the hint dialog, take one more.
    Hint,
    /// Open the collection of words met so far.
    Words,
    /// Turn to the word before or after in the collection.
    CardPrev,
    CardNext,
    /// Ask whether to give up.
    GiveUp,
    /// Give up, confirmed.
    Surrender,
    Copy,
    Close,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageKind {
    /// Something was refused; shown as a chip.
    Error,
    /// The game was won.
    Win,
    /// A plain note.
    Plain,
}

pub struct Message {
    pub text: String,
    pub kind: MessageKind,
    /// When it disappears; `None` stays.
    until: Option<Instant>,
}

/// How the game was started.
pub struct Options {
    pub mode: Mode,
    pub theme: Option<String>,
    pub level: Option<Level>,
    pub animate: bool,
    /// Whether sounds are wanted; the tests and `FUNWORDL_NO_SOUND` say no.
    pub sound: bool,
    pub truecolor: bool,
    /// Fixes the practice word, for tests (`FUNWORDL_DEBUG_ANSWER`).
    pub debug_answer: Option<Word>,
}

pub struct App {
    pub game: Game,
    pub stats: Stats,
    pub theme: Theme,
    truecolor: bool,
    /// The level new games start with. A game under way keeps its own.
    pub chosen: Level,
    /// Hints taken for this word: the first is its meaning, each one after it a letter.
    pub hints: usize,
    /// The spots (0 to 4) whose letters the hints have given away.
    pub hint_letters: Vec<usize>,
    /// Every word met so far with the most stars it earned ("0" for one not solved),
    /// as kept in the `learned` file. Sorted by word.
    pub learned: BTreeMap<String, String>,
    /// Which word of the collection is on show.
    pub card: usize,
    pub modal: Modal,
    pub message: Option<Message>,
    /// The end-of-game message returns once a passing note ("Result copied") is gone.
    restore_end: bool,
    pending_stats: Option<Instant>,
    /// Row being revealed and when it started.
    pub reveal: Option<(usize, Instant)>,
    pub shake: Option<Instant>,
    pub celebrate: Option<(usize, Instant)>,
    /// When the confetti started falling.
    pub confetti: Option<Instant>,
    pub sound: Sound,
    animate: bool,
    /// The time of the frame being drawn; animations are a function of it.
    pub now: Instant,
    pub quit: bool,
    /// Terminal size in columns and rows, kept current by the event loop.
    pub size: (i32, i32),
    /// Set to have the whole screen repainted (Ctrl-L).
    pub repaint: bool,
    debug_answer: Option<Word>,
    seed: u64,
}

/// Local days since 1970-01-01.
pub fn today() -> i64 {
    chrono::Local::now().date_naive().signed_duration_since(chrono::NaiveDate::default()).num_days()
}

impl App {
    pub fn new(mut stats: Stats, options: Options) -> Self {
        stats.expire_daily_streak(today());
        // Normal unless another level was chosen: `level` is absent from a new file.
        let saved_level = stats.text("level").and_then(|v| v.parse::<usize>().ok()).filter(|&v| v <= 3).map_or(Level::DEFAULT, Level::from_index);
        let chosen = options.level.unwrap_or(saved_level);
        let saved_theme = stats.text("theme").filter(|name| theme::NAMES.contains(name)).unwrap_or(theme::DEFAULT);
        let theme = theme::theme(options.theme.as_deref().unwrap_or(saved_theme), options.truecolor);
        let learned = store::read_pairs(&stats.dir().join("learned"));
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) ^ ((std::process::id() as u64) << 32) | 1;
        let mut app = Self {
            game: Game::new(Mode::Practice, 0, *b"WORDL", chosen.difficulty()),
            stats,
            theme,
            truecolor: options.truecolor,
            chosen,
            hints: 0,
            hint_letters: Vec::new(),
            learned,
            card: 0,
            modal: Modal::None,
            message: None,
            restore_end: false,
            pending_stats: None,
            reveal: None,
            shake: None,
            celebrate: None,
            confetti: None,
            sound: Sound::new(options.sound),
            animate: options.animate,
            now: Instant::now(),
            quit: false,
            size: (80, 24),
            repaint: false,
            debug_answer: options.debug_answer,
            seed,
        };
        app.new_game(options.mode);
        app
    }

    /// Starts a game. Practice is always a new random word; the daily puzzle picks up
    /// where it was left, since there is only one word a day.
    pub fn new_game(&mut self, mode: Mode) {
        let day = today();
        let level = self.chosen;
        let fresh = |answer| Game::new(mode, day, answer, level.difficulty()).with_tries(level.tries());
        (self.hints, self.hint_letters) = (0, Vec::new());
        self.game = match mode {
            Mode::Daily => match self.stats.load_daily(day) {
                Some(game) => {
                    self.load_hints(day);
                    game
                }
                None => fresh(daily_answer(day)),
            },
            Mode::Practice => fresh(self.debug_answer.unwrap_or_else(|| self.random_answer())),
        };
        self.modal = Modal::None;
        self.pending_stats = None;
        (self.reveal, self.shake, self.celebrate, self.confetti) = (None, None, None, None);
        self.end_message();
    }

    /// The hints taken for the daily puzzle are kept with the statistics, as
    /// `daily_hints=<day>,<hints>,<spot>,...`, so that leaving and coming back does not
    /// hand the stars back.
    fn save_hints(&mut self) {
        if self.game.mode != Mode::Daily {
            return;
        }
        let mut parts = vec![self.game.day.to_string(), self.hints.to_string()];
        parts.extend(self.hint_letters.iter().map(usize::to_string));
        self.stats.set("daily_hints", parts.join(","));
        self.stats.save();
    }

    fn load_hints(&mut self, day: i64) {
        let saved: Vec<i64> = self.stats.text("daily_hints").unwrap_or_default().split(',').filter_map(|v| v.parse().ok()).collect();
        if let [saved_day, hints, spots @ ..] = saved.as_slice()
            && *saved_day == day
        {
            self.hints = (*hints).clamp(0, MAX_HINTS as i64) as usize;
            self.hint_letters = spots.iter().filter(|&&spot| (0..5).contains(&spot)).map(|&spot| spot as usize).take(MAX_HINTS - 1).collect();
        }
    }

    /// The spot the next hint would give away: one whose letter is neither known from a
    /// green tile nor already given. Spread out, not left to right.
    fn next_hint_spot(&self) -> Option<usize> {
        let green = |spot: usize| self.game.marks.iter().any(|marks| marks[spot] == Mark::Green);
        [0, 2, 4, 1, 3].into_iter().find(|&spot| !self.hint_letters.contains(&spot) && !green(spot))
    }

    /// Whether another hint can be had.
    pub fn more_hints(&self) -> bool {
        self.game.playing() && Level::of(&self.game).hints() && self.hints < MAX_HINTS && (self.hints == 0 || self.next_hint_spot().is_some())
    }

    /// The first hint is what the word means; each one after that gives a letter.
    fn take_hint(&mut self) {
        if !self.more_hints() {
            return;
        }
        if self.hints > 0
            && let Some(spot) = self.next_hint_spot()
        {
            self.hint_letters.push(spot);
        }
        self.hints += 1;
        self.sound.play(Cue::Hint);
        self.save_hints();
    }

    /// The stars this word is worth if it is solved now.
    pub fn stars(&self) -> usize {
        level::stars(self.hints)
    }

    /// The words of the collection, in order.
    pub fn cards(&self) -> Vec<(&str, usize)> {
        self.learned.iter().map(|(word, stars)| (word.as_str(), stars.parse().unwrap_or(0).min(3))).collect()
    }

    /// Counts a finished game. On Easy a word that was not solved costs nothing: it is
    /// not counted as played and does not end a run of solved words. Either way the
    /// word goes into the collection, to be looked at again.
    fn finish(&mut self) {
        let won = self.game.status == Status::Won;
        if won || Level::of(&self.game) != Level::Easy {
            self.stats.record(&self.game);
        }
        let stars = if won { self.stars() } else { 0 };
        if won {
            self.stats.set("stars", self.stats.num("stars") + stars as i64);
            self.stats.save();
        }
        let word = game::text(&self.game.answer).to_lowercase();
        let best = self.learned.get(&word).and_then(|v| v.parse().ok()).unwrap_or(0).max(stars);
        self.learned.insert(word, best.to_string());
        store::write_pairs(&self.stats.dir().join("learned"), &self.learned);
    }

    fn random_answer(&mut self) -> Word {
        // xorshift64: plenty for picking a word.
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        let list = words::answers();
        game::word(list[(self.seed % list.len() as u64) as usize]).expect("answers are five letters")
    }

    /// The puzzle's number, for the daily mode.
    pub fn daily_number(&self) -> i64 {
        self.game.day - DAILY_BASE
    }

    fn small(&self) -> bool {
        layout::layout(self.size.0, self.size.1, self.game.tries).is_none()
    }

    /// Whether an animation is running. While one is, the event loop draws frames and
    /// leaves keys in the queue, so typing ahead is neither lost nor acted on early.
    pub fn animating(&self) -> bool {
        self.reveal.is_some() || self.shake.is_some() || self.celebrate.is_some() || self.confetti.is_some()
    }

    /// When the loop has to wake up without a key: a message expiring, or the
    /// statistics opening.
    pub fn next_deadline(&self) -> Option<Instant> {
        [self.message.as_ref().and_then(|m| m.until), self.pending_stats].into_iter().flatten().min()
    }

    /// Moves time forward: ends animations, expires the message, opens the statistics.
    pub fn tick(&mut self) {
        self.now = Instant::now();
        if self.reveal.is_some_and(|(_, start)| self.now >= start + FLIP * 5) {
            let (row, _) = self.reveal.take().unwrap();
            self.end_sound();
            if self.game.status == Status::Won {
                self.celebrate = Some((row, self.now));
            } else {
                self.after_guess();
            }
        }
        if self.celebrate.is_some_and(|(_, start)| self.now >= start + FLASH_STEP * 6) {
            self.celebrate = None;
            self.confetti = Some(self.now);
            self.after_guess();
        }
        if self.confetti.is_some_and(|start| self.now >= start + CONFETTI) {
            self.confetti = None;
        }
        if self.shake.is_some_and(|start| self.now >= start + SHAKE_STEP * SHAKE_STEPS) {
            self.shake = None;
        }
        if self.message.as_ref().is_some_and(|m| m.until.is_some_and(|t| self.now >= t)) {
            self.message = None;
            if std::mem::take(&mut self.restore_end) {
                self.end_message();
            }
        }
        if self.pending_stats.is_some_and(|t| self.now >= t) {
            self.pending_stats = None;
            if !self.small() {
                self.modal = Modal::Stats;
            }
        }
    }

    fn toast(&mut self, text: impl Into<String>, kind: MessageKind, seconds: u64) {
        self.message = Some(Message { text: text.into(), kind, until: Some(Instant::now() + Duration::from_secs(seconds)) });
    }

    /// A passing message goes when the player types again; a lasting one stays.
    fn clear_toast(&mut self) {
        if self.message.as_ref().is_some_and(|m| m.until.is_some()) {
            self.message = None;
        }
    }

    /// The lasting message of a finished game, or none while it is being played.
    fn end_message(&mut self) {
        const PRAISE: [&str; 8] = ["Genius", "Magnificent", "Amazing", "Splendid", "Great", "Nice", "Well done", "Phew"];
        let (tries, allowed) = (self.game.guesses.len(), self.game.tries);
        let answer = game::text(&self.game.answer);
        self.message = match self.game.status {
            Status::Won => Some((format!("{}! Solved in {tries}/{allowed}", PRAISE[tries.clamp(1, 8) - 1]), MessageKind::Win)),
            Status::Lost if self.game.gave_up => Some((format!("The word was {answer}"), MessageKind::Error)),
            Status::Lost => Some((format!("So close! It was {answer}"), MessageKind::Error)),
            Status::Playing => None,
        }
        .map(|(text, kind)| Message { text, kind, until: None });
    }

    /// The sound of a game that has just ended, if it has.
    fn end_sound(&mut self) {
        match self.game.status {
            Status::Won => self.sound.play(Cue::Won),
            Status::Lost => self.sound.play(Cue::Lost),
            Status::Playing => {}
        }
    }

    /// Once a guess has been revealed: if that ended the game, say so and line up the
    /// statistics.
    fn after_guess(&mut self) {
        if !self.game.playing() {
            self.end_message();
            // The statistics wait for the confetti to land.
            self.pending_stats = Some(Instant::now() + if self.confetti.is_some() { CONFETTI } else { STATS_DELAY });
        }
    }

    fn type_letter(&mut self, letter: u8) {
        if self.game.playing() && self.game.cur.len() < 5 {
            self.clear_toast();
            self.game.cur.push(letter.to_ascii_uppercase());
        }
    }

    fn backspace(&mut self) {
        if self.game.playing() && !self.game.cur.is_empty() {
            self.clear_toast();
            self.game.cur.pop();
        }
    }

    fn refuse(&mut self, why: impl Into<String>, seconds: u64) {
        self.toast(why, MessageKind::Error, seconds);
        self.sound.play(Cue::Refused);
        if self.animate {
            self.shake = Some(Instant::now());
        }
    }

    fn submit(&mut self) {
        if !self.game.playing() {
            return self.act(Action::Stats);
        }
        let Some(guess) = game::word(&String::from_utf8_lossy(&self.game.cur)) else {
            return self.refuse("Type 5 letters first", 2);
        };
        if !words::is_word(&game::text(&guess)) {
            return self.refuse("I don't know that word", 2);
        }
        if let Err(why) = game::check_clues(self.game.difficulty, &self.game.guesses, &self.game.marks, &guess) {
            return self.refuse(why, 3);
        }
        self.clear_toast();
        self.game.cur.clear();
        self.game.add_guess(guess);
        self.stats.save_daily(&self.game);
        if !self.game.playing() {
            self.finish();
        }
        // The tiles are heard as they turn: a note each, in step with the flip. The
        // end of a game has its own sound, which follows the reveal; with animations
        // off there is no reveal to wait for, so it is played in its place.
        let marks = self.game.marks[self.game.guesses.len() - 1];
        if self.animate {
            self.sound.play(Cue::Reveal(marks, FLIP.as_millis() as u32));
            self.reveal = Some((self.game.guesses.len() - 1, Instant::now()));
        } else {
            if self.game.playing() {
                self.sound.play(Cue::Reveal(marks, 70));
            }
            self.end_sound();
            self.after_guess();
        }
    }

    /// The result as text to share: the score and one row of squares per guess.
    pub fn share_text(&self) -> String {
        let game = &self.game;
        let (green, yellow) = if self.theme.name == "contrast" { ("🟧", "🟦") } else { ("🟩", "🟨") };
        let won = game.status == Status::Won;
        let score = if won { game.guesses.len().to_string() } else { "X".to_string() };
        let mut text = match game.mode {
            Mode::Daily => format!("Funwordl #{} {score}/{}", self.daily_number(), game.tries),
            Mode::Practice => format!("Funwordl practice {score}/{}", game.tries),
        };
        if won {
            text.push_str(&format!(" {}", "★".repeat(self.stars())));
        }
        let level = Level::of(game);
        if level != Level::Easy {
            text.push_str(&format!(" ({})", level.name()));
        }
        text.push('\n');
        for marks in &game.marks {
            text.push('\n');
            for mark in marks {
                text.push_str(match mark {
                    game::Mark::Green => green,
                    game::Mark::Yellow => yellow,
                    game::Mark::Gray => "⬛",
                });
            }
        }
        text.push('\n');
        text
    }

    /// Runs one action. Keys and clicks both end up here.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Key(KeyId::Letter(letter)) => self.type_letter(letter),
            Action::Key(KeyId::Enter) => self.submit(),
            Action::Key(KeyId::Back) => self.backspace(),
            Action::Help => (self.modal, self.pending_stats) = (Modal::Help, None),
            Action::Stats => (self.modal, self.pending_stats) = (Modal::Stats, None),
            Action::Close => self.modal = Modal::None,
            Action::New => self.new_game(Mode::Practice),
            Action::Daily => self.new_game(Mode::Daily),
            Action::Theme => {
                self.theme = theme::theme(theme::next_name(self.theme.name), self.truecolor);
                self.stats.set("theme", self.theme.name);
                self.stats.save();
                if self.modal == Modal::None && self.game.playing() && !self.small() {
                    self.toast(format!("Theme: {}", self.theme.name), MessageKind::Plain, 2);
                }
            }
            Action::Difficulty => {
                self.chosen = self.chosen.next();
                self.stats.set("level", self.chosen.index());
                self.stats.save();
                if !self.game.playing() {
                    return;
                }
                // A game under way can be made easier, but not harder: its earlier
                // guesses were not held to the stricter rules, and hints may have been
                // taken.
                let untouched = self.game.guesses.is_empty() && self.hints == 0;
                if self.chosen <= Level::of(&self.game) || untouched {
                    self.chosen.apply(&mut self.game);
                    self.stats.save_daily(&self.game);
                    self.toast(format!("Level: {}", self.chosen.name()), MessageKind::Plain, 2);
                } else {
                    self.toast(format!("{} starts next game", self.chosen.name()), MessageKind::Plain, 3);
                }
            }
            Action::Sound => {
                let on = !self.sound.on && self.sound.available();
                let note = if on {
                    "Sound: on"
                } else if self.sound.on {
                    "Sound: off"
                } else {
                    "No sound player found"
                };
                self.sound.on = on;
                self.sound.play(Cue::On);
                self.stats.set("sound", on as u8);
                self.stats.save();
                if self.modal == Modal::None && !self.small() {
                    self.toast(note, MessageKind::Plain, 2);
                    self.restore_end = !self.game.playing();
                }
            }
            Action::Hint => {
                if !self.game.playing() || self.small() {
                    return;
                }
                if !Level::of(&self.game).hints() {
                    return self.toast("Hints are for Easy and Normal", MessageKind::Plain, 3);
                }
                // Opening the dialog again only shows what was already given; asking
                // from inside it is what takes another hint.
                if self.hints == 0 || self.modal == Modal::Hint {
                    self.take_hint();
                }
                (self.modal, self.pending_stats) = (Modal::Hint, None);
            }
            Action::Words => {
                // After a game the collection opens at the word just met.
                let word = game::text(&self.game.answer).to_lowercase();
                if !self.game.playing()
                    && let Some(at) = self.learned.keys().position(|w| *w == word)
                {
                    self.card = at;
                }
                (self.modal, self.pending_stats) = (Modal::Words, None);
            }
            Action::CardPrev | Action::CardNext => {
                let count = self.learned.len().max(1);
                self.card = (self.card.min(count - 1) + if action == Action::CardNext { 1 } else { count - 1 }) % count;
            }
            // Asks first: one stray key must not end a game.
            Action::GiveUp => {
                if self.game.playing() && !self.small() {
                    (self.modal, self.pending_stats) = (Modal::GiveUp, None);
                }
            }
            // A loss, with the answer shown.
            Action::Surrender => {
                if self.game.playing() {
                    self.modal = Modal::None;
                    self.game.give_up();
                    self.stats.save_daily(&self.game);
                    self.finish();
                    self.end_sound();
                    self.after_guess();
                }
            }
            Action::Copy => {
                if self.game.playing() {
                    return;
                }
                self.modal = Modal::None;
                let copied = copy_to_clipboard(&self.share_text());
                self.toast(if copied { "Result copied" } else { "No clipboard tool found" }, if copied { MessageKind::Plain } else { MessageKind::Error }, 3);
                self.restore_end = true;
            }
            Action::Quit => self.quit = true,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.modifiers.contains(KeyModifiers::ALT) {
            return;
        }
        self.pending_stats = None;
        match key.code {
            KeyCode::Char('q' | 'c') if ctrl => return self.act(Action::Quit),
            KeyCode::Char('l') if ctrl => return self.repaint = true,
            KeyCode::Char('t') if ctrl => return self.act(Action::Theme),
            KeyCode::Char('a') if ctrl => return self.act(Action::Sound),
            _ => {}
        }
        if self.small() {
            return;
        }
        let playing = self.game.playing();
        match (self.modal, key.code) {
            (Modal::None, _) => {}
            // The result of a game stays up until it is answered with N, C or Esc. Enter
            // and letters are pressed once too often at the end of a game, and must not
            // take the result away before it has been read.
            (Modal::Stats, KeyCode::Char('w' | 'W')) => return self.act(Action::Words),
            (Modal::Stats, KeyCode::Char('n' | 'N')) if !playing => return self.act(Action::New),
            (Modal::Stats, KeyCode::Char('c' | 'C')) if !playing => return self.act(Action::Copy),
            (Modal::Stats, KeyCode::Esc) if !playing => return self.act(Action::Close),
            (Modal::Stats, _) if !playing => return,
            (Modal::GiveUp, KeyCode::Enter | KeyCode::Char('y' | 'Y')) => return self.act(Action::Surrender),
            (Modal::Hint, KeyCode::Tab) => return self.act(Action::Hint),
            (Modal::Words, KeyCode::Left | KeyCode::Up) => return self.act(Action::CardPrev),
            (Modal::Words, KeyCode::Right | KeyCode::Down | KeyCode::Char(' ')) => return self.act(Action::CardNext),
            _ => return self.act(Action::Close),
        }
        // Every letter types, so commands are Ctrl keys, Tab or `?`.
        match key.code {
            KeyCode::Tab => self.act(Action::Hint),
            KeyCode::Char('w') if ctrl => self.act(Action::Words),
            KeyCode::Char('n') if ctrl => self.act(Action::New),
            KeyCode::Char('d') if ctrl => self.act(Action::Daily),
            KeyCode::Char('s') if ctrl => self.act(Action::Stats),
            KeyCode::Char('x') if ctrl => self.act(Action::Difficulty),
            KeyCode::Char('g') if ctrl => self.act(Action::GiveUp),
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char(c) if c.is_ascii_alphabetic() => self.type_letter(c as u8),
            KeyCode::Char('?') | KeyCode::F(1) => self.act(Action::Help),
            KeyCode::Enter => self.submit(),
            KeyCode::Backspace | KeyCode::Delete => self.backspace(),
            _ => {}
        }
    }

    /// A left click presses whatever is under it; with a dialog open, a click anywhere
    /// else closes the dialog, except the result of a game, which only its buttons close.
    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        self.pending_stats = None;
        match ui::action_at(self, mouse.column as i32, mouse.row as i32) {
            Some(action) => self.act(action),
            None if self.modal == Modal::Stats && !self.game.playing() => {}
            None if self.modal != Modal::None => self.act(Action::Close),
            None => {}
        }
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { TABLE[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Copies with the first clipboard tool found; failing that, asks the terminal to
/// (OSC 52), which cannot report back, so that counts as done.
fn copy_to_clipboard(text: &str) -> bool {
    let tools: [&[&str]; 5] = [&["wl-copy"], &["pbcopy"], &["xclip", "-selection", "clipboard"], &["xsel", "-ib"], &["clip.exe"]];
    for tool in tools {
        let child = Command::new(tool[0]).args(&tool[1..]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        if let Ok(mut child) = child {
            let written = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
            if child.wait().is_ok_and(|status| status.success()) && written {
                return true;
            }
        }
    }
    let mut out = std::io::stdout().lock();
    write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes())).and_then(|()| out.flush()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app_at(name: &str, level: Option<Level>) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("funwordl-test-{}-app-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (reopen(&dir, level), dir)
    }

    /// The game started again on the same saved files.
    fn reopen(dir: &std::path::Path, level: Option<Level>) -> App {
        let options = Options { mode: Mode::Practice, theme: None, level, animate: false, sound: false, truecolor: true, debug_answer: game::word("CRANE") };
        App::new(Stats::load(dir.to_path_buf()), options)
    }

    /// Most of what is tested here is the Easy level's own.
    fn app(name: &str) -> (App, PathBuf) {
        app_at(name, Some(Level::Easy))
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_word(app: &mut App, word: &str) {
        for c in word.chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
    }

    fn message(app: &App) -> &str {
        app.message.as_ref().map_or("", |m| m.text.as_str())
    }

    #[test]
    fn a_game_is_typed_refused_and_won() {
        let (mut app, dir) = app("win");
        assert_eq!((app.chosen, app.game.tries, app.theme.name), (Level::Easy, 8, theme::DEFAULT));
        // Left to itself the game starts on Normal, and remembers a level once chosen.
        let (fresh, fresh_dir) = app_at("win-default", None);
        assert_eq!((fresh.chosen, fresh.game.tries), (Level::Normal, 6));
        let _ = std::fs::remove_dir_all(fresh_dir);
        type_word(&mut app, "sla");
        assert_eq!(message(&app), "Type 5 letters first");
        app.on_key(key(KeyCode::Backspace));
        assert_eq!(app.game.cur, b"SL");
        type_word(&mut app, "qqq");
        assert_eq!(message(&app), "I don't know that word");
        for _ in 0..5 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_word(&mut app, "slate");
        assert_eq!((app.game.guesses.len(), message(&app)), (1, ""));
        type_word(&mut app, "CRANE");
        assert_eq!(app.game.status, Status::Won);
        assert_eq!(message(&app), "Magnificent! Solved in 2/8");
        assert_eq!((app.stats.of(Mode::Practice, "wins"), app.stats.of(Mode::Practice, "d2"), app.stats.num("stars")), (1, 1, 3));
        assert_eq!(app.cards(), [("crane", 3)]);
        // Enter on a finished game opens the statistics, and more of it changes nothing:
        // the result stays up until it is answered with N, C, W or Esc.
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.modal, Modal::Stats);
        for stray in [KeyCode::Enter, KeyCode::Enter, KeyCode::Char('e'), KeyCode::Char(' '), KeyCode::Backspace, KeyCode::Char('?'), KeyCode::Tab] {
            app.on_key(key(stray));
        }
        app.on_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 0, row: 0, modifiers: KeyModifiers::NONE });
        assert_eq!((app.modal, app.game.status), (Modal::Stats, Status::Won));
        // Esc closes it, Enter brings it back, N starts the next word.
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.modal, Modal::None);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.modal, Modal::Stats);
        app.on_key(key(KeyCode::Char('n')));
        assert_eq!((app.modal, app.game.guesses.len(), app.game.status), (Modal::None, 0, Status::Playing));
        // The statistics of a game under way are only a look: any key closes them.
        app.on_key(ctrl('s'));
        assert_eq!(app.modal, Modal::Stats);
        app.on_key(key(KeyCode::Char('e')));
        assert_eq!((app.modal, app.game.cur.len()), (Modal::None, 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    const WRONG: [&str; 8] = ["slate", "count", "world", "house", "mound", "plant", "brick", "jumpy"];

    #[test]
    fn an_easy_game_has_eight_guesses_and_losing_it_costs_nothing() {
        let (mut app, dir) = app("easy");
        type_word(&mut app, "crane");
        app.on_key(key(KeyCode::Enter));
        app.on_key(key(KeyCode::Char('n')));
        assert_eq!((app.stats.of(Mode::Practice, "played"), app.stats.of(Mode::Practice, "streak")), (1, 1));
        for (i, word) in WRONG.iter().enumerate() {
            assert_eq!(app.game.status, Status::Playing, "before guess {}", i + 1);
            type_word(&mut app, word);
        }
        assert_eq!((app.game.status, message(&app)), (Status::Lost, "So close! It was CRANE"));
        // Not counted, and the run of solved words goes on.
        assert_eq!((app.stats.of(Mode::Practice, "played"), app.stats.of(Mode::Practice, "streak"), app.stats.num("stars")), (1, 1, 3));
        // The stars a word earned are kept when it is met again and not solved.
        assert_eq!(app.cards(), [("crane", 3)]);
        assert!(app.share_text().starts_with("Funwordl practice X/8\n\n⬛⬛🟩⬛🟩\n"), "{}", app.share_text());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_other_levels_play_as_wordl_does() {
        let (mut app, dir) = app_at("normal", Some(Level::Normal));
        type_word(&mut app, "crane");
        app.on_key(key(KeyCode::Enter));
        assert!(app.share_text().starts_with("Funwordl practice 1/6 ★★★ (Normal)\n"), "{}", app.share_text());
        app.on_key(key(KeyCode::Char('n')));
        for word in &WRONG[..6] {
            type_word(&mut app, word);
        }
        assert_eq!(app.game.status, Status::Lost);
        // Counted as played, and the run is over.
        assert_eq!((app.stats.of(Mode::Practice, "played"), app.stats.of(Mode::Practice, "wins"), app.stats.of(Mode::Practice, "streak")), (2, 1, 0));
        // Normal has hints, at the same price in stars as on Easy.
        app.act(Action::New);
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.modal, app.hints, app.stars()), (Modal::Hint, 1, 2));
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.hints, app.hint_letters.len(), app.stars()), (2, 1, 1));
        app.on_key(key(KeyCode::Esc));
        // With a hint taken the game cannot be made harder, only the next one.
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, Level::of(&app.game), message(&app)), (Level::Hard, Level::Normal, "Hard starts next game"));
        type_word(&mut app, "crane");
        assert!(app.share_text().starts_with("Funwordl practice 1/6 ★ (Normal)\n"), "{}", app.share_text());
        // Hard and Ultra Hard are played without.
        for level in [Level::Hard, Level::Ultra] {
            app.chosen = level;
            app.act(Action::New);
            app.on_key(key(KeyCode::Tab));
            assert_eq!((app.modal, app.hints, message(&app)), (Modal::None, 0, "Hints are for Easy and Normal"), "{level:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hints_give_the_meaning_then_letters_and_cost_stars() {
        let (mut app, dir) = app("hints");
        type_word(&mut app, "comet"); // C is green: no hint will spend itself on it
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.modal, app.hints, app.hint_letters.len(), app.stars()), (Modal::Hint, 1, 0, 2));
        // Closing and opening again shows the same hint and takes no new one.
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.modal, app.hints), (Modal::Hint, 1));
        // Tab inside the dialog asks for more: letters, spread out, skipping the green.
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.hints, app.hint_letters.clone(), app.stars()), (2, vec![2], 1));
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.hints, app.hint_letters.clone(), app.more_hints()), (4, vec![2, 4, 1], false));
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.hints, app.stars()), (MAX_HINTS, 1));
        app.on_key(key(KeyCode::Char('x')));
        assert_eq!((app.modal, app.game.cur.len()), (Modal::None, 0));
        type_word(&mut app, "crane");
        assert_eq!((app.stats.num("stars"), app.cards()), (1, vec![("crane", 1)]));
        assert!(app.share_text().starts_with("Funwordl practice 2/8 ★\n"));
        // The next word starts with no hints taken.
        app.act(Action::New);
        assert_eq!((app.hints, app.hint_letters.len(), app.stars()), (0, 0, 3));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_level_can_ease_mid_game_but_not_tighten() {
        let (mut app, dir) = app("level");
        // Nothing done yet: any level takes effect at once.
        app.on_key(ctrl('x'));
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, Level::of(&app.game), app.game.tries, message(&app)), (Level::Hard, Level::Hard, 6, "Level: Hard"));
        type_word(&mut app, "slate");
        type_word(&mut app, "count");
        assert_eq!(message(&app), "3rd letter must be A");
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, Level::of(&app.game), message(&app)), (Level::Ultra, Level::Hard, "Ultra Hard starts next game"));
        // Round to Easy: easier, so at once, with its two extra guesses.
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, Level::of(&app.game), app.game.tries), (Level::Easy, Level::Easy, 8));
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, Level::of(&app.game), message(&app)), (Level::Normal, Level::Easy, "Normal starts next game"));
        // The choice is remembered, Easy too.
        assert_eq!(reopen(&dir, None).chosen, Level::Normal);
        app.on_key(ctrl('x'));
        app.on_key(ctrl('x'));
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, reopen(&dir, None).chosen), (Level::Easy, Level::Easy));
        // A hint counts as having started, too.
        let (mut app, dir2) = app_at("level-hint", Some(Level::Easy));
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Esc));
        app.on_key(ctrl('x'));
        assert_eq!((app.chosen, Level::of(&app.game)), (Level::Normal, Level::Easy));
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[test]
    fn giving_up_asks_first_and_shows_the_word() {
        let (mut app, dir) = app_at("giveup", Some(Level::Normal));
        type_word(&mut app, "slate");
        app.on_key(ctrl('g'));
        assert_eq!((app.modal, app.game.status), (Modal::GiveUp, Status::Playing));
        app.on_key(key(KeyCode::Esc));
        assert_eq!((app.modal, app.game.status), (Modal::None, Status::Playing));
        app.on_key(ctrl('g'));
        app.on_key(key(KeyCode::Enter));
        assert_eq!((app.modal, app.game.status, app.game.gave_up), (Modal::None, Status::Lost, true));
        assert_eq!(message(&app), "The word was CRANE");
        assert_eq!((app.stats.of(Mode::Practice, "played"), app.stats.of(Mode::Practice, "wins")), (1, 0));
        // The word is kept to look at again, with no stars.
        assert_eq!(app.cards(), [("crane", 0)]);
        // Not twice.
        app.act(Action::Surrender);
        assert_eq!(app.stats.of(Mode::Practice, "played"), 1);
        app.act(Action::New);
        assert_eq!((app.game.status, app.game.gave_up), (Status::Playing, false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_daily_puzzle_keeps_its_guesses_hints_and_level() {
        let (mut app, dir) = app("daily");
        app.on_key(ctrl('d'));
        assert_eq!((app.game.mode, app.game.tries), (Mode::Daily, 8));
        assert_eq!(app.daily_number(), today() - DAILY_BASE);
        // Whatever today's word is, this is a valid first guess or the answer itself.
        type_word(&mut app, "slate");
        if app.game.playing() {
            app.on_key(key(KeyCode::Tab));
            app.on_key(key(KeyCode::Tab));
            let letters = app.hint_letters.clone();
            assert_eq!((app.hints, letters.len()), (2, 1));
            // Another start of the game, even one that asks for a harder level.
            let mut app = reopen(&dir, Some(Level::Hard));
            app.on_key(ctrl('d'));
            assert_eq!((app.game.guesses.len(), app.game.tries, Level::of(&app.game)), (1, 8, Level::Easy));
            assert_eq!((app.hints, app.hint_letters.clone()), (2, letters));
            app.act(Action::Surrender);
            app.on_key(ctrl('n'));
            app.on_key(ctrl('d'));
            assert_eq!((app.game.status, app.game.gave_up, app.game.guesses.len()), (Status::Lost, true, 1));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_daily_word_is_fixed_for_a_day_and_is_not_wordls() {
        assert_eq!(daily_answer(20733), daily_answer(20733));
        assert!(words::is_word(&game::text(&daily_answer(-5))));
        let same = (20733..20833).filter(|&day| daily_answer(day) == game::daily_answer(day)).count();
        assert!(same <= 2, "{same} of 100 days share wordl's word");
    }

    #[test]
    fn the_collection_is_kept_and_leafed_through() {
        let (mut app, dir) = app("words");
        app.on_key(ctrl('w'));
        assert_eq!((app.modal, app.cards().len()), (Modal::Words, 0));
        // Turning pages of an empty collection does nothing, and any other key closes it.
        app.on_key(key(KeyCode::Right));
        app.on_key(key(KeyCode::Char('q')));
        assert_eq!((app.modal, app.card), (Modal::None, 0));
        for word in ["CRANE", "ABOUT", "ZEBRA"] {
            app.debug_answer = game::word(word);
            app.act(Action::New);
            type_word(&mut app, word);
        }
        assert_eq!(app.cards(), [("about", 3), ("crane", 3), ("zebra", 3)]);
        // After a game it opens at the word just met; the arrows go round.
        app.on_key(key(KeyCode::Enter));
        app.on_key(key(KeyCode::Char('w')));
        assert_eq!((app.modal, app.card), (Modal::Words, 2));
        app.on_key(key(KeyCode::Right));
        assert_eq!(app.card, 0);
        app.on_key(key(KeyCode::Left));
        app.on_key(key(KeyCode::Left));
        assert_eq!((app.modal, app.card), (Modal::Words, 1));
        // Read back by the next start of the game.
        assert_eq!(reopen(&dir, None).cards().len(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn themes_cycle_and_are_remembered() {
        let (mut app, dir) = app("theme");
        let next = theme::next_name(theme::DEFAULT);
        app.on_key(ctrl('t'));
        assert_eq!((app.theme.name, message(&app)), (next, format!("Theme: {next}").as_str()));
        assert_eq!(reopen(&dir, None).theme.name, next);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sounds_follow_what_happens_in_the_game() {
        let (mut app, dir) = app("sound");
        // Off, as in every other test: nothing is asked of it.
        type_word(&mut app, "qqqqq");
        assert!(app.sound.log.is_empty());
        app.sound.on = true;
        for _ in 0..5 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_word(&mut app, "qqqqq");
        for _ in 0..5 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_word(&mut app, "slate");
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Esc));
        // Looking at the hint again is not a new hint, and makes no sound.
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Esc));
        type_word(&mut app, "crane");
        let slate = game::evaluate(&game::word("SLATE").unwrap(), &game::word("CRANE").unwrap());
        // Animations are off here, so the winning guess goes straight to the fanfare.
        assert_eq!(app.sound.log, [Cue::Refused, Cue::Reveal(slate, 70), Cue::Hint, Cue::Won]);

        // With animations on, the tiles are heard at the pace they flip, and the end of
        // the game is heard once they have.
        app.sound.log.clear();
        app.animate = true;
        app.act(Action::New);
        type_word(&mut app, "crane");
        assert_eq!(app.sound.log, [Cue::Reveal([game::Mark::Green; 5], FLIP.as_millis() as u32)]);
        std::thread::sleep(FLIP * 5 + Duration::from_millis(30));
        app.tick();
        assert_eq!(app.sound.log.last(), Some(&Cue::Won));
        app.act(Action::New);
        app.act(Action::Surrender);
        assert_eq!(app.sound.log.last(), Some(&Cue::Lost));

        // Ctrl-A is the switch. The tests have no player, so it cannot be switched on,
        // and says so; switching off always works, and is remembered.
        app.act(Action::New);
        app.on_key(ctrl('a'));
        assert_eq!((app.sound.on, message(&app), app.stats.text("sound")), (false, "Sound: off", Some("0")));
        app.on_key(ctrl('a'));
        assert_eq!((app.sound.on, message(&app)), (false, "No sound player found"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
