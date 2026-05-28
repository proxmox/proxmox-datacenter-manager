//! Pretty terminal rendering utilities.

use std::fmt;

use crate::env;
use crate::env::use_emoji;

const COLOR_RESET: &str = "\x1b[0m";

/// Print something where, if the terminal supports it and color output is not disabled, the
/// content is preceded by the chosen format escape sequence and followed by a reset escape
/// sequence.
pub struct Formatted<'a, T>(pub &'a str, pub T);

impl<T: fmt::Display> fmt::Display for Formatted<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if env().use_color() {
            write!(f, "{}{}{}", self.0, self.1, COLOR_RESET)
        } else {
            fmt::Display::fmt(&self.1, f)
        }
    }
}

/// If a terminal with emoji-support is used, render the content at a fixed position from the left.
/// Otherwise, skip the content.
pub struct Position<T>(pub usize, pub T);

impl<T: fmt::Display> fmt::Display for Position<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !use_emoji() {
            return Ok(());
        }
        let pos = self.0;
        let out = self.1.to_string();
        write!(f, "\x1b7\x1b[{pos}G{out}\x1b8")
    }
}

/*
/// Render something right-aligned with a known length.
struct Right<T>(T, usize);

impl<T: fmt::Display> fmt::Display for Right<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !use_emoji() {
            return Ok(());
        }
        let out = self.0.to_string();
        let len = self.1;
        write!(f, "\x1b7\x1b[999C\x1b[{len}D{out}\x1b8")
    }
}
*/

/// Only render text if the terminal has a minimum length.
pub struct IfWide<T>(pub u32, pub T);

impl<T> fmt::Display for IfWide<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if env::WinSize::cols().is_some_and(|s| s >= self.0) {
            fmt::Display::fmt(&self.1, f)
        } else {
            Ok(())
        }
    }
}

pub struct FractionAsBar(pub f64);

fn color_for_fraction(fraction: f64) -> &'static str {
    const COLOR_LOW: &str = "\x1b[37m";
    const COLOR_MID: &str = "\x1b[97m";
    const COLOR_HI: &str = "\x1b[31m";

    match fraction {
        f if f > 0.9 => COLOR_HI,
        f if f > 0.3 => COLOR_MID,
        _ => COLOR_LOW,
    }
}

impl fmt::Display for FractionAsBar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {}",
            Formatted(
                color_for_fraction(self.0),
                format_args!("{:5.1}%", self.0 * 100.),
            ),
            colored_fraction(self.0, fraction_to_bar(self.0)),
        )
    }
}

fn fraction_to_bar(fraction: f64) -> &'static str {
    const BAR_EIGHTS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let index = if fraction < 0.1 {
        0
    } else {
        ((fraction * 8.0).ceil() as usize).min(8)
    };
    BAR_EIGHTS[index]
}

pub struct FractionAsBlock(pub f64);

impl fmt::Display for FractionAsBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Since we use "black" as a background color and many terminal color schemes make this
        // the same as the background color, let's also add an "end" of the bar:
        let block_end = if use_emoji() { "▏" } else { "" };

        write!(
            f,
            "{} {}{block_end}",
            Formatted(
                color_for_fraction(self.0),
                format_args!("{:5.1}%", self.0 * 100.),
            ),
            colored_fraction(self.0, fraction_to_block(self.0, 3)),
        )
    }
}

fn fraction_to_block(fraction: f64, len: usize) -> String {
    const BLOCK_EIGTHS: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

    let mut out = String::new();
    let perblock = 1f64 / len as f64;
    for block in 0..len {
        let remaining = fraction - perblock * block as f64;
        let scaled = remaining / perblock;
        // +.0001 to ensure inaccuracy errs on the side of 100% rather than 7/8th.
        let index = ((scaled * 9.0001).floor() as usize).min(8);
        out.push_str(BLOCK_EIGTHS[index]);
    }
    out
}

fn colored_fraction<T>(fraction: f64, content: T) -> Formatted<'static, T>
where
    T: fmt::Display,
{
    const FORMAT_LOW: &str = "\x1b[37;40m";
    const FORMAT_MID: &str = "\x1b[97;40m";
    const FORMAT_HI: &str = "\x1b[31;40m";

    let color = match fraction {
        f if f > 0.9 => FORMAT_HI,
        f if f > 0.3 => FORMAT_MID,
        _ => FORMAT_LOW,
    };

    Formatted(color, content)
}

/// Render a fraction as a line of blocks, coloring the filled area.
///
/// If emoji output or colors are disabled, an ASCII version will be shown.
///
/// ```text
/// ■■■■■■■■■■   (with colors & emoji support)
/// ====------   (either of them disabled)
/// ```
///
/// The first parameter is the desired line length.
pub struct FractionAsLine {
    length: u8,
    fraction: f64,
    background: &'static str,
    colors: &'static [(f64, &'static str)],
}

const DEFAULT_LINE_BACKGROUND: &str = "\x1b[38;5;246m";
const DEFAULT_LINE_FOREGROUND: &str = "\x1b[97m";
const DEFAULT_LINE_COLORS: &[(f64, &str)] = &[(0., "\x1b[37m"), (0.3, "\x1b[97m")];

impl FractionAsLine {
    pub const fn new(fraction: f64) -> Self {
        Self {
            length: 10,
            fraction,
            colors: DEFAULT_LINE_COLORS,
            background: DEFAULT_LINE_BACKGROUND,
        }
    }

    pub const fn length(self, length: u8) -> Self {
        Self { length, ..self }
    }

    pub const fn background(self, background: &'static str) -> Self {
        Self { background, ..self }
    }

    pub const fn colors(self, colors: &'static [(f64, &'static str)]) -> Self {
        Self { colors, ..self }
    }
}

impl fmt::Display for FractionAsLine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let filled = (self.length as f64 * self.fraction).clamp(0., self.length as f64) as u8;

        if !use_emoji() || !env().use_color() {
            for _ in 0..filled {
                f.write_str("=")?;
            }
            for _ in filled..self.length {
                f.write_str("-")?;
            }
        } else {
            if filled > 0 {
                let color = self
                    .colors
                    .iter()
                    .filter_map(|(from, color)| (self.fraction >= *from).then_some(*color))
                    .next()
                    .unwrap_or(DEFAULT_LINE_FOREGROUND);
                f.write_str(color)?;
                for _ in 0..filled {
                    f.write_str("■")?;
                }
            }
            if filled != self.length {
                f.write_str(self.background)?;
                for _ in filled..self.length {
                    f.write_str("■")?;
                }
            }
            f.write_str(COLOR_RESET)?;
        }
        Ok(())
    }
}
