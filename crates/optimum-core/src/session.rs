use std::time::{Duration, Instant};

use ghostty_vt::{
    ClickTiming, Key, KeyEncoder, KeyEvent, Mode, Mods, MouseEncoder, MouseEvent, MouseGeometry,
    Pointer, RenderState, Scroll, SelectionGeometry, Terminal, encode_focus, encode_paste,
};

const HOLD_LIMIT: Duration = Duration::from_secs(1);

use crate::{Config, Error, Frame, GridSize, Pty};

pub struct Session<P: Pty> {
    terminal: Terminal,
    render: RenderState,
    frame: Frame,
    pty: P,
    size: GridSize,
    keys: KeyEncoder,
    mouse: MouseEncoder,
    encoded: Vec<u8>,
    held_since: Option<Instant>,
}

impl<P: Pty> Session<P> {
    pub fn new(pty: P, size: GridSize, config: &Config) -> Result<Self, Error> {
        let terminal = Terminal::new(size.cols, size.rows)?;
        let mut session = Self {
            terminal,
            render: RenderState::new()?,
            frame: Frame::default(),
            pty,
            size,
            keys: KeyEncoder::new()?,
            mouse: MouseEncoder::new()?,
            encoded: Vec::new(),
            held_since: None,
        };
        session.configure(config)?;
        Ok(session)
    }

    pub fn configure(&mut self, config: &Config) -> Result<(), Error> {
        let theme = &config.colors;
        self.terminal.set_default_colors(
            theme.foreground.into(),
            theme.background.into(),
            theme.cursor.map(Into::into),
        )?;
        self.terminal
            .set_palette(&theme.palette256().map(Into::into))?;
        self.terminal
            .set_default_cursor(config.cursor.style, config.cursor.blink)?;
        self.frame.selection = theme.selection;
        self.render = RenderState::new()?;
        Ok(())
    }

    pub fn refresh(&mut self) -> Result<bool, Error> {
        if !self.terminal.held() {
            self.render.update(&self.terminal)?;
        }
        Ok(self.frame.update(&mut self.render)?)
    }

    pub fn held(&self) -> bool {
        self.terminal.held()
    }

    pub fn expire_hold(&mut self, now: Instant) -> bool {
        match self.held_since {
            Some(since) if now.duration_since(since) >= HOLD_LIMIT => {
                self.terminal.release_hold();
                self.held_since = None;
                true
            }
            _ => false,
        }
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }

    pub fn size(&self) -> GridSize {
        self.size
    }

    pub fn feed(&mut self, output: &[u8]) -> Result<(), Error> {
        self.terminal.write_capturing(output, &mut self.render);
        self.held_since = match (self.terminal.held(), self.held_since) {
            (true, None) => Some(Instant::now()),
            (true, since) => since,
            (false, _) => None,
        };
        let responses = self.terminal.take_responses();
        if !responses.is_empty() {
            self.pty.write(&responses)?;
        }
        Ok(())
    }

