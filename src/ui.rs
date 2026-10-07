//! All drawing. Pure functions of `&App`: nothing here changes state.
//!
//! The whole screen is redrawn every frame from the state and the terminal size;
//! ratatui sends only the cells that changed. Where things are clicked is worked out by
//! the same geometry functions that draw them, so the two cannot drift apart.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};

use crate::app::{Action, App, CONFETTI, FLASH_STEP, FLIP, KeyId, MessageKind, Modal, SHAKE_STEP};
use crate::font::{self, Label};
use crate::game::{self, Mark, Mode, Status};
use crate::layout::{self, Layout, TITLE_LETTERS};
use crate::level::Level;
use crate::theme::{self, Paint, Shades, Theme};
use crate::words;

const TITLE: &str = "FUNWORDL";
const KEY_ROWS: [&str; 3] = ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"];
/// Dialog lines are at most this wide, so a dialog fits the narrowest supported
/// terminal (39 columns) with its frame and padding.
const DIALOG_WIDTH: i32 = 35;

fn on(fg: Color, bg: Color) -> Style {
    Style::new().fg(fg).bg(bg)
}

fn width(s: &str) -> i32 {
    s.chars().count() as i32
}

/// `s` in the middle of `w` cells.
fn centered(s: &str, w: i32) -> String {
    let left = ((w - width(s)) / 2).max(0);
    let right = (w - width(s) - left).max(0);
    format!("{}{s}{}", " ".repeat(left as usize), " ".repeat(right as usize))
}

struct Canvas<'a> {
    buf: &'a mut Buffer,
    th: &'a Theme,
}

impl Canvas<'_> {
    /// Writes `s` one cell per character, replacing whatever was there. Anything off
    /// screen is dropped, so callers never have to clip.
    fn put(&mut self, x: i32, y: i32, s: &str, style: Style) {
        if y < 0 {
            return;
        }
        for (i, ch) in s.chars().enumerate() {
            let cx = x + i as i32;
            if cx >= 0
                && let Some(cell) = self.buf.cell_mut((cx as u16, y as u16))
            {
                cell.reset();
                cell.set_char(ch).set_style(style);
            }
        }
    }

    /// Colors one pixel, half a cell, leaving the other half of the cell as it is. Only
    /// cells that are empty or already hold two pixels can take one: text is left alone.
    fn pixel(&mut self, x: i32, row: i32, color: Color) {
        if x < 0 || row < 0 {
            return;
        }
        let Some(cell) = self.buf.cell_mut((x as u16, (row / 2) as u16)) else { return };
        let upper = row % 2 == 0;
        match (cell.symbol().chars().next(), upper) {
            (Some(' '), _) => {
                cell.set_char(if upper { '▀' } else { '▄' }).set_fg(color);
            }
            // The character's color is the half it draws; the background is the other.
            (Some('▀'), true) | (Some('▄'), false) => {
                cell.set_fg(color);
            }
            (Some('▀'), false) | (Some('▄'), true) => {
                cell.set_bg(color);
            }
            _ => {}
        }
    }

    /// `s` centered in the `w` cells from `x0`, cut short if it is longer.
    fn center(&mut self, y: i32, x0: i32, w: i32, s: &str, style: Style) {
        let s: String = s.chars().take(w.max(0) as usize).collect();
        self.put(x0 + (w - width(&s)) / 2, y, &s, style);
    }

    /// Draws one tile or key at a size level's height `t`:
    ///   1    one row of text on a colored background
    ///   3    half-block edges around one row of text
    ///   5+   half-block edges around a bitmap glyph
    ///
    /// The first and last rows are half rows (`▄` on top, `▀` at the bottom, in the
    /// block's color on the screen background), which is what puts a gap between
    /// blocks without spending a row on it. `fill` paints the block in `bg`; otherwise
    /// it is a frame in `bg` around the label. `inset` squashes the block vertically,
    /// for the flip animation.
    ///
    /// A filled block is drawn raised, as pixel art, where the theme has shades for it
    /// (see `sprite`); `sunken` presses it in instead.
    #[allow(clippy::too_many_arguments)]
    fn block(&mut self, x: i32, y: i32, w: i32, t: i32, fill: bool, bg: Paint, fg: Paint, label: Label, inset: i32, sunken: bool) {
        let screen = self.th.bg.bg;
        if t < 3 {
            return self.put(x, y, &centered(&label.text(w), w), on(fg.fg, bg.bg).bold());
        }
        let (top, bottom) = (inset, t - 1 - inset);
        let edge = on(bg.fg, screen);
        if bottom < top {
            return;
        }
        if bottom == top {
            return self.put(x, y + top, &"━".repeat(w as usize), edge);
        }
        let shades = self.th.shades(bg).filter(|_| fill);
        if let Some(shades) = shades
            && t >= 5
        {
            return self.sprite(x, y, w, t, bg, fg, label, inset, sunken, shades);
        }
        // With one row of text there is no room for more than a lit top edge and a
        // dark bottom one.
        let (above, below) = match shades {
            Some(s) if sunken => (s.dark, s.light),
            Some(s) => (s.light, s.dark),
            None => (bg.fg, bg.fg),
        };
        self.put(x, y + top, &"▄".repeat(w as usize), on(above, screen));
        self.put(x, y + bottom, &"▀".repeat(w as usize), on(below, screen));
        let rows = if t == 3 { vec![centered(&label.text(w), w)] } else { font::glyph(label, w, t) };
        for i in top + 1..bottom {
            let Some(row) = rows.get((i - 1) as usize) else { continue };
            if fill {
                self.put(x, y + i, row, on(fg.fg, bg.bg).bold());
            } else {
                let inner: String = row.chars().skip(1).take((w - 2).max(0) as usize).collect();
                self.put(x, y + i, "█", edge);
                self.put(x + 1, y + i, &inner, on(fg.fg, screen).bold());
                self.put(x + w - 1, y + i, "█", edge);
            }
        }
    }

    /// A filled block as pixel art: lit top and left edges, dark bottom and right
    /// edges, and a letter that casts a shadow down and to the right.
    ///
    /// Every pixel has its own color. A cell holds two of them, one above the other:
    /// it is drawn as `▀` in the color of the upper pixel on a background in the color
    /// of the lower one. The block's first and last pixel rows are the screen's, as
    /// for flat blocks, which keeps the gap between blocks.
    #[allow(clippy::too_many_arguments)]
    fn sprite(&mut self, x: i32, y: i32, w: i32, t: i32, bg: Paint, fg: Paint, label: Label, inset: i32, sunken: bool, shades: Shades) {
        let screen = self.th.bg.bg;
        let ink = font::pixels(label, w, t);
        let inked = |col: i32, row: i32| row >= 2 && col >= 0 && ink.get((row - 2) as usize).and_then(|r| r.get(col as usize)).copied().unwrap_or(false);
        let (lit, unlit) = if sunken { (shades.dark, shades.light) } else { (shades.light, shades.dark) };
        // Big blocks get a thicker edge and a longer shadow, so the look scales.
        let edge = if t >= 10 { 2 } else { 1 };
        let reach = (font::scale_for(t) + 1) / 2;
        // A light letter throws a dark shadow; a dark letter on a bright tile gets a
        // light one instead, and reads as engraved.
        let cast = if theme::brightness(fg) >= theme::brightness(bg) { shades.shadow } else { shades.light };
        let (first, last) = (1 + 2 * inset, 2 * t - 2 - 2 * inset);
        let color_at = |col: i32, row: i32| {
            if row < first || row > last {
                screen
            } else if inked(col, row) {
                fg.fg
            } else if row - first < edge || col < edge {
                lit
            } else if last - row < edge || w - 1 - col < edge {
                unlit
            } else if !sunken && inked(col - reach, row - reach) {
                cast
            } else {
                bg.bg
            }
        };
        for i in inset..t - inset {
            for col in 0..w {
                let (upper, lower) = (color_at(col, 2 * i), color_at(col, 2 * i + 1));
                self.put(x + col, y + i, if upper == lower { " " } else { "▀" }, on(upper, lower));
            }
        }
    }
}

