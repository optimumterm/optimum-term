use std::fmt;
use std::ops::{Range, RangeInclusive};

use serde::Deserialize;
use serde::de::{self, Deserializer};
use toml::Spanned;

use crate::{CursorStyle, Rgb, Theme};

pub const DEFAULT_TOML: &str = r##"# Optimum settings. Changes apply as soon as the file is saved.
# Only look and feel can be configured here.

[font]
# Font family installed on the system. The built-in default is JetBrainsMono Nerd Font.
family = "JetBrainsMono Nerd Font"
# Size in points.
size = 12.0
# Line height as a multiple of the font's natural line height.
line_height = 1.0

[text]
# "system" | "cleartype" | "grayscale"
antialiasing = "system"
# "system" or a number from 1.0 to 2.2
gamma = "system"
# "system" or a number from 0.0 to 10.0
contrast = "system"

[colors]
foreground = "#d4d4d4"
background = "#121212"
# "foreground" or a color
cursor = "foreground"
selection = "#264f78"
# The 16 ANSI colors: black, red, green, yellow, blue, magenta, cyan, white,
# then their bright variants in the same order.
palette = [
  "#1d1f21", "#cc6666", "#b5bd68", "#f0c674", "#81a2be", "#b294bb", "#8abeb7", "#c5c8c6",
  "#666666", "#d54e53", "#b9ca4a", "#e7c547", "#7aa6da", "#c397d8", "#70c0b1", "#eaeaea",
]

[cursor]
# "block" | "bar" | "underline"
style = "block"
blink = false

[window]
# Padding around the text in pixels at 100% scale. The top padding is the drag strip.
padding = 12
# Window opacity from 0.1 to 1.0.
opacity = 1.0
# Blur what is behind the window when opacity is below 1.0 (Windows 11).
blur = false
"##;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Antialiasing {
    System,
    ClearType,
    Grayscale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextConfig {
    pub antialiasing: Antialiasing,
    pub gamma: Option<f32>,
    pub contrast: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorConfig {
    pub style: CursorStyle,
    pub blink: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowConfig {
    pub padding: u32,
    pub opacity: f32,
    pub blur: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub font: FontConfig,
    pub text: TextConfig,
    pub colors: Theme,
    pub cursor: CursorConfig,
    pub window: WindowConfig,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            font: FontConfig {
                family: "JetBrainsMono Nerd Font".to_owned(),
                size: 12.0,
                line_height: 1.0,
            },
            text: TextConfig {
                antialiasing: Antialiasing::System,
                gamma: None,
                contrast: None,
            },
            colors: Theme::default(),
            cursor: CursorConfig {
                style: CursorStyle::Block,
                blink: false,
            },
            window: WindowConfig {
                padding: 12,
                opacity: 1.0,
                blur: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "config.toml:{line}: {}", self.message),
            None => write!(f, "config.toml: {}", self.message),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ConfigError {
            line: e.span().map(|span| line_of(text, span)),
            message: first_line(e.message()),
        })?;
        raw.validate(text)
    }
}

fn line_of(text: &str, span: Range<usize>) -> usize {
    text[..span.start.min(text.len())].matches('\n').count() + 1
}

fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or(message).trim().to_owned()
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct Raw {
    font: RawFont,
    text: RawText,
    colors: RawColors,
    cursor: RawCursor,
    window: RawWindow,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawFont {
    family: Option<String>,
    size: Option<Spanned<f32>>,
    line_height: Option<Spanned<f32>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawText {
    antialiasing: Option<RawAntialiasing>,
    gamma: Option<Spanned<Level>>,
    contrast: Option<Spanned<Level>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawAntialiasing {
    System,
    ClearType,
    Grayscale,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Level {
    Named(String),
    Value(f32),
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawColors {
    foreground: Option<Color>,
    background: Option<Color>,
    cursor: Option<CursorColor>,
    selection: Option<Color>,
    palette: Option<Spanned<Vec<Color>>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawCursor {
    style: Option<RawCursorStyle>,
    blink: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawCursorStyle {
    Block,
    Bar,
    Underline,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct RawWindow {
    padding: Option<Spanned<u32>>,
    opacity: Option<Spanned<f32>>,
    blur: Option<bool>,
}

struct Color(Rgb);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_color(&text).map(Color).ok_or_else(|| {
            de::Error::custom(format!("invalid color {text:?}, expected \"#rrggbb\""))
        })
    }
}

enum CursorColor {
    Foreground,
    Color(Rgb),
}

impl<'de> Deserialize<'de> for CursorColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CursorColor, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "foreground" {
            return Ok(CursorColor::Foreground);
        }
        parse_color(&text).map(CursorColor::Color).ok_or_else(|| {
            de::Error::custom(format!(
                "invalid cursor color {text:?}, expected \"foreground\" or \"#rrggbb\""
            ))
        })
    }
}

pub fn parse_color(text: &str) -> Option<Rgb> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Rgb::new(channel(0)?, channel(2)?, channel(4)?))
}

fn ranged<T>(
    text: &str,
    value: Option<Spanned<T>>,
    range: RangeInclusive<T>,
    name: &str,
    default: T,
) -> Result<T, ConfigError>
where
    T: PartialOrd + Copy + fmt::Display,
{
    let Some(value) = value else {
        return Ok(default);
    };
    let span = value.span();
    let value = value.into_inner();
    if range.contains(&value) {
        Ok(value)
    } else {
        Err(ConfigError {
            line: Some(line_of(text, span)),
            message: format!(
                "{name} must be between {} and {}",
                range.start(),
                range.end()
            ),
        })
    }
}

fn level(
    text: &str,
    value: Option<Spanned<Level>>,
    range: RangeInclusive<f32>,
    name: &str,
) -> Result<Option<f32>, ConfigError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let span = value.span();
    match value.into_inner() {
        Level::Named(name) if name == "system" => Ok(None),
        Level::Value(v) if range.contains(&v) => Ok(Some(v)),
        _ => Err(ConfigError {
            line: Some(line_of(text, span)),
            message: format!(
                "{name} must be \"system\" or a number between {} and {}",
                range.start(),
                range.end()
            ),
        }),
    }
}

impl Raw {
    fn validate(self, text: &str) -> Result<Config, ConfigError> {
        let defaults = Config::default();
        let palette = match self.colors.palette {
            None => defaults.colors.palette,
            Some(palette) => {
                let span = palette.span();
                let colors = palette.into_inner();
                <[Color; 16]>::try_from(colors)
                    .map(|colors| colors.map(|c| c.0))
                    .map_err(|_| ConfigError {
                        line: Some(line_of(text, span)),
                        message: "palette must have exactly 16 colors".to_owned(),
                    })?
            }
        };
        let foreground = self
            .colors
            .foreground
            .map_or(defaults.colors.foreground, |c| c.0);
        Ok(Config {
            font: FontConfig {
                family: self
                    .font
                    .family
                    .filter(|f| !f.trim().is_empty())
                    .unwrap_or(defaults.font.family),
                size: ranged(text, self.font.size, 4.0..=96.0, "size", 12.0)?,
                line_height: ranged(text, self.font.line_height, 0.5..=3.0, "line_height", 1.0)?,
            },
            text: TextConfig {
                antialiasing: match self.text.antialiasing {
                    None | Some(RawAntialiasing::System) => Antialiasing::System,
                    Some(RawAntialiasing::ClearType) => Antialiasing::ClearType,
                    Some(RawAntialiasing::Grayscale) => Antialiasing::Grayscale,
                },
                gamma: level(text, self.text.gamma, 1.0..=2.2, "gamma")?,
                contrast: level(text, self.text.contrast, 0.0..=10.0, "contrast")?,
            },
            colors: Theme {
                foreground,
                background: self
                    .colors
                    .background
                    .map_or(defaults.colors.background, |c| c.0),
                cursor: match self.colors.cursor {
                    None | Some(CursorColor::Foreground) => None,
                    Some(CursorColor::Color(c)) => Some(c),
                },
                selection: self
                    .colors
                    .selection
                    .map_or(defaults.colors.selection, |c| c.0),
                palette,
            },
            cursor: CursorConfig {
                style: match self.cursor.style {
                    None | Some(RawCursorStyle::Block) => CursorStyle::Block,
                    Some(RawCursorStyle::Bar) => CursorStyle::Bar,
                    Some(RawCursorStyle::Underline) => CursorStyle::Underline,
                },
                blink: self.cursor.blink.unwrap_or(false),
            },
            window: WindowConfig {
                padding: ranged(text, self.window.padding, 0..=200, "padding", 12)?,
                opacity: ranged(text, self.window.opacity, 0.1..=1.0, "opacity", 1.0)?,
                blur: self.window.blur.unwrap_or(false),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_matches_default_config() {
        assert_eq!(Config::parse(DEFAULT_TOML).unwrap(), Config::default());
    }

    #[test]
    fn empty_file_is_all_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn values_are_read() {
        let config = Config::parse(
            "[font]\nfamily = \"Consolas\"\nsize = 14\n[text]\nantialiasing = \"grayscale\"\ngamma = 1.8\n[colors]\ncursor = \"#ff0000\"\n[cursor]\nstyle = \"bar\"\nblink = true\n[window]\npadding = 4\nopacity = 0.9\nblur = true\n",
        )
        .unwrap();
        assert_eq!(config.font.family, "Consolas");
        assert_eq!(config.font.size, 14.0);
        assert_eq!(config.text.antialiasing, Antialiasing::Grayscale);
        assert_eq!(config.text.gamma, Some(1.8));
        assert_eq!(config.colors.cursor, Some(Rgb::new(255, 0, 0)));
        assert_eq!(config.cursor.style, CursorStyle::Bar);
        assert!(config.cursor.blink);
        assert_eq!(config.window.padding, 4);
        assert_eq!(config.window.opacity, 0.9);
        assert!(config.window.blur);
    }

    #[test]
    fn errors_carry_line_numbers() {
        let error =
            Config::parse("[font]\nsize = 12.0\n\n[colors]\nforeground = \"red\"\n").unwrap_err();
        assert_eq!(error.line, Some(5));
        assert!(error.message.contains("invalid color"), "{error}");

        let error = Config::parse("[window]\nopacity = 2.0\n").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.message.contains("opacity"), "{error}");

        let error = Config::parse("[font]\nsise = 12\n").unwrap_err();
        assert_eq!(error.line, Some(2));

        let error = Config::parse("[text]\ngamma = \"high\"\n").unwrap_err();
        assert_eq!(error.line, Some(2));

        let error = Config::parse("[colors]\npalette = [\"#000000\"]\n").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert_eq!(
            error.to_string(),
            "config.toml:2: palette must have exactly 16 colors"
        );

        let error = Config::parse("[font\n").unwrap_err();
        assert_eq!(error.line, Some(1));
    }
}
