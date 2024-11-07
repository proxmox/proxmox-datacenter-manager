use std::fmt;

// Taken from proxmox-widget-toolkit's src/Utils.js:
//
//  stringToRGB: function(string) {
//      let hash = 0;
//      if (!string) {
//          return hash;
//      }
//      string += 'prox'; // give short strings more variance
//      for (let i = 0; i < string.length; i++) {
//          hash = string.charCodeAt(i) + ((hash << 5) - hash);
//          hash = hash & hash; // to int
//      }
//
//      let alpha = 0.7; // make the color a bit brighter
//      let bg = 255; // assume white background
//
//      return [
//          (hash & 255) * alpha + bg * (1 - alpha),
//          ((hash >> 8) & 255) * alpha + bg * (1 - alpha),
//          ((hash >> 16) & 255) * alpha + bg * (1 - alpha),
//      ];
//  },
pub fn text_to_rgb(text: &str) -> Option<Rgb> {
    if text.is_empty() {
        return None;
    }

    let mut hash = 0u32;
    for ch in text.chars() {
        hash = (ch as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
    }
    for ch in "prox".chars() {
        hash = (ch as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
    }

    let alpha = 0.7;
    let bg = 255.0;

    Some(Rgb {
        r: ((hash & 0xff) as f64 * alpha + bg * (1.0 - alpha)) as u8,
        g: (((hash >> 8) & 0xff) as f64 * alpha + bg * (1.0 - alpha)) as u8,
        b: (((hash >> 16) & 0xff) as f64 * alpha + bg * (1.0 - alpha)) as u8,
    })
}

/// Returns the best contrast color for the given RGB color value
///
/// returns either white rgb(255,255,255) or black rgb(0,0,0)
// Taken from proxmox-widget-toolkit's src/Utils.js:
//
// optimized & simplified SAPC function
// https://github.com/Myndex/SAPC-APCA
// getTextContrastClass: function(rgb) {
//     const blkThrs = 0.022;
//     const blkClmp = 1.414;
//
//     // linearize & gamma correction
//     let r = (rgb[0] / 255) ** 2.4;
//     let g = (rgb[1] / 255) ** 2.4;
//     let b = (rgb[2] / 255) ** 2.4;
//
//     // relative luminance sRGB
//     let bg = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
//
//     // black clamp
//     bg = bg > blkThrs ? bg : bg + (blkThrs - bg) ** blkClmp;
//
//     // SAPC with white text
//     let contrastLight = bg ** 0.65 - 1;
//     // SAPC with black text
//     let contrastDark = bg ** 0.56 - 0.046134502;
//
//     if (Math.abs(contrastLight) >= Math.abs(contrastDark)) {
//          'light';
//     } else {
//          'dark';
//     }
// },
pub fn get_best_contrast_color(color: &Rgb) -> Rgb {
    const BLACK_THRESHOLD: f64 = 0.022;
    const BLACK_CLAMP: f64 = 1.414;

    // linearize & gamma correct
    let r = (color.r as f64 / 255.0).powf(2.4);
    let g = (color.g as f64 / 255.0).powf(2.4);
    let b = (color.b as f64 / 255.0).powf(2.4);

    // relative luminance sRGB
    let bg = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;

    // black clamp
    let bg = if bg > BLACK_THRESHOLD {
        bg
    } else {
        bg + (BLACK_THRESHOLD - bg).powf(BLACK_CLAMP)
    };

    // SAPC with white text
    let light = bg.powf(0.65) - 1.0;

    // SAPC with black text
    let dark = bg.powf(0.56) - 0.046134502;

    if light.abs() >= dark.abs() {
        Rgb {
            r: 255,
            g: 255,
            b: 255,
        }
    } else {
        Rgb { r: 0, g: 0, b: 0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TtyResetColor;

impl fmt::Display for TtyResetColor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("\x1b[0m")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn as_ansi(&self) -> AnsiRgb<'_> {
        AnsiRgb(self)
    }

    pub fn as_css_rgb(&self) -> CssRgb<'_> {
        CssRgb(self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CssRgb<'a>(&'a Rgb);

impl fmt::Display for CssRgb<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Rgb { r, g, b } = *self.0;
        write!(f, "rgb({r}, {g}, {b})")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnsiRgb<'a>(&'a Rgb);

impl fmt::Display for AnsiRgb<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Rgb { r, g, b } = *self.0;
        write!(f, "\x1b[38;2;{r};{g};{b}m")
    }
}
