//! Color themes. Code asks for roles ("the green tile", "an unused key"), never for
//! colors; only this file knows RGB values.
//!
//! "terminal" uses only the terminal's own 16-color palette and no background, so it
//! follows whatever theme the terminal has. The others are truecolor, mapped to the
//! nearest of 256 colors on terminals that do not announce truecolor.

use ratatui::style::Color;

/// One role's color, as a foreground and as a background. They are the same color in
/// the truecolor themes; in "terminal" they can differ (an absent key is gray text on
/// the terminal's own background).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Paint {
    pub fg: Color,
    pub bg: Color,
    /// The color as red, green and blue, from which lighter and darker shades are
    /// worked out. `None` in the "terminal" theme, whose colors are the terminal's own
    /// and cannot be shaded.
    pub rgb: Option<(u8, u8, u8)>,
}

/// The shades that make a flat tile look raised: a lit edge, a dark edge, and the
/// shadow a letter casts on it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shades {
    pub light: Color,
    pub dark: Color,
    pub shadow: Color,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    /// Whether colors are sent as exact RGB or as the nearest of 256.
    truecolor: bool,
    /// The screen.
    pub bg: Paint,
    /// Ordinary text.
    pub fg: Paint,
    /// Hints and labels.
    pub dim: Paint,
    /// The frame of an empty tile.
    pub empty: Paint,
    /// The frame of a tile with a typed letter.
    pub typed: Paint,
    /// Right letter, right spot.
    pub g: Paint,
    /// Right letter, wrong spot.
    pub y: Paint,
    /// Letter not in the word.
    pub x: Paint,
    /// The flash that runs along a winning row.
    pub win: Paint,
    /// Letters on green, yellow and gray tiles.
    pub gfg: Paint,
    pub yfg: Paint,
    pub xfg: Paint,
    /// A key not tried yet, and its letter.
    pub key: Paint,
    pub keyfg: Paint,
    /// A key known not to be in the word, and its letter.
    pub keyx: Paint,
    pub keyxfg: Paint,
    /// Key names, dialog titles.
    pub accent: Paint,
    /// Dialog background and border.
    pub panel: Paint,
    pub panelb: Paint,
    /// The message chip and its text.
    pub toast: Paint,
    pub toastfg: Paint,
    /// Dialog buttons and their text.
    pub btn: Paint,
    pub btnfg: Paint,
}

/// The theme a new player gets: a bright one.
pub const DEFAULT: &str = "candy";

pub const NAMES: [&str; 21] = [
    "midnight", "daylight", "neon", "contrast", "ocean", "ember", "paper", "sky", "candy", "coral", "cherry", "ruby", "sunset", "lemon", "mint", "meadow",
    "sage", "lavender", "orchid", "grape", "terminal",
];

/// The nearest of the 6x6x6 color cube and the 24-step gray ramp.
fn nearest_256(r: u8, g: u8, b: u8) -> Color {
    let rgb = [r as i32, g as i32, b as i32];
    let cube_index = |v: i32| {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    };
    let cube_value = |i: i32| if i == 0 { 0 } else { 55 + 40 * i };
    let ci = rgb.map(cube_index);
    let gray = (rgb[0] + rgb[1] + rgb[2]) / 3;
    let gi = if gray < 8 {
        0
    } else if gray > 238 {
        23
    } else {
        (gray - 3) / 10
    };
    let gv = 8 + 10 * gi;
    let cube_distance: i32 = (0..3).map(|i| (rgb[i] - cube_value(ci[i])).pow(2)).sum();
    let gray_distance: i32 = rgb.iter().map(|v| (v - gv).pow(2)).sum();
    Color::Indexed(if gray_distance < cube_distance { 232 + gi } else { 16 + 36 * ci[0] + 6 * ci[1] + ci[2] } as u8)
}