    pub fn input(&mut self, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.pty.write(bytes)?)
    }

    pub fn key(&mut self, event: &KeyEvent<'_>) -> Result<(), Error> {
        self.encoded.clear();
        self.keys.encode(&self.terminal, event, &mut self.encoded)?;
        if !self.encoded.is_empty() {
            self.terminal.scroll(Scroll::Bottom);
        }
        self.flush_encoded()
    }

    pub fn text(&mut self, text: &str) -> Result<(), Error> {
        self.terminal.scroll(Scroll::Bottom);
        Ok(self.pty.write(text.as_bytes())?)
    }

    pub fn paste(&mut self, text: &str) -> Result<(), Error> {
        let bracketed = self.terminal.mode(Mode::BracketedPaste);
        let encoded = encode_paste(text, bracketed)?;
        self.terminal.scroll(Scroll::Bottom);
        Ok(self.pty.write(&encoded)?)
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.terminal.take_clipboard()
    }

    pub fn wheel(&mut self, notches: i32, lines_per_notch: u32) -> Result<(), Error> {
        if self.terminal.alternate_screen() {
            if !self.terminal.mode(Mode::AltScroll) {
                return Ok(());
            }
            let key = if notches > 0 {
                Key::ArrowUp
            } else {
                Key::ArrowDown
            };
            self.encoded.clear();
            for _ in 0..notches.unsigned_abs() {
                self.keys.encode(
                    &self.terminal,
                    &KeyEvent::press(key, Mods::NONE),
                    &mut self.encoded,
                )?;
            }
            return self.flush_encoded();
        }
        let lines = -(notches as isize) * lines_per_notch as isize;
        self.terminal.scroll(Scroll::Lines(lines));
        Ok(())
    }

    pub fn select_press(&mut self, pointer: Pointer, timing: ClickTiming) -> Result<(), Error> {
        Ok(self.terminal.select_press(pointer, timing)?)
    }

    pub fn select_drag(
        &mut self,
        pointer: Pointer,
        geometry: SelectionGeometry,
    ) -> Result<(), Error> {
        Ok(self.terminal.select_drag(pointer, geometry)?)
    }

    pub fn select_release(&mut self, pointer: Pointer) -> Result<Option<String>, Error> {
        self.terminal.select_release(pointer)?;
        Ok(self
            .terminal
            .selection_text()
            .filter(|text| !text.is_empty()))
    }

    pub fn mouse_tracking(&self) -> bool {
        self.terminal.mouse_tracking()
    }

    pub fn mouse(
        &mut self,
        event: &MouseEvent,
        geometry: MouseGeometry,
        any_pressed: bool,
    ) -> Result<(), Error> {
        self.encoded.clear();
        self.mouse.encode(
            &self.terminal,
            event,
            geometry,
            any_pressed,
            &mut self.encoded,
        )?;
        self.flush_encoded()
    }

    pub fn focus(&mut self, gained: bool) -> Result<(), Error> {
        if !self.terminal.mode(Mode::FocusEvent) {
            return Ok(());
        }
        self.encoded.clear();
        encode_focus(gained, &mut self.encoded)?;
        self.flush_encoded()
    }

    fn flush_encoded(&mut self) -> Result<(), Error> {
        if self.encoded.is_empty() {
            return Ok(());
        }
        Ok(self.pty.write(&self.encoded)?)
    }

    pub fn resize(
        &mut self,
        size: GridSize,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), Error> {
        if size == self.size {
            return Ok(());
        }
        self.terminal
            .resize(size.cols, size.rows, cell_width_px, cell_height_px)?;
        self.pty.resize(size)?;
        self.size = size;
        Ok(())
    }

    pub fn screen_text(&self) -> Result<String, Error> {
        Ok(self.terminal.plain_text()?)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use ghostty_vt::{Key, Mods};

    use super::*;
    use crate::Theme;

    #[derive(Default)]
    struct FakePty {
        written: Vec<u8>,
        size: Option<GridSize>,
    }

    impl Pty for FakePty {
        fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.written.extend_from_slice(bytes);
            Ok(())
        }

        fn resize(&mut self, size: GridSize) -> io::Result<()> {
            self.size = Some(size);
            Ok(())
        }
    }

    const SIZE: GridSize = GridSize { cols: 80, rows: 24 };

    #[test]
    fn output_reaches_the_screen() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed(b"hello\r\nworld").unwrap();
        assert_eq!(session.screen_text().unwrap(), "hello\nworld");
    }

    #[test]
    fn terminal_replies_go_back_to_the_pty() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed(b"\x1b[6n").unwrap();
        assert_eq!(session.pty.written, b"\x1b[1;1R");
    }

    #[test]
    fn input_is_written_verbatim() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.input(b"ls\r").unwrap();
        assert_eq!(session.pty.written, b"ls\r");
    }

    #[test]
    fn keys_are_encoded_for_the_pty() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session
            .key(&KeyEvent::press(Key::ArrowUp, Mods::NONE))
            .unwrap();
        session.feed(b"\x1b[?1h").unwrap();
        session
            .key(&KeyEvent::press(Key::ArrowUp, Mods::NONE))
            .unwrap();
        assert_eq!(session.pty.written, b"\x1b[A\x1bOA");
    }

    #[test]
    fn selection_is_highlighted_and_returned() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed(b"hello world").unwrap();
        session.refresh().unwrap();
        session
            .frame_mut()
            .rows
            .iter_mut()
            .for_each(|r| r.dirty = false);
        let at = |col: u16, x: f64| Pointer {
            col,
            row: 0,
            x,
            y: 5.0,
        };
        let timing = ClickTiming {
            time_ns: 1,
            interval_ns: 1,
            distance: 1.0,
        };
        let geometry = SelectionGeometry {
            columns: 80,
            cell_width: 10,
            padding_left: 0,
            screen_height: 480,
        };
        session.select_press(at(0, 1.0), timing).unwrap();
        session.select_drag(at(4, 47.0), geometry).unwrap();
        let copied = session.select_release(at(4, 47.0)).unwrap();
        assert_eq!(copied.as_deref(), Some("hello"));

        assert!(session.refresh().unwrap());
        let row = &session.frame().rows[0];
        assert!(row.dirty);
        assert_eq!(row.cells[0].background, Some(Theme::default().selection));
        assert_eq!(row.cells[5].background, None);
    }

    #[test]
    fn configure_recolors_existing_cells() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed(b"\x1b[31mred\x1b[0m plain").unwrap();
        session.refresh().unwrap();
        assert_eq!(
            session.frame().rows[0].cells[0].foreground,
            Theme::default().palette[1]
        );

        let mut config = Config::default();
        config.colors.palette[1] = crate::Rgb::new(1, 2, 3);
        config.colors.foreground = crate::Rgb::new(4, 5, 6);
        session.configure(&config).unwrap();
        session.refresh().unwrap();
        let cells = &session.frame().rows[0].cells;
        assert_eq!(cells[0].foreground, crate::Rgb::new(1, 2, 3));
        assert_eq!(cells[4].foreground, crate::Rgb::new(4, 5, 6));
    }

    #[test]
    fn paste_respects_bracketed_mode() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.paste("a\nb").unwrap();
        session.feed(b"\x1b[?2004h").unwrap();
        session.paste("c").unwrap();
        assert_eq!(session.pty.written, b"a\rb\x1b[200~c\x1b[201~");
    }

    #[test]
    fn wheel_sends_arrows_on_the_alternate_screen() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed(b"\x1b[?1049h\x1b[?1007h").unwrap();
        session.wheel(2, 3).unwrap();
        session.wheel(-1, 3).unwrap();
        assert_eq!(session.pty.written, b"\x1b[A\x1b[A\x1b[B");
    }

    #[test]
    fn stuck_sync_blocks_are_released_after_a_second() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed(b"\x1b[?2026h").unwrap();
        assert!(session.held());
        assert!(!session.expire_hold(Instant::now()));
        assert!(session.expire_hold(Instant::now() + HOLD_LIMIT));
        assert!(!session.held());
    }

    #[test]
    fn focus_is_reported_only_when_requested() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.focus(true).unwrap();
        assert!(session.pty.written.is_empty());
        session.feed(b"\x1b[?1004h").unwrap();
        session.focus(false).unwrap();
        assert_eq!(session.pty.written, b"\x1b[O");
    }

    #[test]
    fn refresh_builds_the_frame_and_tracks_dirty_rows() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        session.feed("a日\x1b[7mb".as_bytes()).unwrap();
        assert!(session.refresh().unwrap());

        let frame = session.frame();
        assert_eq!(frame.rows.len(), 24);
        assert_eq!(frame.rows[0].cells.len(), 80);
        let cells = &frame.rows[0].cells;
        assert_eq!((cells[0].text.as_str(), cells[0].width), ("a", 1));
        assert_eq!((cells[1].text.as_str(), cells[1].width), ("日", 2));
        assert_eq!(cells[2].width, 0);
        assert_eq!(cells[3].foreground, Theme::default().background);
        assert_eq!(cells[3].background, Some(Theme::default().foreground));
        assert_eq!(frame.cursor.map(|c| (c.x, c.y)), Some((4, 0)));

        session
            .frame_mut()
            .rows
            .iter_mut()
            .for_each(|r| r.dirty = false);
        assert!(!session.refresh().unwrap());

        session.feed(b"\x1b[5;1Hz").unwrap();
        assert!(session.refresh().unwrap());
        let dirty: Vec<usize> = session
            .frame()
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.dirty)
            .map(|(y, _)| y)
            .collect();
        assert!(dirty.contains(&4));
        assert!(!dirty.contains(&10));
    }

    #[test]
    fn resize_updates_terminal_and_pty() {
        let mut session = Session::new(FakePty::default(), SIZE, &Config::default()).unwrap();
        let size = GridSize {
            cols: 100,
            rows: 40,
        };
        session.resize(size, 8, 16).unwrap();
        assert_eq!(session.size(), size);
        assert_eq!(session.pty.size, Some(size));
    }
}
