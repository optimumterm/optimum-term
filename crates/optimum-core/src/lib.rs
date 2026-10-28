mod color;
mod config;
mod error;
mod frame;
mod layout;
mod pty;
mod scene;
mod session;

pub use color::{DEFAULT_BACKGROUND, Rgb, Theme};
pub use config::{
    Antialiasing, Config, ConfigError, CursorConfig, DEFAULT_TOML, FontConfig, TextConfig,
    WindowConfig, parse_color,
};
pub use error::Error;
pub use frame::{Cell, Frame, FrameCursor, Row};
pub use ghostty_vt::{
    Adjust, ClickTiming, CursorStyle, Key, KeyAction, KeyEvent, Mods, MouseAction, MouseButton,
    MouseEvent, MouseGeometry, Pointer, Scroll, SelectionGeometry, codepoint_width,
};
pub use layout::{BASE_DPI, CellSize, DEFAULT_PADDING, cell_at, grid_size, scale, wrap};
pub use pty::{GridSize, Pty, PtyEvent};
pub use scene::{
    ATLAS_SIZE, Bitmap, CLEARTYPE, COLOR, CellMetrics, GRAYSCALE, GlyphSource, Instance, Layout,
    SOLID, Scene, Upload, View, overlay_columns, pack,
};
pub use session::Session;