/// Breaks text into lines of at most `width` characters, at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

fn mark_paints(th: &Theme, mark: Mark) -> (Paint, Paint) {
    match mark {
        Mark::Green => (th.g, th.gfg),
        Mark::Yellow => (th.y, th.yfg),
        Mark::Gray => (th.x, th.xfg),
    }
}

/// The line under the title: which game this is.
fn info_text(app: &App) -> String {
    let game = &app.game;
    let mut s = match game.mode {
        Mode::Daily => format!("Daily #{}", app.daily_number()),
        Mode::Practice => "Practice".to_string(),
    };
    let level = Level::of(game);
    if level != Level::Easy {
        s += &format!(" · {}", level.name());
    }
    let stars = app.stats.num("stars");
    if stars > 0 {
        s += &format!(" · ★ {stars}");
    }
    let streak = app.stats.of(game.mode, "streak");
    if streak > 1 {
        s += &format!(" · {streak} in a row");
    }
    s
}

/// The layout for the game being played: an Easy game's board has two more rows.
fn layout_for(app: &App, cols: i32, rows: i32) -> Option<Layout> {
    layout::layout(cols, rows, app.game.tries)
}

fn draw_title(c: &mut Canvas, app: &App, l: &Layout) {
    let th = c.th;
    let info = info_text(app);
    let dim = on(th.dim.fg, th.bg.bg);
    let (party, ink) = th.party();
    let (tw, _) = l.title.grid(TITLE_LETTERS, 1);
    let mut x = l.hx;
    match l.info_y {
        Some(y) => c.center(y, l.rx, l.rw, &info, dim),
        // On one row: the title, two spaces, the info, centered together when they fit.
        None if tw + 2 + width(&info) <= l.rw => {
            x = l.rx + (l.rw - tw - 2 - width(&info)) / 2;
            c.put(x + tw + 2, l.hy, &info, dim);
        }
        // Too narrow for both: the info matters more than the tiles, so the title is
        // written small, one colored letter per cell.
        None if TITLE_LETTERS + 2 + width(&info) <= l.rw => {
            x = l.rx + (l.rw - TITLE_LETTERS - 2 - width(&info)) / 2;
            for (i, letter) in TITLE.chars().enumerate() {
                c.put(x + i as i32, l.hy, &letter.to_string(), on(party[i].fg, th.bg.bg).bold());
            }
            return c.put(x + TITLE_LETTERS + 2, l.hy, &info, dim);
        }
        None => {}
    }
    for (i, letter) in TITLE.bytes().enumerate() {
        c.block(x + i as i32 * l.title.px, l.hy, l.title.w, l.title.t, true, party[i], ink, Label::Letter(letter), 0, false);
    }
}

/// How far the row being typed is pushed sideways by the shake after a refused guess.
fn shake_offset(app: &App, l: &Layout) -> i32 {
    let Some(start) = app.shake else { return 0 };
    let (bw, _) = l.board.grid(5, 1);
    let room = 2.min(l.bx).min(l.cols - l.bx - bw);
    if room < 1 {
        return 0;
    }
    let step = (app.now.saturating_duration_since(start).as_millis() / SHAKE_STEP.as_millis()) as usize;
    [-room, room, -room, room, -1, 1, 0].get(step).copied().unwrap_or(0)
}

fn draw_tile(c: &mut Canvas, app: &App, l: &Layout, row: usize, col: usize) {
    let (th, game) = (c.th, &app.game);
    let guessed = game.guesses.len();
    let d = l.board;
    let mut x = l.bx + col as i32 * d.px;
    let y = l.by + row as i32 * d.py;
    if row == guessed {
        x += shake_offset(app, l);
    }

    // A guess is revealed one tile at a time: each squashes flat in its typed look,
    // then grows back in its color.
    let (mut revealed, mut inset) = (row < guessed, 0);
    if let Some((reveal_row, start)) = app.reveal
        && reveal_row == row
    {
        let elapsed = app.now.saturating_duration_since(start).as_millis();
        let turning = (elapsed / FLIP.as_millis()) as usize;
        let phase = (elapsed % FLIP.as_millis()) as f32 / FLIP.as_millis() as f32;
        let half = (d.t / 2) as f32;
        if col > turning {
            revealed = false;
        } else if col == turning {
            revealed = phase >= 0.5;
            inset = (half * 2.0 * if revealed { 1.0 - phase } else { phase }).round() as i32;
        }
    }

    if revealed {
        let (mut bg, mut fg) = mark_paints(th, game.marks[row][col]);
        // The flash that runs along a winning row.
        if let Some((flash_row, start)) = app.celebrate
            && flash_row == row
            && (app.now.saturating_duration_since(start).as_millis() / FLASH_STEP.as_millis()) as usize == col
        {
            (bg, fg) = (th.win, th.gfg);
        }
        return c.block(x, y, d.w, d.t, true, bg, fg, Label::Letter(game.guesses[row][col]), inset, false);
    }
    let letter = if row < guessed {
        Some(game.guesses[row][col])
    } else if row == guessed && game.gave_up {
        // After giving up, the answer is shown in the row the next guess would have used.
        Some(game.answer[col])
    } else if row == guessed {
        game.cur.get(col).copied()
    } else {
        None
    };
    // A letter given by a hint waits, faintly, in its spot of the row being typed.
    let hinted = (letter.is_none() && row == guessed && game.playing() && app.hint_letters.contains(&col)).then_some(game.answer[col]);
    let ink = if hinted.is_some() { th.dim } else { th.fg };
    let label = letter.or(hinted).map_or(Label::None, Label::Letter);
    if d.t == 1 {
        c.block(x, y, d.w, 1, true, th.empty, ink, label, 0, false);
    } else {
        c.block(x, y, d.w, d.t, false, if letter.is_some() { th.typed } else { th.empty }, ink, label, inset, false);
    }
}

