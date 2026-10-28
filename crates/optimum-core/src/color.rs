#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Rgb::new(
            lerp(self.r, other.r),
            lerp(self.g, other.g),
            lerp(self.b, other.b),
        )
    }

    pub fn to_f32(self) -> [f32; 4] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
            1.0,
        ]
    }
}

impl From<ghostty_vt::Rgb> for Rgb {
    fn from(c: ghostty_vt::Rgb) -> Rgb {
        Rgb::new(c.r, c.g, c.b)
    }
}

impl From<Rgb> for ghostty_vt::Rgb {
    fn from(c: Rgb) -> ghostty_vt::Rgb {
        ghostty_vt::Rgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Option<Rgb>,
    pub selection: Rgb,
    pub palette: [Rgb; 16],
}

impl Default for Theme {
    fn default() -> Theme {
        Theme {
            foreground: Rgb::new(0xd4, 0xd4, 0xd4),
            background: DEFAULT_BACKGROUND,
            cursor: None,
            selection: Rgb::new(0x26, 0x4f, 0x78),
            palette: [
                Rgb::new(0x1d, 0x1f, 0x21),
                Rgb::new(0xcc, 0x66, 0x66),
                Rgb::new(0xb5, 0xbd, 0x68),
                Rgb::new(0xf0, 0xc6, 0x74),
                Rgb::new(0x81, 0xa2, 0xbe),
                Rgb::new(0xb2, 0x94, 0xbb),
                Rgb::new(0x8a, 0xbe, 0xb7),
                Rgb::new(0xc5, 0xc8, 0xc6),
                Rgb::new(0x66, 0x66, 0x66),
                Rgb::new(0xd5, 0x4e, 0x53),
                Rgb::new(0xb9, 0xca, 0x4a),
                Rgb::new(0xe7, 0xc5, 0x47),
                Rgb::new(0x7a, 0xa6, 0xda),
                Rgb::new(0xc3, 0x97, 0xd8),
                Rgb::new(0x70, 0xc0, 0xb1),
                Rgb::new(0xea, 0xea, 0xea),
            ],
        }
    }
}

impl Theme {
    pub fn palette256(&self) -> [Rgb; 256] {
        let mut palette = [Rgb::default(); 256];
        palette[..16].copy_from_slice(&self.palette);
        let level = |v: usize| if v == 0 { 0 } else { (v * 40 + 55) as u8 };
        for (n, color) in palette[16..232].iter_mut().enumerate() {
            *color = Rgb::new(level(n / 36), level(n / 6 % 6), level(n % 6));
        }
        for (n, color) in palette[232..].iter_mut().enumerate() {
            let v = (n * 10 + 8) as u8;
            *color = Rgb::new(v, v, v);
        }
        palette
    }
}

pub const DEFAULT_BACKGROUND: Rgb = Rgb::new(0x12, 0x12, 0x12);

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn palette256_extends_the_ansi_colors_like_xterm() {
        let palette = Theme::default().palette256();
        assert_eq!(palette[1], Theme::default().palette[1]);
        assert_eq!(palette[16], Rgb::new(0, 0, 0));
        assert_eq!(palette[21], Rgb::new(0, 0, 255));
        assert_eq!(palette[196], Rgb::new(255, 0, 0));
        assert_eq!(palette[232], Rgb::new(8, 8, 8));
        assert_eq!(palette[255], Rgb::new(238, 238, 238));
    }
}
