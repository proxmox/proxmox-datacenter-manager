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
}

#[derive(Clone, Copy, Debug)]
pub struct AnsiRgb<'a>(&'a Rgb);

impl fmt::Display for AnsiRgb<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Rgb { r, g, b } = *self.0;
        write!(f, "\x1b[38;2;{r};{g};{b}m")
    }
}