/// Every key of the on-screen keyboard as (x, y, width, key). Backspace is bottom left
/// and Enter bottom right.
pub fn keyboard_keys(l: &Layout) -> Vec<(i32, i32, i32, KeyId)> {
    let d = l.keys;
    let gap = d.px - d.w;
    let mut keys = Vec::with_capacity(28);
    for (r, letters) in KEY_ROWS.iter().enumerate() {
        let y = l.ky + r as i32 * d.py;
        let mut x = l.kx + if r == 1 { d.px / 2 } else { 0 };
        if r == 2 {
            keys.push((x, y, l.wide_left, KeyId::Back));
            x += l.wide_left + gap;
        }
        for letter in letters.bytes() {
            keys.push((x, y, d.w, KeyId::Letter(letter)));
            x += d.px;
        }
        if r == 2 {
            keys.push((x, y, l.wide_right, KeyId::Enter));
        }
    }
    keys
}

fn draw_keyboard(c: &mut Canvas, app: &App, l: &Layout) {
    let th = c.th;
    // A key takes its color once the guess that earned it has been revealed.
    let shown = app.game.guesses.len() - app.reveal.is_some() as usize;
    for (x, y, w, key) in keyboard_keys(l) {
        let (label, mark) = match key {
            KeyId::Letter(letter) => (Label::Letter(letter), app.game.key_mark(letter, shown)),
            KeyId::Enter => (Label::Enter, None),
            KeyId::Back => (Label::Back, None),
        };
        let (bg, fg) = match mark {
            Some(Mark::Gray) => (th.keyx, th.keyxfg),
            Some(mark) => mark_paints(th, mark),
            None => (th.key, th.keyfg),
        };
        // A key known not to be in the word looks pressed in.
        c.block(x, y, w, l.keys.t, true, bg, fg, label, 0, mark == Some(Mark::Gray));
    }
}

fn draw_message(c: &mut Canvas, app: &App, l: &Layout) {
    let th = c.th;
    let Some(message) = &app.message else { return };
    let (text, style) = match message.kind {
        MessageKind::Win => (format!(" {} ", message.text), on(th.gfg.fg, th.g.bg)),
        MessageKind::Error => (format!(" {} ", message.text), on(th.toastfg.fg, th.toast.bg)),
        MessageKind::Plain => (message.text.clone(), on(th.fg.fg, th.bg.bg)),
    };
    c.center(l.msg_y, l.rx, l.rw, &text, style.bold());
}

pub struct FooterItem {
    pub x: i32,
    pub key: &'static str,
    pub label: String,
    pub action: Action,
}

/// The footer's buttons, positioned for a terminal `cols` wide, and whether there is
/// room for their labels (otherwise only the keys are shown).
pub fn footer_items(app: &App, cols: i32) -> (Vec<FooterItem>, bool) {
    let level = app.chosen.name().split(' ').next().unwrap_or_default();
    let items = [
        ("?", "Help", Action::Help),
        ("Tab", "Hint", Action::Hint),
        ("^N", "New", Action::New),
        ("^D", "Daily", Action::Daily),
        ("^S", "Stars", Action::Stats),
        ("^T", "Theme", Action::Theme),
        ("^X", level, Action::Difficulty),
        ("^G", "Reveal", Action::GiveUp),
        ("^Q", "Quit", Action::Quit),
    ];
    let count = items.len() as i32;
    let with_labels: i32 = items.iter().map(|(key, label, _)| width(key) + 1 + width(label)).sum();
    let keys_only: i32 = items.iter().map(|(key, ..)| width(key)).sum();
    let (labels, sep, total) = [3, 2, 1]
        .into_iter()
        .map(|sep| (true, sep, with_labels + sep * (count - 1)))
        .find(|&(_, _, total)| total <= cols - 2)
        .unwrap_or((false, 2, keys_only + 2 * (count - 1)));
    let mut x = (cols - total) / 2;
    let placed = items
        .into_iter()
        .map(|(key, label, action)| {
            let item = FooterItem { x, key, label: label.to_string(), action };
            x += width(key) + if labels { 1 + width(label) } else { 0 } + sep;
            item
        })
        .collect();
    (placed, labels)
}

fn draw_footer(c: &mut Canvas, app: &App, l: &Layout) {
    let th = c.th;
    let (items, labels) = footer_items(app, l.cols);
    for item in items {
        c.put(item.x, l.footer_y, item.key, on(th.accent.fg, th.bg.bg).bold());
        if !labels {
            continue;
        }
        // The level's name is colored when it is one of the hard ones.
        let style = match (item.action, app.chosen) {
            (Action::Difficulty, Level::Hard) => on(th.g.fg, th.bg.bg).bold(),
            (Action::Difficulty, Level::Ultra) => on(th.y.fg, th.bg.bg).bold(),
            _ => on(th.dim.fg, th.bg.bg),
        };
        c.put(item.x + width(item.key) + 1, l.footer_y, &item.label, style);
    }
}

/// One line of a dialog: styled pieces, centered or from the left.
struct DialogLine {
    spans: Vec<(String, Style)>,
    center: bool,
}

impl DialogLine {
    fn width(&self) -> i32 {
        self.spans.iter().map(|(s, _)| width(s)).sum()
    }
}

