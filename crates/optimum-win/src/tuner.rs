use optimum_core::{FontConfig, Key, Mods};

use crate::font::{Antialias, TextParams};

const BAR: usize = 20;

pub(crate) fn is_toggle(key: Key, mods: Mods) -> bool {
    matches!(key, Key::F11 | Key::F12) && mods.binding() == Mods::CTRL | Mods::SHIFT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Antialias,
    Gamma,
    Contrast,
    GrayscaleContrast,
    ClearTypeLevel,
    Size,
    LineHeight,
}

const FIELDS: [Field; 7] = [
    Field::Antialias,
    Field::Gamma,
    Field::Contrast,
    Field::GrayscaleContrast,
    Field::ClearTypeLevel,
    Field::Size,
    Field::LineHeight,
];

impl Field {
    fn name(self) -> &'static str {
        match self {
            Field::Antialias => "antialiasing",
            Field::Gamma => "gamma",
            Field::Contrast => "cleartype contrast",
            Field::GrayscaleContrast => "grayscale contrast",
            Field::ClearTypeLevel => "cleartype level",
            Field::Size => "font size",
            Field::LineHeight => "line height",
        }
    }

    fn range(self) -> (f32, f32, f32) {
        match self {
            Field::Antialias => (0.0, 1.0, 1.0),
            Field::Gamma => (1.0, 2.2, 0.1),
            Field::Contrast | Field::GrayscaleContrast => (0.0, 10.0, 0.1),
            Field::ClearTypeLevel => (0.0, 1.0, 0.1),
            Field::Size => (4.0, 96.0, 0.5),
            Field::LineHeight => (0.5, 3.0, 0.05),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Change {
    None,
    Text(TextParams),
    Font(FontConfig),
    Close,
}

pub(crate) struct Tuner {
    selected: usize,
    pub text: TextParams,
    pub font: FontConfig,
}

impl Tuner {
    pub fn new(text: TextParams, font: FontConfig) -> Tuner {
        Tuner {
            selected: 0,
            text,
            font,
        }
    }

    fn value(&self, field: Field) -> f32 {
        match field {
            Field::Antialias => f32::from(u8::from(self.text.antialias == Antialias::ClearType)),
            Field::Gamma => self.text.gamma,
            Field::Contrast => self.text.enhanced_contrast,
            Field::GrayscaleContrast => self.text.grayscale_enhanced_contrast,
            Field::ClearTypeLevel => self.text.cleartype_level,
            Field::Size => self.font.size,
            Field::LineHeight => self.font.line_height,
        }
    }

    fn set(&mut self, field: Field, value: f32) -> Change {
        match field {
            Field::Antialias => {
                self.text.antialias = if value >= 0.5 {
                    Antialias::ClearType
                } else {
                    Antialias::Grayscale
                };
            }
            Field::Gamma => self.text.gamma = value,
            Field::Contrast => self.text.enhanced_contrast = value,
            Field::GrayscaleContrast => self.text.grayscale_enhanced_contrast = value,
            Field::ClearTypeLevel => self.text.cleartype_level = value,
            Field::Size => {
                self.font.size = value;
                return Change::Font(self.font.clone());
            }
            Field::LineHeight => {
                self.font.line_height = value;
                return Change::Font(self.font.clone());
            }
        }
        Change::Text(self.text)
    }

    pub fn key(&mut self, key: Key) -> Option<Change> {
        let field = FIELDS[self.selected];
        let (min, max, step) = field.range();
        let adjust = |value: f32, direction: f32| {
            let next = ((value + direction * step) / step).round() * step;
            (next * 100.0).round() / 100.0
        };
        let key = match key {
            Key::NumpadUp => Key::ArrowUp,
            Key::NumpadDown => Key::ArrowDown,
            Key::NumpadLeft => Key::ArrowLeft,
            Key::NumpadRight => Key::ArrowRight,
            other => other,
        };
        Some(match key {
            Key::ArrowUp => {
                self.selected = (self.selected + FIELDS.len() - 1) % FIELDS.len();
                Change::None
            }
            Key::ArrowDown => {
                self.selected = (self.selected + 1) % FIELDS.len();
                Change::None
            }
            Key::ArrowLeft | Key::ArrowRight => {
                let direction = if key == Key::ArrowRight { 1.0 } else { -1.0 };
                let next = adjust(self.value(field), direction).clamp(min, max);
                if next == self.value(field) {
                    Change::None
                } else {
                    self.set(field, next)
                }
            }
            Key::Escape => Change::Close,
            _ => return None,
        })
    }

    pub fn lines(&self, info: &str) -> Vec<String> {
        let mut lines = vec![
            "clarity tuner (debug build)".to_owned(),
            "up/down select · left/right adjust · Esc close".to_owned(),
            "toggle: Ctrl+Shift+F11 or Ctrl+Shift+F12".to_owned(),
            String::new(),
        ];
        for (i, &field) in FIELDS.iter().enumerate() {
            let (min, max, _) = field.range();
            let value = self.value(field);
            let shown = match field {
                Field::Antialias if value >= 0.5 => "cleartype".to_owned(),
                Field::Antialias => "grayscale".to_owned(),
                _ => format!("{value:.2}"),
            };
            let filled = (((value - min) / (max - min)) * BAR as f32).round() as usize;
            let marker = if i == self.selected { '>' } else { ' ' };
            lines.push(format!(
                "{marker} {:<19}{shown:>10}  {}{}",
                field.name(),
                "█".repeat(filled.min(BAR)),
                "░".repeat(BAR - filled.min(BAR)),
            ));
        }
        lines.push(String::new());
        lines.push(info.to_owned());
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        lines
            .into_iter()
            .map(|l| {
                let pad = width - l.chars().count();
                l + &" ".repeat(pad)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use optimum_core::Config;

    use super::*;

    fn tuner() -> Tuner {
        Tuner::new(
            TextParams {
                antialias: Antialias::ClearType,
                gamma: 1.8,
                enhanced_contrast: 0.5,
                grayscale_enhanced_contrast: 1.0,
                cleartype_level: 1.0,
            },
            Config::default().font,
        )
    }

    #[test]
    fn only_ctrl_shift_f12_toggles() {
        assert!(is_toggle(Key::F12, Mods::CTRL | Mods::SHIFT));
        assert!(!is_toggle(Key::F12, Mods::CTRL));
        assert!(is_toggle(Key::F11, Mods::CTRL | Mods::SHIFT));
        assert!(!is_toggle(Key::F10, Mods::CTRL | Mods::SHIFT));
    }

    #[test]
    fn arrows_select_and_adjust_within_range() {
        let mut t = tuner();
        assert_eq!(t.key(Key::NumpadDown), Some(Change::None));
        match t.key(Key::ArrowRight) {
            Some(Change::Text(p)) => assert_eq!(p.gamma, 1.9),
            other => panic!("{other:?}"),
        }
        for _ in 0..20 {
            t.key(Key::ArrowRight);
        }
        assert_eq!(t.text.gamma, 2.2);
        assert_eq!(t.key(Key::ArrowRight), Some(Change::None));

        t.key(Key::ArrowUp);
        assert!(
            matches!(t.key(Key::ArrowLeft), Some(Change::Text(p)) if p.antialias == Antialias::Grayscale)
        );

        t.key(Key::ArrowUp);
        assert!(matches!(t.key(Key::ArrowRight), Some(Change::Font(f)) if f.line_height == 1.05));
        assert_eq!(t.key(Key::Escape), Some(Change::Close));
        assert_eq!(t.key(Key::A), None);
    }

    #[test]
    fn lines_are_equal_width() {
        let lines = tuner().lines("dpi 144");
        let width = lines[0].chars().count();
        assert!(lines.iter().all(|l| l.chars().count() == width));
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("> antialiasing") && l.contains("cleartype"))
        );
    }
}