fn from_rgb(name: &'static str, truecolor: bool, c: [(u8, u8, u8); 23]) -> Theme {
    let p = |i: usize| {
        let (r, g, b) = c[i];
        let color = if truecolor { Color::Rgb(r, g, b) } else { nearest_256(r, g, b) };
        Paint { fg: color, bg: color, rgb: Some((r, g, b)) }
    };
    Theme {
        name,
        truecolor,
        bg: p(0),
        fg: p(1),
        dim: p(2),
        empty: p(3),
        typed: p(4),
        g: p(5),
        y: p(6),
        x: p(7),
        win: p(8),
        gfg: p(9),
        yfg: p(10),
        xfg: p(11),
        key: p(12),
        keyfg: p(13),
        keyx: p(14),
        keyxfg: p(15),
        accent: p(16),
        panel: p(17),
        panelb: p(18),
        toast: p(19),
        toastfg: p(20),
        btn: p(21),
        btnfg: p(22),
    }
}

/// The terminal's own colors. `Reset` is its default foreground or background.
fn terminal() -> Theme {
    let same = |c: Color| Paint { fg: c, bg: c, rgb: None };
    let default = same(Color::Reset);
    Theme {
        name: "terminal",
        truecolor: false,
        bg: Paint { fg: Color::Black, bg: Color::Reset, rgb: None },
        fg: default,
        dim: same(Color::DarkGray),
        empty: same(Color::DarkGray),
        typed: same(Color::Gray),
        g: same(Color::Green),
        y: same(Color::Yellow),
        x: same(Color::DarkGray),
        win: same(Color::LightGreen),
        gfg: same(Color::Black),
        yfg: same(Color::Black),
        xfg: same(Color::White),
        key: same(Color::Gray),
        keyfg: same(Color::Black),
        keyx: Paint { fg: Color::DarkGray, bg: Color::Reset, rgb: None },
        keyxfg: same(Color::DarkGray),
        accent: same(Color::Cyan),
        panel: default,
        panelb: same(Color::Cyan),
        toast: same(Color::Gray),
        toastfg: same(Color::Black),
        btn: same(Color::Cyan),
        btnfg: same(Color::Black),
    }
}

impl Theme {
    /// Lighter and darker versions of a color, for a tile drawn as raised pixel art.
    /// `None` when the theme's colors cannot be shaded: such tiles stay flat.
    pub fn shades(&self, paint: Paint) -> Option<Shades> {
        let (r, g, b) = paint.rgb?;
        let color = |f: fn(f32) -> f32| {
            let [r, g, b] = [r, g, b].map(|c| f(c as f32).clamp(0.0, 255.0) as u8);
            if self.truecolor { Color::Rgb(r, g, b) } else { nearest_256(r, g, b) }
        };
        Some(Shades { light: color(|c| c * 1.35 + 20.0), dark: color(|c| c * 0.6), shadow: color(|c| c * 0.45) })
    }
}

/// The colors of the title's letters and of the confetti: the same cheerful eight in
/// every theme, as red, green and blue.
/// Stand-ins for party colors that a theme's background would swallow: dark ones, which
/// show on any bright screen and take the same white letter.
const STAND_INS: [(u8, u8, u8); 4] = [(38, 70, 83), (88, 52, 130), (22, 108, 98), (150, 62, 24)];
const PARTY: [(u8, u8, u8); 8] = [(226, 68, 92), (238, 125, 38), (214, 158, 18), (58, 170, 95), (26, 166, 178), (60, 120, 230), (140, 90, 220), (226, 88, 168)];

impl Theme {
    /// The party colors and the color of a letter on them. The "terminal" theme has only
    /// the terminal's own palette, so there they are its six plain colors.
    pub fn party(&self) -> ([Paint; 8], Paint) {
        if self.name == "terminal" {
            let same = |c: Color| Paint { fg: c, bg: c, rgb: None };
            let six = [Color::Red, Color::Yellow, Color::Green, Color::Cyan, Color::Blue, Color::Magenta];
            return (std::array::from_fn(|i| same(six[i % 6])), same(Color::Black));
        }
        let paint = |(r, g, b): (u8, u8, u8)| {
            let color = if self.truecolor { Color::Rgb(r, g, b) } else { nearest_256(r, g, b) };
            Paint { fg: color, bg: color, rgb: Some((r, g, b)) }
        };
        // A party color that is nearly the screen's own (red on a red theme) would
        // vanish, as a title letter and as confetti: a stand-in takes its place.
        let screen = self.bg.rgb.unwrap_or((0, 0, 0));
        let mut spare = STAND_INS.into_iter().filter(|&c| !near(c, screen)).cycle();
        let colors = PARTY.map(|c| if near(c, screen) { spare.next().unwrap_or(c) } else { c });
        (colors.map(paint), paint((255, 255, 255)))
    }
}