struct Dialog {
    lines: Vec<DialogLine>,
    buttons: Vec<(&'static str, Action)>,
}

/// Three stars, the first `earned` of them lit.
fn star_spans(earned: usize, lit: Style, unlit: Style) -> Vec<(String, Style)> {
    vec![("★".repeat(earned), lit), ("☆".repeat(3 - earned.min(3)), unlit)]
}

fn dialog(app: &App) -> Option<Dialog> {
    let th = &app.theme;
    let panel = th.panel.bg;
    let text = on(th.fg.fg, panel);
    let dim = on(th.dim.fg, panel);
    let accent = on(th.accent.fg, panel).bold();
    let gold = on(th.y.fg, panel).bold();
    let line = |spans: Vec<(String, Style)>| DialogLine { spans, center: false };
    let middle = |s: &str, style: Style| DialogLine { spans: vec![(s.to_string(), style)], center: true };
    let centered = |spans: Vec<(String, Style)>| DialogLine { spans, center: true };
    let blank = || DialogLine { spans: Vec::new(), center: false };
    let game = &app.game;
    let answer = game::text(&game.answer);
    let meaning = words::definition(&answer).unwrap_or_default();

    Some(match app.modal {
        Modal::None => return None,
        Modal::Help => {
            let mut lines = vec![middle("HOW TO PLAY", accent), blank()];
            for s in ["Guess the hidden 5-letter word.", "The tiles show how close you are:"] {
                lines.push(line(vec![(s.to_string(), text)]));
            }
            lines.push(blank());
            for (mark, meaning) in
                [(Mark::Green, "right letter, right spot"), (Mark::Yellow, "right letter, wrong spot"), (Mark::Gray, "letter is not in the word")]
            {
                let (bg, fg) = mark_paints(th, mark);
                lines.push(line(vec![(" A ".to_string(), on(fg.fg, bg.bg).bold()), (format!("  {meaning}"), text)]));
            }
            lines.push(blank());
            for (k1, d1, k2, d2) in [
                ("A-Z", "type", "Enter", "guess"),
                ("Bksp", "delete", "Tab", "hint"),
                ("^N", "new word", "^D", "daily puzzle"),
                ("^S", "stars", "^W", "my words"),
                ("^T", "theme", "^X", "level"),
                ("^G", "show the word", "^Q", "quit"),
            ] {
                let pad = " ".repeat((17 - width(k1) - width(d1)).max(0) as usize);
                lines.push(line(vec![(k1.to_string(), accent), (format!(" {d1}{pad}"), text), (k2.to_string(), accent), (format!(" {d2}"), text)]));
            }
            lines.push(blank());
            for (level, rule) in [("Easy", "8 guesses, and hints"), ("Normal", "6 guesses"), ("Hard", "reuse green and yellow"), ("Ultra", "obey every clue")] {
                lines.push(line(vec![(format!("{level:<8}"), text.bold()), (rule.to_string(), dim)]));
            }
            Dialog { lines, buttons: vec![("Esc Close", Action::Close)] }
        }
        Modal::Stats => {
            let mode = game.mode;
            let title = match game.status {
                Status::Won => "YOU DID IT!".to_string(),
                Status::Lost => "NICE TRY".to_string(),
                Status::Playing => format!("STARS · {}", mode.key().to_uppercase()),
            };
            let mut lines = vec![middle(&title, accent), blank()];
            // Once a game is over the word is given with what it means: the players are
            // children, and this is where a new word gets learned.
            match game.status {
                Status::Won => {
                    let mut spans = star_spans(app.stars(), gold, dim);
                    spans.push((format!("  Solved in {}/{}", game.guesses.len(), game.tries), on(th.g.fg, panel).bold()));
                    lines.push(centered(spans));
                    // "CRANE: a tall bird..." with the word picked out on the first line.
                    for (i, row) in wrap(&format!("{answer}: {meaning}"), DIALOG_WIDTH as usize).into_iter().enumerate() {
                        match row.strip_prefix(answer.as_str()).filter(|_| i == 0 && !meaning.is_empty()) {
                            Some(rest) => lines.push(centered(vec![(answer.clone(), gold), (rest.to_string(), text)])),
                            None if !meaning.is_empty() => lines.push(middle(&row, text)),
                            None => {}
                        }
                    }
                    lines.push(blank());
                }
                Status::Lost => {
                    lines.push(centered(vec![("The word was ".to_string(), text), (answer.clone(), gold)]));
                    lines.extend(wrap(meaning, DIALOG_WIDTH as usize).iter().map(|row| middle(row, text)));
                    lines.push(blank());
                }
                Status::Playing => {}
            }
            lines.push(middle(&format!("{:>7} {:>8} {:>6} {:>6}", "Solved", "In a row", "Best", "Stars"), dim));
            let row = format!(
                "{:>7} {:>8} {:>6} {:>6}",
                app.stats.of(mode, "wins"),
                app.stats.of(mode, "streak"),
                app.stats.of(mode, "best"),
                app.stats.num("stars")
            );
            lines.extend([middle(&row, text.bold()), blank(), middle(&format!("Words in your collection: {}", app.learned.len()), dim)]);
            let buttons = match game.status {
                Status::Playing => vec![("W Words", Action::Words), ("Esc Close", Action::Close)],
                _ => vec![("N New", Action::New), ("C Copy", Action::Copy), ("W Words", Action::Words), ("Esc", Action::Close)],
            };
            Dialog { lines, buttons }
        }
        Modal::GiveUp => {
            let what = match Level::of(game) {
                Level::Easy => ["The word will be shown, and kept", "in your words to look at again."],
                _ => ["The word will be shown and the", "game counts as not solved."],
            };
            Dialog {
                lines: vec![middle("SHOW THE WORD?", accent), blank(), middle(what[0], text), middle(what[1], text)],
                buttons: vec![("Enter Show it", Action::Surrender), ("Esc Keep playing", Action::Close)],
            }
        }
        Modal::Hint => {
            let mut lines = vec![middle("HINT", accent), blank(), middle("What the word means:", dim)];
            lines.extend(wrap(meaning, DIALOG_WIDTH as usize).iter().map(|row| middle(row, text.bold())));
            if !app.hint_letters.is_empty() {
                let letters: Vec<String> =
                    (0..5).map(|i| if app.hint_letters.contains(&i) { (game.answer[i] as char).to_string() } else { "_".to_string() }).collect();
                lines.extend([blank(), centered(vec![("Letters:  ".to_string(), dim), (letters.join(" "), gold)])]);
            }
            let mut stars = vec![("Stars if you solve it: ".to_string(), dim)];
            stars.extend(star_spans(app.stars(), gold, dim));
            lines.extend([blank(), centered(stars)]);
            let buttons = if app.more_hints() { vec![("Tab One more", Action::Hint), ("Esc Back", Action::Close)] } else { vec![("Esc Back", Action::Close)] };
            Dialog { lines, buttons }
        }
        Modal::Words => {
            let cards = app.cards();
            let Some(&(word, stars)) = cards.get(app.card.min(cards.len().saturating_sub(1))) else {
                let lines =
                    vec![middle("MY WORDS", accent), blank(), middle("No words yet. Finish a game and", text), middle("its word is kept here to learn.", text)];
                return Some(Dialog { lines, buttons: vec![("Esc Close", Action::Close)] });
            };
            let at = app.card.min(cards.len() - 1) + 1;
            let mut name = vec![(format!("{}  ", word.to_uppercase()), gold)];
            name.extend(star_spans(stars, gold, dim));
            let mut lines = vec![middle(&format!("MY WORDS · {at} of {}", cards.len()), accent), blank(), centered(name), blank()];
            // Always three lines of meaning, so the buttons stay put from word to word.
            let mut rows = wrap(words::definition(word).unwrap_or_default(), DIALOG_WIDTH as usize);
            rows.resize(3, String::new());
            lines.extend(rows.iter().map(|row| middle(row, text)));
            Dialog { lines, buttons: vec![("< Back", Action::CardPrev), ("> Next", Action::CardNext), ("Esc Close", Action::Close)] }
        }
    })
}

/// Where a dialog and its buttons are for a terminal of `cols` by `rows`.
struct DialogBox {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    inner: i32,
    /// Lines that fit; a dialog taller than the terminal loses its last ones.
    shown: usize,
    /// Each button as (x, y, label, action); it is drawn as the label with a space on
    /// either side.
    buttons: Vec<(i32, i32, &'static str, Action)>,
}

fn dialog_box(dialog: &Dialog, cols: i32, rows: i32) -> DialogBox {
    let inner = dialog.lines.iter().map(DialogLine::width).max().unwrap_or(0).max(DIALOG_WIDTH);
    let w = inner + 4;
    let shown = (dialog.lines.len() as i32).min(rows - 4).max(0);
    let h = shown + 4;
    let (x, y) = (((cols - w) / 2).max(0), ((rows - h) / 2).max(0));
    let buttons_width: i32 = dialog.buttons.iter().map(|(label, _)| width(label) + 4).sum::<i32>() - 2;
    let mut bx = x + 2 + (inner - buttons_width) / 2;
    let buttons = dialog
        .buttons
        .iter()
        .map(|&(label, action)| {
            let button = (bx, y + h - 2, label, action);
            bx += width(label) + 4;
            button
        })
        .collect();
    DialogBox { x, y, w, h, inner, shown: shown as usize, buttons }
}

fn draw_dialog(c: &mut Canvas, app: &App, l: &Layout) {
    let Some(dialog) = dialog(app) else { return };
    let th = c.th;
    let b = dialog_box(&dialog, l.cols, l.rows);
    let panel = on(th.fg.fg, th.panel.bg);
    let border = on(th.panelb.fg, th.panel.bg);
    let bar = "─".repeat((b.w - 2) as usize);
    c.put(b.x, b.y, &format!("╭{bar}╮"), border);
    c.put(b.x, b.y + b.h - 1, &format!("╰{bar}╯"), border);
    for i in 1..b.h - 1 {
        c.put(b.x, b.y + i, "│", border);
        c.put(b.x + 1, b.y + i, &" ".repeat((b.w - 2) as usize), panel);
        c.put(b.x + b.w - 1, b.y + i, "│", border);
    }
    for (i, line) in dialog.lines.iter().take(b.shown).enumerate() {
        let mut x = b.x + 2 + if line.center { (b.inner - line.width()) / 2 } else { 0 };
        for (text, style) in &line.spans {
            c.put(x, b.y + 1 + i as i32, text, *style);
            x += width(text);
        }
    }
    for (x, y, label, _) in b.buttons {
        c.put(x, y, &format!(" {label} "), on(th.btnfg.fg, th.btn.bg).bold());
    }
}

/// Confetti after a solved word: colored pixels falling down the whole screen, each on
/// a path of its own that depends only on the word and the time, so nothing is kept
/// between frames. It falls over the board and the keys but never over text.
fn draw_confetti(c: &mut Canvas, app: &App, l: &Layout) {
    let Some(start) = app.confetti else { return };
    let age = app.now.saturating_duration_since(start).as_millis() as i64;
    let (party, _) = c.th.party();
    // Pixel rows above the footer.
    let floor = 2 * l.footer_y as i64;
    let mut seed = app.game.answer.iter().fold(0x9E37_79B9_7F4A_7C15_u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01B3));
    for _ in 0..(l.cols * 3 / 4).max(16) {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let column = (seed % l.cols.max(1) as u64) as i64;
        let delay = (seed >> 16) as i64 % 900;
        // Every piece is down before the confetti's time is up.
        let fall = CONFETTI.as_millis() as i64 - 900 - (seed >> 28) as i64 % 600;
        let color = party[(seed >> 40) as usize % party.len()].fg;
        if age < delay {
            continue;
        }
        let row = (age - delay) * (floor + 2) / fall - 2;
        let sway = [0, 1, 0, -1][(((age - delay) / 170 + (seed >> 50) as i64) % 4) as usize];
        // A piece is two pixels tall, so that it is seen.
        for row in [row, row + 1] {
            if row < floor {
                c.pixel((column + sway) as i32, row as i32, color);
            }
        }
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let (cols, rows) = (area.width as i32, area.height as i32);
    let th = &app.theme;
    let mut c = Canvas { buf: frame.buffer_mut(), th };
    let screen = on(th.fg.fg, th.bg.bg);
    for y in 0..rows {
        c.put(0, y, &" ".repeat(cols as usize), screen);
    }
    let Some(l) = layout_for(app, cols, rows) else {
        let needs = format!("funwordl needs {}x{}", layout::MIN_COLS, layout::min_rows(app.game.tries));
        c.center((rows - 1) / 2, 0, cols, &needs, screen.bold());
        if rows > 2 {
            c.center((rows - 1) / 2 + 1, 0, cols, &format!("this is {cols}x{rows}"), on(th.dim.fg, th.bg.bg));
        }
        return;
    };
    draw_title(&mut c, app, &l);
    for row in 0..app.game.tries {
        for col in 0..5 {
            draw_tile(&mut c, app, &l, row, col);
        }
    }
    draw_message(&mut c, app, &l);
    draw_keyboard(&mut c, app, &l);
    draw_footer(&mut c, app, &l);
    draw_confetti(&mut c, app, &l);
    draw_dialog(&mut c, app, &l);
}

/// What a click at (x, y) presses, if anything. With a dialog open only its buttons
/// can be pressed.
pub fn action_at(app: &App, x: i32, y: i32) -> Option<Action> {
    let l = layout_for(app, app.size.0, app.size.1)?;
    if let Some(dialog) = dialog(app) {
        let b = dialog_box(&dialog, l.cols, l.rows);
        return b.buttons.into_iter().find(|&(bx, by, label, _)| y == by && x >= bx && x < bx + width(label) + 2).map(|(.., action)| action);
    }
    if let Some(&(.., key)) = keyboard_keys(&l).iter().find(|&&(kx, ky, w, _)| x >= kx && x < kx + w && y >= ky && y < ky + l.keys.py) {
        return Some(Action::Key(key));
    }
    let (items, labels) = footer_items(app, l.cols);
    items
        .into_iter()
        .find(|item| {
            // A column of slack on either side makes the small targets easier to hit.
            let w = width(item.key) + if labels { 1 + width(&item.label) } else { 0 };
            y == l.footer_y && x >= item.x - 1 && x <= item.x + w
        })
        .map(|item| item.action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Options;
    use crate::store::Stats;
    use crate::theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::{Duration, Instant};

    fn app_at(name: &str, level: Level) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("funwordl-test-{}-ui-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let options = Options { mode: Mode::Practice, theme: None, level: Some(level), animate: true, truecolor: true, debug_answer: game::word("CRANE") };
        (App::new(Stats::load(dir.clone()), options), dir)
    }

    fn app(name: &str) -> (App, std::path::PathBuf) {
        app_at(name, Level::Easy)
    }

    fn buffer(app: &mut App, cols: u16, rows: u16) -> Buffer {
        app.size = (cols as i32, rows as i32);
        let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// The screen as text, one string per row.
    fn screen(app: &mut App, cols: u16, rows: u16) -> Vec<String> {
        let buf = buffer(app, cols, rows);
        (0..rows).map(|y| (0..cols).map(|x| buf[(x, y)].symbol()).collect()).collect()
    }

    fn has(screen: &[String], text: &str) -> bool {
        screen.iter().any(|row| row.contains(text))
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn draws_an_easy_game_at_80x24() {
        let (mut app, dir) = app("80x24");
        app.game.add_guess(game::word("SLATE").unwrap());
        app.game.cur = b"CR".to_vec();
        let s = screen(&mut app, 80, 24);
        assert!(s[0].contains(" F   U   N   W   O   R   D   L   Practice"), "{}", s[0]);
        // Eight rows: a revealed guess, the one being typed, and six still empty.
        assert!(s[2].contains("  S     L     A     T     E  "), "{}", s[2]);
        assert!(s[4].contains("  C     R  "), "{}", s[4]);
        assert_eq!(layout_for(&app, 80, 24).unwrap().by + 14, 16, "the eighth row");
        assert!(s[20].contains("Q   W   E   R   T   Y   U   I   O   P"));
        assert!(s[22].contains("⌫    Z   X   C   V   B   N   M    ↵"));
        for item in ["? Help", "Tab Hint", "^S Stars", "^X Easy", "^G Reveal", "^Q Quit"] {
            assert!(s[23].contains(item), "{item}: {}", s[23]);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_other_levels_have_six_rows_and_bigger_tiles() {
        let (mut app, dir) = app_at("six", Level::Hard);
        app.game.add_guess(game::word("SLATE").unwrap());
        app.game.cur = b"CR".to_vec();
        let s = screen(&mut app, 80, 24);
        assert!(s[0].contains("L   Practice · Hard"), "{}", s[0]);
        assert!(s[2].contains("  S      L      A      T      E  "), "{}", s[2]);
        assert!(s[5].contains("█ C █  █ R █  █   █"), "{}", s[5]);
        assert!(s[23].contains("^X Hard"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_narrow_terminal_gets_a_small_title_beside_the_info() {
        let (mut app, dir) = app("narrow");
        let s = screen(&mut app, 39, 14);
        assert!(s[0].contains("FUNWORDL  Practice"), "{}", s[0]);
        // Keys only in the footer, all nine of them.
        assert!(s[13].contains("?  Tab  ^N  ^D  ^S  ^T  ^X  ^G  ^Q"), "{}", s[13]);
        // With no room for the info either, the title is back in tiles.
        app.stats.set("stars", 1234567);
        app.stats.set("practice_streak", 1234567);
        let s = screen(&mut app, 39, 14);
        assert!(s[0].contains("F   U   N   W   O   R   D   L"), "{}", s[0]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn draws_big_letters_on_a_big_terminal() {
        let (mut app, dir) = app_at("big", Level::Normal);
        app.game.cur = b"L".to_vec();
        let s = screen(&mut app, 190, 50);
        // The typed L as a bitmap inside its frame: a vertical stroke, then its foot.
        assert!(has(&s, "█ ██         █  █            █"), "no block letter");
        assert!(has(&s, "█ ██▄▄▄▄▄▄▄▄ █") || has(&s, "█ ██████████ █"), "no foot of the L");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A revealed tile on a big terminal is pixel art: every cell carries two pixels,
    /// the upper as the character's color and the lower as its background.
    #[test]
    fn revealed_tiles_are_shaded_pixel_art() {
        let (mut app, dir) = app_at("sprite", Level::Normal);
        app.game.add_guess(game::word("CRANE").unwrap());
        let l = layout_for(&app, 190, 50).unwrap();
        let cells = |app: &mut App| {
            let buf = buffer(app, 190, 50);
            move |x: i32, y: i32| buf[((l.bx + x) as u16, (l.by + y) as u16)].clone()
        };
        let th = app.theme;
        let shades = th.shades(th.g).unwrap();
        let at = cells(&mut app);
        // Top-left cell of the first tile: the screen above, the lit edge below.
        assert_eq!((at(0, 0).symbol(), at(0, 0).fg, at(0, 0).bg), ("▀", th.bg.bg, shades.light));
        // Further down the left edge both pixels are lit, so the cell is one color.
        assert_eq!((at(0, 3).symbol(), at(0, 3).bg), (" ", shades.light));
        // The right edge and the bottom edge are dark.
        assert_eq!(at(l.board.w - 1, 3).bg, shades.dark);
        assert_eq!((at(3, l.board.t - 1).fg, at(3, l.board.t - 1).bg), (shades.dark, th.bg.bg));
        // Somewhere on the tile there is the letter, its shadow, and the plain green.
        let seen: Vec<_> = (0..l.board.w).flat_map(|x| (0..l.board.t).map(move |y| (x, y))).flat_map(|(x, y)| [at(x, y).fg, at(x, y).bg]).collect();
        for color in [th.gfg.fg, shades.shadow, th.g.bg] {
            assert!(seen.contains(&color), "{color:?} is missing from the tile");
        }

        // The terminal theme has no shades to work with: its tiles stay flat.
        app.theme = theme::theme("terminal", true);
        let at = cells(&mut app);
        assert_eq!((at(0, 0).symbol(), at(0, 0).fg), ("▄", app.theme.g.fg));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_title_is_in_party_colors() {
        let (mut app, dir) = app("title");
        let l = layout_for(&app, 190, 50).unwrap();
        let (party, _) = app.theme.party();
        let buf = buffer(&mut app, 190, 50);
        for (i, paint) in party.iter().enumerate() {
            // The middle of a letter's block, clear of its edges, holds its own color.
            let colors: Vec<Color> = (0..l.title.w)
                .flat_map(|dx| (1..l.title.t - 1).map(move |dy| (dx, dy)))
                .map(|(dx, dy)| buf[((l.hx + i as i32 * l.title.px + dx) as u16, (l.hy + dy) as u16)].bg)
                .collect();
            assert!(colors.contains(&paint.bg), "letter {i}");
        }
        // The terminal theme has six colors of its own for it, and no RGB.
        let (party, ink) = theme::theme("terminal", true).party();
        assert!(party.iter().all(|p| p.rgb.is_none()) && ink.rgb.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn says_when_the_terminal_is_too_small() {
        let (mut app, dir) = app("small");
        let s = screen(&mut app, 30, 8);
        assert!(has(&s, "funwordl needs 39x14") && has(&s, "this is 30x8"));
        // Six rows need less.
        Level::Normal.apply(&mut app.game);
        assert!(has(&screen(&mut app, 30, 8), "funwordl needs 39x12"));
        assert!(has(&screen(&mut app, 39, 12), "^Q"));
        // Nothing to draw into at all must not panic either.
        screen(&mut app, 1, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    const MODALS: [Modal; 6] = [Modal::None, Modal::Help, Modal::Stats, Modal::GiveUp, Modal::Hint, Modal::Words];

    /// Every size, theme, level and dialog, mid-animation: drawing must never panic,
    /// and a dialog must keep its frame.
    #[test]
    fn draws_everything_at_any_size() {
        for level in [Level::Easy, Level::Normal] {
            let (mut app, dir) = app_at("sweep", level);
            app.game.add_guess(game::word("SLATE").unwrap());
            app.game.add_guess(game::word("CRANE").unwrap());
            app.learned.insert("crane".to_string(), "2".to_string());
            (app.hints, app.hint_letters) = (3, vec![0, 2]);
            app.reveal = Some((1, Instant::now()));
            app.shake = Some(Instant::now());
            app.celebrate = Some((1, Instant::now()));
            app.now = Instant::now() + Duration::from_millis(1200);
            app.confetti = Some(Instant::now());
            for (cols, rows) in [(39, 14), (40, 15), (60, 20), (68, 9), (80, 24), (100, 30), (120, 40), (190, 50), (250, 70), (400, 100), (39, 100), (400, 14)]
            {
                for name in theme::NAMES {
                    for truecolor in [true, false] {
                        app.theme = theme::theme(name, truecolor);
                        for modal in MODALS {
                            app.modal = modal;
                            let s = screen(&mut app, cols, rows);
                            if modal == Modal::None {
                                assert!(has(&s, "^Q"), "{cols}x{rows}");
                            } else {
                                assert!(has(&s, "╭") && has(&s, "╯"), "{cols}x{rows} {modal:?}");
                            }
                        }
                    }
                }
            }
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn fits(app: &App, what: &str) {
        let d = dialog(app).unwrap();
        assert!(d.lines.iter().all(|l| l.width() <= DIALOG_WIDTH), "{what}: {:?}", d.lines.iter().map(DialogLine::width).max());
        let buttons: i32 = d.buttons.iter().map(|(label, _)| width(label) + 4).sum::<i32>() - 2;
        assert!(buttons <= DIALOG_WIDTH, "{what}: buttons are {buttons} wide");
    }

    #[test]
    fn dialogs_fit_the_narrowest_terminal() {
        let (mut app, dir) = app("dialogs");
        // Numbers as wide as they will ever get.
        for key in ["practice_wins", "practice_streak", "practice_best", "stars"] {
            app.stats.set(key, 99999);
        }
        (app.hints, app.hint_letters) = (4, vec![0, 2, 4]);
        for level in Level::ALL {
            level.apply(&mut app.game);
            for modal in &MODALS[1..] {
                app.modal = *modal;
                fits(&app, &format!("{modal:?} at {level:?}, playing"));
            }
        }
        Level::Easy.apply(&mut app.game);
        app.game.give_up();
        app.learned.insert("crane".to_string(), "0".to_string());
        for modal in [Modal::Stats, Modal::Words] {
            app.modal = modal;
            fits(&app, &format!("{modal:?}, given up"));
        }
        app.modal = Modal::Stats;
        let s = screen(&mut app, 39, 24);
        assert!(has(&s, "NICE TRY") && has(&s, "The word was CRANE") && has(&s, " N New ") && has(&s, " W Words "), "{s:#?}");
        // The meaning of the word comes with it, wrapped to the dialog.
        assert!(has(&s, "a tall bird with long legs; a") && has(&s, "machine that lifts"), "{s:#?}");

        // After a win the word is named with its meaning too, and the stars it earned.
        app.act(Action::New);
        app.game.answer = game::word("SKEIN").unwrap();
        app.game.add_guess(game::word("SKEIN").unwrap());
        app.modal = Modal::Stats;
        fits(&app, "Stats, won");
        let s = screen(&mut app, 80, 24);
        assert!(has(&s, "YOU DID IT!") && has(&s, "★★★  Solved in 1/8") && has(&s, "SKEIN: a loose bundle of yarn or"), "{s:#?}");
        assert!(has(&s, "╭") && has(&s, "╯") && has(&s, " Esc "));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hints_show_the_meaning_then_letters_on_the_board() {
        let (mut app, dir) = app("hints");
        press(&mut app, KeyCode::Tab);
        let s = screen(&mut app, 80, 24);
        assert!(has(&s, "What the word means:") && has(&s, "a tall bird with long legs; a"), "{s:#?}");
        assert!(has(&s, "Stars if you solve it: ★★☆") && has(&s, " Tab One more ") && !has(&s, "Letters:"), "{s:#?}");
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        let s = screen(&mut app, 80, 24);
        assert!(has(&s, "Letters:  C _ A _ _") && has(&s, "Stars if you solve it: ★☆☆"), "{s:#?}");
        // Back in the game the letters wait in the row being typed, until typed over.
        press(&mut app, KeyCode::Esc);
        let row = |app: &mut App| screen(app, 80, 24)[2].trim().to_string();
        assert_eq!(row(&mut app), "C           A");
        app.game.cur = b"S".to_vec();
        assert_eq!(row(&mut app), "S           A");
        // Once there is nothing more to give, the button goes.
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        let s = screen(&mut app, 80, 24);
        assert!(has(&s, "Letters:  C _ A _ E") && !has(&s, "One more") && has(&s, " Esc Back "), "{s:#?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_collection_shows_a_word_with_its_stars_and_meaning() {
        let (mut app, dir) = app("words");
        app.modal = Modal::Words;
        assert!(has(&screen(&mut app, 80, 24), "No words yet. Finish a game and"));
        for (word, stars) in [("crane", "2"), ("skein", "0"), ("about", "3")] {
            app.learned.insert(word.to_string(), stars.to_string());
        }
        let s = screen(&mut app, 80, 24);
        assert!(has(&s, "MY WORDS · 1 of 3") && has(&s, "ABOUT  ★★★"), "{s:#?}");
        press(&mut app, KeyCode::Right);
        let s = screen(&mut app, 80, 24);
        assert!(has(&s, "MY WORDS · 2 of 3") && has(&s, "CRANE  ★★☆") && has(&s, "machine that lifts"), "{s:#?}");
        press(&mut app, KeyCode::Right);
        assert!(has(&screen(&mut app, 80, 24), "SKEIN  ☆☆☆"));
        // A card that is past the end, should the file have shrunk, shows the last word.
        app.card = 40;
        assert!(has(&screen(&mut app, 80, 24), "MY WORDS · 3 of 3"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn confetti_falls_over_everything_but_text() {
        let (mut app, dir) = app("confetti");
        app.game.add_guess(game::word("CRANE").unwrap());
        let plain = buffer(&mut app, 120, 40);
        app.confetti = Some(app.now);
        app.now += Duration::from_millis(1300);
        let party = buffer(&mut app, 120, 40);
        let (colors, _) = app.theme.party();
        let is_party = |c: Color| colors.iter().any(|p| p.fg == c);
        let mut pieces = 0;
        for y in 0..40 {
            for x in 0..120 {
                let (before, after) = (&plain[(x, y)], &party[(x, y)]);
                if before == after {
                    continue;
                }
                pieces += 1;
                assert!(matches!(before.symbol(), " " | "▀" | "▄"), "confetti over {:?} at {x},{y}", before.symbol());
                assert!(is_party(after.fg) || is_party(after.bg), "{x},{y}");
                assert!(y < 39, "confetti on the footer");
            }
        }
        assert!(pieces >= 20, "only {pieces} cells of confetti");
        // When its time is up there is none left, and nothing still moving.
        app.now += CONFETTI;
        assert!(buffer(&mut app, 120, 40) == plain);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wraps_text_at_spaces() {
        assert_eq!(wrap("a tall bird with long legs; a machine that lifts", 35), ["a tall bird with long legs; a", "machine that lifts"]);
        assert_eq!(wrap("short", 35), ["short"]);
        assert!(wrap("", 35).is_empty());
        // Every definition fits the dialog in three lines, with the word in front.
        for word in crate::words::answers() {
            let line = format!("{}: {}", word.to_uppercase(), words::definition(word).unwrap());
            let rows = wrap(&line, DIALOG_WIDTH as usize);
            assert!(rows.len() <= 3 && rows.iter().all(|r| r.chars().count() <= DIALOG_WIDTH as usize), "{word}: {rows:?}");
        }
    }

    #[test]
    fn clicks_land_on_what_is_drawn_there() {
        let (mut app, dir) = app("clicks");
        let find = |s: &[String], text: &str| {
            let (y, row) = s.iter().enumerate().find(|(_, row)| row.contains(text)).unwrap_or_else(|| panic!("no {text:?} on screen"));
            (row[..row.find(text).unwrap()].chars().count() as i32, y as i32)
        };
        let s = screen(&mut app, 80, 24);
        let (x, y) = find(&s, "Q   W");
        assert_eq!(action_at(&app, x, y), Some(Action::Key(KeyId::Letter(b'Q'))));
        assert_eq!(action_at(&app, x + 4, y), Some(Action::Key(KeyId::Letter(b'W'))));
        let (x, y) = find(&s, "⌫");
        assert_eq!(action_at(&app, x, y), Some(Action::Key(KeyId::Back)));
        let (x, y) = find(&s, "↵");
        assert_eq!(action_at(&app, x, y), Some(Action::Key(KeyId::Enter)));
        let (x, y) = find(&s, "^G Reveal");
        assert_eq!(action_at(&app, x + 5, y), Some(Action::GiveUp));
        let (x, y) = find(&s, "Tab Hint");
        assert_eq!(action_at(&app, x + 5, y), Some(Action::Hint));
        assert_eq!(action_at(&app, 0, 0), None);

        // With a dialog open, only its buttons can be pressed.
        app.modal = Modal::GiveUp;
        let s = screen(&mut app, 80, 24);
        let (x, y) = find(&s, " Enter Show it ");
        assert_eq!(action_at(&app, x + 1, y), Some(Action::Surrender));
        let (x, y) = find(&s, " Esc Keep playing ");
        assert_eq!(action_at(&app, x + 3, y), Some(Action::Close));
        assert_eq!(action_at(&app, 0, 23), None);
        for (modal, button, action) in
            [(Modal::Hint, " Tab One more ", Action::Hint), (Modal::Stats, " W Words ", Action::Words), (Modal::Words, " Esc Close ", Action::Close)]
        {
            app.modal = modal;
            let s = screen(&mut app, 80, 24);
            let (x, y) = find(&s, button);
            assert_eq!(action_at(&app, x + 2, y), Some(action), "{modal:?}");
        }
        app.learned.insert("crane".to_string(), "3".to_string());
        let s = screen(&mut app, 80, 24);
        let (x, y) = find(&s, " > Next ");
        assert_eq!(action_at(&app, x + 2, y), Some(Action::CardNext));
        let _ = std::fs::remove_dir_all(dir);
    }
}