/// Whether two colors are too alike to tell one from the other at a glance.
fn near(a: (u8, u8, u8), b: (u8, u8, u8)) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) < 80 * 80
}

/// The hue of a color in degrees: 0 red, 60 yellow, 120 green, 240 blue.
#[cfg(test)]
fn hue((r, g, b): (u8, u8, u8)) -> f32 {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max == min {
        return 0.0;
    }
    let h = if max == r {
        (g - b) / (max - min)
    } else if max == g {
        2.0 + (b - r) / (max - min)
    } else {
        4.0 + (r - g) / (max - min)
    };
    (h * 60.0).rem_euclid(360.0)
}

/// How bright a color looks, from 0 to 255; 128 when it is not known.
pub fn brightness(paint: Paint) -> u32 {
    paint.rgb.map_or(128, |(r, g, b)| (299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000)
}

/// The theme of that name; an unknown name gives the default, "midnight".
#[rustfmt::skip]
pub fn theme(name: &str, truecolor: bool) -> Theme {
    match name {
        "daylight" => from_rgb("daylight", truecolor, [
        (248, 248, 244), // bg
        (28, 31, 40), // fg
        (128, 134, 148), // dim
        (205, 210, 221), // empty
        (112, 119, 136), // typed
        (58, 158, 84), // g
        (219, 166, 35), // y
        (118, 125, 141), // x
        (104, 204, 130), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (212, 217, 228), // key
        (28, 31, 40), // keyfg
        (118, 125, 141), // keyx
        (240, 241, 245), // keyxfg
        (37, 99, 220), // accent
        (255, 255, 255), // panel
        (37, 99, 220), // panelb
        (28, 31, 40), // toast
        (248, 248, 244), // toastfg
        (37, 99, 220), // btn
        (255, 255, 255), // btnfg
        ]),
        "neon" => from_rgb("neon", truecolor, [
        (14, 9, 30), // bg
        (240, 234, 255), // fg
        (138, 120, 184), // dim
        (62, 46, 106), // empty
        (158, 128, 230), // typed
        (0, 200, 120), // g
        (255, 190, 0), // y
        (58, 44, 96), // x
        (120, 255, 190), // win
        (6, 30, 20), // gfg
        (40, 26, 0), // yfg
        (240, 234, 255), // xfg
        (98, 74, 156), // key
        (255, 255, 255), // keyfg
        (30, 22, 54), // keyx
        (98, 82, 140), // keyxfg
        (255, 92, 205), // accent
        (26, 18, 52), // panel
        (255, 92, 205), // panelb
        (255, 92, 205), // toast
        (14, 9, 30), // toastfg
        (255, 92, 205), // btn
        (14, 9, 30), // btnfg
        ]),
        // Orange and blue instead of green and yellow, for color-blind players.
        "contrast" => from_rgb("contrast", truecolor, [
        (17, 19, 26), // bg
        (236, 238, 244), // fg
        (125, 131, 150), // dim
        (52, 57, 74), // empty
        (122, 130, 156), // typed
        (245, 121, 58), // g
        (133, 192, 249), // y
        (62, 67, 84), // x
        (255, 170, 120), // win
        (20, 12, 6), // gfg
        (8, 20, 34), // yfg
        (255, 255, 255), // xfg
        (86, 93, 117), // key
        (255, 255, 255), // keyfg
        (32, 35, 46), // keyx
        (96, 102, 122), // keyxfg
        (133, 192, 249), // accent
        (27, 30, 41), // panel
        (133, 192, 249), // panelb
        (236, 238, 244), // toast
        (17, 19, 26), // toastfg
        (133, 192, 249), // btn
        (8, 20, 34), // btnfg
        ]),
        "ocean" => from_rgb("ocean", truecolor, [
        (8, 24, 42), // bg
        (226, 238, 248), // fg
        (108, 138, 166), // dim
        (30, 58, 86), // empty
        (104, 146, 184), // typed
        (32, 178, 128), // g
        (236, 180, 52), // y
        (44, 70, 98), // x
        (120, 236, 190), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (62, 100, 138), // key
        (255, 255, 255), // keyfg
        (18, 38, 60), // keyx
        (78, 108, 138), // keyxfg
        (86, 204, 232), // accent
        (14, 34, 56), // panel
        (86, 204, 232), // panelb
        (226, 238, 248), // toast
        (8, 24, 42), // toastfg
        (86, 204, 232), // btn
        (6, 24, 36), // btnfg
        ]),
        "ember" => from_rgb("ember", truecolor, [
        (28, 18, 16), // bg
        (248, 236, 226), // fg
        (160, 130, 118), // dim
        (74, 50, 44), // empty
        (176, 136, 120), // typed
        (96, 170, 72), // g
        (232, 160, 40), // y
        (86, 62, 56), // x
        (170, 236, 130), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (124, 90, 80), // key
        (255, 255, 255), // keyfg
        (46, 31, 28), // keyx
        (122, 94, 86), // keyxfg
        (255, 140, 90), // accent
        (40, 27, 24), // panel
        (255, 140, 90), // panelb
        (248, 236, 226), // toast
        (28, 18, 16), // toastfg
        (255, 140, 90), // btn
        (36, 16, 8), // btnfg
        ]),
        // The bright ones: a colored page instead of a dark screen.
        "paper" => from_rgb("paper", truecolor, [
        (246, 238, 214), // bg
        (46, 48, 72), // fg
        (128, 124, 118), // dim
        (214, 208, 184), // empty
        (96, 100, 132), // typed
        (78, 140, 110), // g
        (228, 164, 72), // y
        (132, 130, 138), // x
        (150, 206, 176), // win
        (255, 255, 255), // gfg
        (46, 48, 72), // yfg
        (255, 255, 255), // xfg
        (226, 220, 196), // key
        (46, 48, 72), // keyfg
        (170, 166, 160), // keyx
        (240, 236, 220), // keyxfg
        (190, 84, 58), // accent
        (252, 250, 240), // panel
        (224, 122, 95), // panelb
        (61, 64, 91), // toast
        (244, 241, 222), // toastfg
        (212, 104, 78), // btn
        (255, 255, 255), // btnfg
        ]),
        "sky" => from_rgb("sky", truecolor, [
        (160, 212, 238), // bg
        (2, 48, 71), // fg
        (40, 98, 128), // dim
        (110, 176, 208), // empty
        (33, 118, 150), // typed
        (30, 150, 96), // g
        (255, 183, 3), // y
        (104, 116, 128), // x
        (120, 220, 160), // win
        (255, 255, 255), // gfg
        (2, 48, 71), // yfg
        (255, 255, 255), // xfg
        (255, 243, 214), // key
        (2, 48, 71), // keyfg
        (118, 170, 198), // keyx
        (214, 236, 248), // keyxfg
        (196, 88, 0), // accent
        (255, 250, 238), // panel
        (251, 133, 0), // panelb
        (2, 48, 71), // toast
        (255, 246, 226), // toastfg
        (251, 133, 0), // btn
        (2, 40, 60), // btnfg
        ]),
        "candy" => from_rgb("candy", truecolor, [
        (255, 206, 226), // bg
        (60, 22, 58), // fg
        (150, 84, 124), // dim
        (232, 160, 196), // empty
        (88, 112, 190), // typed
        (30, 160, 110), // g
        (240, 160, 20), // y
        (140, 124, 136), // x
        (120, 226, 176), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (236, 245, 255), // key
        (24, 44, 96), // keyfg
        (200, 150, 180), // keyx
        (250, 226, 240), // keyxfg
        (36, 99, 214), // accent
        (246, 250, 255), // panel
        (58, 134, 255), // panelb
        (24, 44, 96), // toast
        (236, 244, 255), // toastfg
        (42, 110, 224), // btn
        (255, 255, 255), // btnfg
        ]),
        "coral" => from_rgb("coral", truecolor, [
        (255, 160, 137), // bg
        (28, 36, 54), // fg
        (122, 58, 52), // dim
        (232, 128, 108), // empty
        (120, 44, 44), // typed
        (30, 136, 92), // g
        (255, 221, 120), // y
        (112, 100, 104), // x
        (80, 200, 140), // win
        (255, 255, 255), // gfg
        (28, 36, 54), // yfg
        (255, 255, 255), // xfg
        (255, 240, 224), // key
        (28, 36, 54), // keyfg
        (224, 130, 112), // keyx
        (255, 226, 216), // keyxfg
        (18, 70, 110), // accent
        (255, 246, 236), // panel
        (0, 128, 128), // panelb
        (28, 36, 54), // toast
        (255, 240, 224), // toastfg
        (0, 128, 128), // btn
        (255, 255, 255), // btnfg
        ]),
        "cherry" => from_rgb("cherry", truecolor, [
        (255, 107, 107), // bg
        (20, 30, 40), // fg
        (110, 24, 36), // dim
        (226, 82, 86), // empty
        (120, 24, 36), // typed
        (22, 122, 82), // g
        (255, 230, 109), // y
        (110, 92, 96), // x
        (90, 210, 150), // win
        (255, 255, 255), // gfg
        (26, 83, 92), // yfg
        (255, 244, 244), // xfg
        (247, 255, 247), // key
        (26, 83, 92), // keyfg
        (228, 90, 94), // keyx
        (255, 200, 200), // keyxfg
        (18, 62, 70), // accent
        (247, 255, 247), // panel
        (26, 83, 92), // panelb
        (26, 83, 92), // toast
        (247, 255, 247), // toastfg
        (78, 205, 196), // btn
        (16, 60, 66), // btnfg
        ]),
        "ruby" => from_rgb("ruby", truecolor, [
        (224, 82, 96), // bg
        (255, 246, 240), // fg
        (255, 190, 190), // dim
        (246, 130, 138), // empty
        (255, 214, 214), // typed
        (26, 134, 86), // g
        (255, 214, 102), // y
        (92, 80, 84), // x
        (80, 200, 140), // win
        (255, 255, 255), // gfg
        (70, 16, 28), // yfg
        (244, 236, 236), // xfg
        (255, 236, 230), // key
        (70, 16, 28), // keyfg
        (190, 62, 78), // keyx
        (255, 176, 180), // keyxfg
        (255, 214, 102), // accent
        (92, 20, 36), // panel
        (255, 214, 102), // panelb
        (255, 246, 240), // toast
        (120, 20, 40), // toastfg
        (255, 214, 102), // btn
        (70, 16, 28), // btnfg
        ]),
        "sunset" => from_rgb("sunset", truecolor, [
        (244, 162, 97), // bg
        (30, 58, 70), // fg
        (120, 66, 30), // dim
        (222, 138, 72), // empty
        (130, 70, 30), // typed
        (46, 148, 100), // g
        (246, 214, 130), // y
        (122, 112, 106), // x
        (96, 204, 150), // win
        (255, 255, 255), // gfg
        (30, 58, 70), // yfg
        (255, 255, 255), // xfg
        (255, 240, 214), // key
        (30, 58, 70), // keyfg
        (226, 140, 80), // keyx
        (255, 228, 196), // keyxfg
        (38, 70, 83), // accent
        (255, 246, 230), // panel
        (42, 157, 143), // panelb
        (38, 70, 83), // toast
        (255, 240, 214), // toastfg
        (231, 111, 81), // btn
        (255, 255, 255), // btnfg
        ]),
        "lemon" => from_rgb("lemon", truecolor, [
        (255, 243, 176), // bg
        (84, 11, 14), // fg
        (130, 112, 60), // dim
        (226, 208, 130), // empty
        (150, 128, 60), // typed
        (74, 140, 88), // g
        (224, 159, 62), // y
        (146, 142, 128), // x
        (130, 200, 140), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (196, 220, 226), // key
        (30, 60, 70), // keyfg
        (214, 200, 140), // keyx
        (255, 255, 255), // keyxfg
        (51, 92, 103), // accent
        (255, 252, 236), // panel
        (51, 92, 103), // panelb
        (51, 92, 103), // toast
        (255, 243, 176), // toastfg
        (158, 42, 43), // btn
        (255, 255, 255), // btnfg
        ]),
        "mint" => from_rgb("mint", truecolor, [
        (183, 228, 199), // bg
        (27, 67, 50), // fg
        (70, 120, 96), // dim
        (140, 200, 166), // empty
        (64, 145, 108), // typed
        (45, 106, 79), // g
        (240, 170, 40), // y
        (120, 134, 128), // x
        (116, 198, 157), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (255, 240, 246), // key
        (27, 67, 50), // keyfg
        (150, 196, 170), // keyx
        (226, 246, 234), // keyxfg
        (190, 44, 104), // accent
        (250, 255, 251), // panel
        (214, 64, 120), // panelb
        (27, 67, 50), // toast
        (236, 250, 240), // toastfg
        (214, 64, 120), // btn
        (255, 255, 255), // btnfg
        ]),
        "meadow" => from_rgb("meadow", truecolor, [
        (176, 224, 132), // bg
        (30, 50, 20), // fg
        (78, 112, 50), // dim
        (140, 196, 96), // empty
        (70, 120, 50), // typed
        (34, 120, 70), // g
        (226, 140, 20), // y
        (116, 122, 112), // x
        (90, 200, 120), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (250, 240, 255), // key
        (60, 30, 90), // keyfg
        (150, 200, 116), // keyx
        (232, 248, 214), // keyxfg
        (110, 50, 160), // accent
        (252, 248, 255), // panel
        (140, 80, 190), // panelb
        (50, 30, 80), // toast
        (240, 232, 255), // toastfg
        (130, 70, 190), // btn
        (255, 255, 255), // btnfg
        ]),
        "sage" => from_rgb("sage", truecolor, [
        (204, 213, 174), // bg
        (52, 58, 40), // fg
        (110, 116, 84), // dim
        (170, 182, 138), // empty
        (120, 110, 80), // typed
        (88, 129, 87), // g
        (221, 161, 94), // y
        (130, 128, 116), // x
        (140, 190, 130), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (254, 250, 224), // key
        (52, 58, 40), // keyfg
        (178, 188, 150), // keyx
        (240, 244, 222), // keyxfg
        (170, 92, 28), // accent
        (254, 250, 224), // panel
        (221, 161, 94), // panelb
        (40, 54, 24), // toast
        (254, 250, 224), // toastfg
        (188, 108, 37), // btn
        (255, 255, 255), // btnfg
        ]),
        "lavender" => from_rgb("lavender", truecolor, [
        (224, 187, 228), // bg
        (50, 30, 70), // fg
        (116, 84, 140), // dim
        (196, 152, 204), // empty
        (120, 90, 150), // typed
        (60, 150, 110), // g
        (232, 150, 20), // y
        (134, 128, 144), // x
        (120, 214, 160), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (255, 223, 211), // key
        (50, 30, 70), // keyfg
        (196, 160, 206), // keyx
        (244, 226, 246), // keyxfg
        (120, 64, 160), // accent
        (253, 246, 255), // panel
        (149, 125, 173), // panelb
        (50, 30, 70), // toast
        (253, 240, 255), // toastfg
        (240, 180, 60), // btn
        (50, 30, 70), // btnfg
        ]),
        "orchid" => from_rgb("orchid", truecolor, [
        (214, 148, 214), // bg
        (48, 16, 60), // fg
        (112, 56, 120), // dim
        (188, 116, 190), // empty
        (110, 50, 120), // typed
        (30, 146, 96), // g
        (255, 214, 90), // y
        (128, 118, 134), // x
        (90, 214, 170), // win
        (255, 255, 255), // gfg
        (48, 16, 60), // yfg
        (255, 255, 255), // xfg
        (236, 255, 240), // key
        (30, 70, 50), // keyfg
        (190, 124, 194), // keyx
        (248, 220, 248), // keyxfg
        (16, 100, 82), // accent
        (250, 244, 252), // panel
        (20, 140, 110), // panelb
        (48, 16, 60), // toast
        (248, 232, 250), // toastfg
        (20, 140, 110), // btn
        (255, 255, 255), // btnfg
        ]),
        "grape" => from_rgb("grape", truecolor, [
        (124, 77, 170), // bg
        (250, 244, 255), // fg
        (214, 190, 240), // dim
        (160, 116, 204), // empty
        (224, 200, 246), // typed
        (40, 170, 120), // g
        (255, 210, 80), // y
        (176, 168, 190), // x
        (110, 230, 170), // win
        (255, 255, 255), // gfg
        (50, 24, 80), // yfg
        (50, 24, 80), // xfg
        (240, 230, 255), // key
        (50, 24, 80), // keyfg
        (98, 58, 140), // keyx
        (190, 160, 220), // keyxfg
        (255, 210, 80), // accent
        (48, 24, 80), // panel
        (255, 210, 80), // panelb
        (250, 244, 255), // toast
        (70, 30, 110), // toastfg
        (255, 210, 80), // btn
        (50, 24, 80), // btnfg
        ]),
        "terminal" => terminal(),
        _ => from_rgb("midnight", truecolor, [
        (17, 19, 26), // bg
        (236, 238, 244), // fg
        (125, 131, 150), // dim
        (52, 57, 74), // empty
        (122, 130, 156), // typed
        (40, 167, 88), // g
        (222, 168, 36), // y
        (62, 67, 84), // x
        (110, 226, 150), // win
        (255, 255, 255), // gfg
        (255, 255, 255), // yfg
        (255, 255, 255), // xfg
        (86, 93, 117), // key
        (255, 255, 255), // keyfg
        (32, 35, 46), // keyx
        (96, 102, 122), // keyxfg
        (108, 168, 255), // accent
        (27, 30, 41), // panel
        (108, 168, 255), // panelb
        (236, 238, 244), // toast
        (17, 19, 26), // toastfg
        (108, 168, 255), // btn
        (12, 18, 32), // btnfg
        ]),
    }
}

/// The theme after this one in `NAMES`, wrapping around.
pub fn next_name(name: &str) -> &'static str {
    let at = NAMES.iter().position(|n| *n == name).unwrap_or(0);
    NAMES[(at + 1) % NAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_has_a_theme_and_the_cycle_visits_all() {
        let mut name = NAMES[0];
        for expected in NAMES {
            assert_eq!(name, expected);
            assert_eq!(theme(name, true).name, name);
            name = next_name(name);
        }
        assert_eq!(name, NAMES[0]);
        assert_eq!(theme("no such theme", true).name, "midnight");
    }

    /// Whatever is written must stand out from what it is written on, in every theme.
    #[test]
    fn text_can_be_read_in_every_theme() {
        for name in NAMES.into_iter().filter(|name| *name != "terminal") {
            let th = theme(name, true);
            let pairs = [
                ("text", th.fg, th.bg, 120),
                ("text in a dialog", th.fg, th.panel, 120),
                ("hints", th.dim, th.bg, 50),
                ("hints in a dialog", th.dim, th.panel, 50),
                ("green tile", th.gfg, th.g, 70),
                ("yellow tile", th.yfg, th.y, 70),
                ("gray tile", th.xfg, th.x, 70),
                ("key", th.keyfg, th.key, 100),
                ("absent key", th.keyxfg, th.keyx, 50),
                ("message", th.toastfg, th.toast, 100),
                ("button", th.btnfg, th.btn, 100),
            ];
            for (what, text, on, least) in pairs {
                assert!(brightness(text).abs_diff(brightness(on)) >= least, "{name}: {what}");
            }
            // Tiles and keys must not melt into the screen either.
            for (what, block) in [("green", th.g), ("yellow", th.y), ("gray", th.x), ("key", th.key), ("frame", th.empty)] {
                assert!(block.rgb != th.bg.rgb && brightness(block).abs_diff(brightness(th.bg)) >= 15, "{name}: {what} on the screen");
            }
        }
    }

    /// The title and the confetti must show on every background, the red ones too.
    #[test]
    fn party_colors_stand_out_from_the_screen() {
        for name in NAMES.into_iter().filter(|name| *name != "terminal") {
            let th = theme(name, true);
            let (party, ink) = th.party();
            for paint in party {
                assert!(!near(paint.rgb.unwrap(), th.bg.rgb.unwrap()), "{name}: {:?} on {:?}", paint.rgb, th.bg.rgb);
                assert!(brightness(ink).abs_diff(brightness(paint)) >= 60, "{name}: a title letter on {:?}", paint.rgb);
            }
        }
        // Where nothing clashes, they are the eight as listed.
        assert_eq!(theme("midnight", true).party().0.map(|p| p.rgb.unwrap()), PARTY);
    }

    /// Whatever the background, the three clues keep their meaning by color: right spot
    /// is a green, wrong spot a yellow or an orange, not in the word a gray. Only
    /// `contrast` differs, on purpose: orange and blue, for color-blind players.
    #[test]
    fn clue_colors_are_green_yellow_and_gray_in_every_theme() {
        for name in NAMES.into_iter().filter(|name| !["terminal", "contrast"].contains(name)) {
            let th = theme(name, true);
            let (g, y, x) = (th.g.rgb.unwrap(), th.y.rgb.unwrap(), th.x.rgb.unwrap());
            assert!((100.0..=165.0).contains(&hue(g)), "{name}: green is at {}", hue(g));
            assert!((25.0..=60.0).contains(&hue(y)), "{name}: yellow is at {}", hue(y));
            let spread = x.0.max(x.1).max(x.2) - x.0.min(x.1).min(x.2);
            // A gray may lean towards its theme (the dark themes' do), but not far.
            assert!(spread <= 60, "{name}: gray has too much color in it ({spread})");
        }
    }

    #[test]
    fn shades_a_color_lighter_and_darker() {
        let th = theme("midnight", true);
        let shades = th.shades(th.g).unwrap();
        assert_eq!(th.g.bg, Color::Rgb(40, 167, 88));
        assert_eq!(shades.light, Color::Rgb(74, 245, 138));
        assert_eq!(shades.dark, Color::Rgb(24, 100, 52));
        assert_eq!(shades.shadow, Color::Rgb(18, 75, 39));
        // Without truecolor the shades are still colors the terminal has.
        assert!(matches!(theme("midnight", false).shades(th.g).unwrap().light, Color::Indexed(_)));
        // The terminal's own colors cannot be shaded: its tiles stay flat.
        let terminal = theme("terminal", true);
        assert_eq!(terminal.shades(terminal.g), None);
        assert!(brightness(th.gfg) > brightness(th.g));
    }

    #[test]
    fn falls_back_to_256_colors() {
        // Cream must stay cream: a little less green and it lands on the cube's pink.
        assert_eq!(theme("paper", false).bg.bg, Color::Indexed(230));
        // A dark blue-gray background must not turn into the cube's dark blue.
        assert_eq!(nearest_256(17, 19, 26), Color::Indexed(233));
        assert_eq!(nearest_256(255, 255, 255), Color::Indexed(231));
        assert_eq!(nearest_256(0, 0, 0), Color::Indexed(16));
        assert_eq!(nearest_256(40, 167, 88), Color::Indexed(35));
        assert!(matches!(theme("midnight", false).bg.bg, Color::Indexed(_)));
        assert!(matches!(theme("midnight", true).bg.bg, Color::Rgb(17, 19, 26)));
    }
}
