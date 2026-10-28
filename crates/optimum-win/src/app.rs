use std::collections::VecDeque;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Instant;

use optimum_core::{
    CellSize, ClickTiming, Config, GridSize, Key, Mods, MouseAction, MouseButton, MouseEvent,
    MouseGeometry, Pointer, PtyEvent, Scroll, SelectionGeometry, Session, WindowConfig, cell_at,
    grid_size, scale, wrap,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{ScreenToClient, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{
    MK_CONTROL, MK_LBUTTON, MK_MBUTTON, MK_RBUTTON, MK_XBUTTON1, MK_XBUTTON2,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::Ime::ISC_SHOWUICOMPOSITIONWINDOW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetDoubleClickTime, ReleaseCapture, SetCapture, VK_F4, VK_PROCESSKEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCaretBlinkTime, GetClientRect, GetMessageW,
    GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HCURSOR, HTCLIENT, IDC_ARROW, IDC_IBEAM,
    IsIconic, IsZoomed, KillTimer, LoadCursorW, MINMAXINFO, MSG, PostMessageW, PostQuitMessage,
    RegisterClassExW, SC_KEYMENU, SM_CXDOUBLECLK, SPI_GETWHEELSCROLLLINES, SW_RESTORE,
    SW_SHOWDEFAULT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetCursor, SetForegroundWindow, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    WINDOWPLACEMENT, WM_APP, WM_CHAR, WM_CLOSE, WM_DEADCHAR, WM_DESTROY, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCACTIVATE,
    WM_NCCALCSIZE, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR,
    WM_SETFOCUS, WM_SIZE, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSDEADCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_NOREDIRECTIONBITMAP,
    WS_OVERLAPPEDWINDOW, XBUTTON1,
};
use windows::core::{PCWSTR, w};

use crate::clipboard;
use crate::ime::{self, Composition};
use crate::keyboard::{self, Chars, Command, Stroke};
use crate::renderer::{Renderer, View};
use crate::settings;
use crate::window::{self, frame_thickness, hit_test, point_from_lparam, rect_size};
use crate::{WinPty, WslStatus, instance};

const WM_APP_EVENTS: u32 = WM_APP + 1;
const WHEEL_DELTA: i32 = 120;
const HOLD_TIMER: usize = 1;
const HOLD_POLL_MS: u32 = 250;
const BLINK_TIMER: usize = 2;
const MIN_FONT_SIZE: f32 = 4.0;
const MAX_FONT_SIZE: f32 = 96.0;

pub fn run() -> ExitCode {
    // SAFETY: called once at startup, before any window exists.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let Some(_lock) = instance::acquire() else {
        instance::activate_existing();
        return ExitCode::SUCCESS;
    };

    let app = Box::into_raw(Box::new(App::new()));
    let hwnd = match create_window(app) {
        Some(hwnd) => hwnd,
        None => {
            // SAFETY: window creation failed, so no window procedure references `app`.
            drop(unsafe { Box::from_raw(app) });
            return ExitCode::FAILURE;
        }
    };

    // SAFETY: `app` is live until the message loop ends; each borrow below is released before the next call that can dispatch window messages.
    unsafe {
        (*app).attach(hwnd);
        show(hwnd);
        (*app).create_renderer();
        (*app).watch_settings();
        window::set_translucency(hwnd, (*app).config.window);
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        start_session((*app).grid(), Arc::clone(&(*app).events));
    }

    let mut msg = MSG::default();
    // SAFETY: standard message loop on the thread that owns the window.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(Box::from_raw(app));
    }
    ExitCode::SUCCESS
}

fn create_window(app: *mut App) -> Option<HWND> {
    let class = instance::class_name();
    // SAFETY: the class structure references a static window procedure and a live class name; `app` is passed through lpCreateParams to WM_NCCREATE.
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let dpi = GetDpiForSystem().max(1);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: module.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return None;
        }
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            &class,
            w!("Optimum"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(window::INITIAL_WIDTH, dpi) as i32,
            scale(window::INITIAL_HEIGHT, dpi) as i32,
            None,
            None,
            Some(module.into()),
            Some(app.cast()),
        )
        .ok()
    }
}

fn show(hwnd: HWND) {
    window::enable_frameless(hwnd);
    // SAFETY: `hwnd` is the live window created by this thread.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(1);
    window::set_icons(hwnd, dpi);
    let width = scale(window::INITIAL_WIDTH, dpi) as i32;
    let height = scale(window::INITIAL_HEIGHT, dpi) as i32;
    // SAFETY: `hwnd` is live; the flags only resize and re-run WM_NCCALCSIZE.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

fn start_session(size: GridSize, events: Arc<EventQueue>) {
    thread::spawn(move || {
        let status = WslStatus::detect();
        if status != WslStatus::Ready {
            events.push(AppEvent::WslUnavailable(status));
            return;
        }
        let pty_events = Arc::clone(&events);
        match WinPty::spawn_wsl(size, move |e| pty_events.push(AppEvent::Pty(e))) {
            Ok(pty) => events.push(AppEvent::Started(pty, size)),
            Err(e) => events.push(AppEvent::SpawnFailed(e.to_string())),
        }
    });
}

enum AppEvent {
    Started(WinPty, GridSize),
    WslUnavailable(WslStatus),
    SpawnFailed(String),
    Pty(PtyEvent),
    ConfigChanged,
}

struct EventQueue {
    events: Mutex<VecDeque<AppEvent>>,
    hwnd: Mutex<Option<usize>>,
}

impl EventQueue {
    fn push(&self, event: AppEvent) {
        let was_empty = {
            let mut events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
            let was_empty = events.is_empty();
            events.push_back(event);
            was_empty
        };
        let hwnd = *self.hwnd.lock().unwrap_or_else(PoisonError::into_inner);
        if let (true, Some(hwnd)) = (was_empty, hwnd) {
            // SAFETY: PostMessageW only enqueues; a stale handle makes it fail harmlessly.
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(hwnd as *mut _)),
                    WM_APP_EVENTS,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
    }

    fn drain(&self) -> VecDeque<AppEvent> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

struct App {
    hwnd: HWND,
    dpi: u32,
    activate_message: u32,
    renderer: Option<Renderer>,
    session: Option<Session<WinPty>>,
    pending_output: Vec<Vec<u8>>,
    unavailable: Option<String>,
    events: Arc<EventQueue>,
    focused: bool,
    preedit: String,
    settings_key_held: bool,
    swallowed: Vec<u16>,
    high_surrogate: Option<u16>,
    wheel: (i32, i32),
    ibeam: Option<HCURSOR>,
    arrow: Option<HCURSOR>,
    selecting: bool,
    extending: bool,
    zoom: f32,
    fullscreen: Option<WINDOWPLACEMENT>,
    started: Instant,
    config: Config,
    config_error: Option<String>,
    watching: bool,
    blink_on: bool,
    blinking: bool,
    #[cfg(debug_assertions)]
    tuner: Option<crate::tuner::Tuner>,
}

enum Reply {
    Value(LRESULT),
    Default,
    DefaultWith(LPARAM),
}

enum After {
    Nothing,
    DpiChanged(RECT, u32),
    Activate,
    Destroy,
    Capture,
    ReleaseCapture,
    PlaceIme(RECT),
    SetCursor(Option<HCURSOR>),
    Copy(String),
    Paste,
    SystemMenu,
    Fullscreen,
    OpenSettings,
    Translucency(WindowConfig),
    Both(Box<After>, Box<After>),
}

impl After {
    fn and(self, next: After) -> After {
        match (self, next) {
            (After::Nothing, next) => next,
            (first, After::Nothing) => first,
            (first, next) => After::Both(Box::new(first), Box::new(next)),
        }
    }
}

enum Input {
    None,
    Chars(Chars),
    Composition(Composition),
}

impl App {
    fn new() -> App {
        let (config, config_error) = match settings::load() {
            Ok(config) => (config, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        };
        App {
            hwnd: HWND::default(),
            dpi: optimum_core::BASE_DPI,
            activate_message: instance::activate_message(),
            renderer: None,
            session: None,
            pending_output: Vec::new(),
            unavailable: None,
            events: Arc::new(EventQueue {
                events: Mutex::new(VecDeque::new()),
                hwnd: Mutex::new(None),
            }),
            focused: false,
            preedit: String::new(),
            settings_key_held: false,
            swallowed: Vec::new(),
            high_surrogate: None,
            wheel: (0, 0),
            // SAFETY: loading a predefined system cursor has no preconditions.
            ibeam: unsafe { LoadCursorW(None, IDC_IBEAM) }.ok(),
            // SAFETY: loading a predefined system cursor has no preconditions.
            arrow: unsafe { LoadCursorW(None, IDC_ARROW) }.ok(),
            selecting: false,
            extending: false,
            zoom: 0.0,
            fullscreen: None,
            started: Instant::now(),
            config,
            config_error,
            watching: false,
            blink_on: true,
            blinking: false,
            #[cfg(debug_assertions)]
            tuner: None,
        }
    }

    fn attach(&mut self, hwnd: HWND) {
        self.hwnd = hwnd;
        *self
            .events
            .hwnd
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(hwnd.0 as usize);
        // SAFETY: `hwnd` is the live window created by this thread.
        self.dpi = unsafe { GetDpiForWindow(hwnd) }.max(1);
    }

    fn create_renderer(&mut self) {
        let (w, h) = self.client_size();
        self.renderer = Renderer::new(self.hwnd, w, h, self.dpi, &self.shown_config()).ok();
        self.render();
    }

    fn shown_config(&self) -> Config {
        let mut config = self.config.clone();
        config.font.size = (config.font.size + self.zoom).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        config
    }

    fn zoom(&mut self, step: Option<f32>) {
        let size = self.config.font.size;
        let wanted = step.map_or(size, |step| size + self.zoom + step);
        let zoom = wanted.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE) - size;
        if zoom == self.zoom {
            return;
        }
        self.zoom = zoom;
        let shown = self.shown_config();
        if let Some(renderer) = &mut self.renderer
            && renderer.configure(&shown).is_err()
        {
            self.renderer = None;
            let (w, h) = self.client_size();
            self.renderer = Renderer::new(self.hwnd, w, h, self.dpi, &shown).ok();
        }
        self.resize_session();
        self.render();
    }

    fn client_size(&self) -> (u32, u32) {
        let mut rect = RECT::default();
        // SAFETY: `self.hwnd` is live; `rect` is a valid out-pointer.
        let _ = unsafe { GetClientRect(self.hwnd, &mut rect) };
        rect_size(&rect)
    }

    fn cell(&self) -> CellSize {
        self.renderer
            .as_ref()
            .map_or_else(|| CellSize::placeholder(self.dpi), Renderer::cell)
    }

    fn padding(&self) -> u32 {
        scale(self.config.window.padding, self.dpi)
    }

    fn watch_settings(&mut self) {
        if self.watching {
            return;
        }
        let events = Arc::clone(&self.events);
        self.watching = settings::watch(move || events.push(AppEvent::ConfigChanged));
    }

    fn reload(&mut self) -> After {
        let config = match settings::load() {
            Ok(config) => config,
            Err(e) => {
                self.config_error = Some(e.to_string());
                self.render();
                return After::Nothing;
            }
        };
        self.config_error = None;
        if config == self.config {
            self.render();
            return After::Nothing;
        }
        let window = config.window;
        self.config = config;
        let shown = self.shown_config();
        if let Some(renderer) = &mut self.renderer
            && renderer.configure(&shown).is_err()
        {
            self.renderer = None;
            let (w, h) = self.client_size();
            self.renderer = Renderer::new(self.hwnd, w, h, self.dpi, &shown).ok();
        }
        if let Some(session) = &mut self.session {
            let _ = session.configure(&self.config);
        }
        self.resize_session();
        self.render();
        After::Translucency(window)
    }

    #[cfg(debug_assertions)]
    fn tune(&mut self, key: Key, mods: Mods) -> bool {
        use crate::tuner::{Change, Tuner, is_toggle};
        if is_toggle(key, mods) {
            self.tuner = match (&self.tuner, &self.renderer) {
                (None, Some(renderer)) => {
                    Some(Tuner::new(renderer.text_params(), self.config.font.clone()))
                }
                _ => None,
            };
            self.render();
            return true;
        }
        let Some(change) = self.tuner.as_mut().and_then(|t| t.key(key)) else {
            return false;
        };
        match change {
            Change::None => {}
            Change::Close => self.tuner = None,
            Change::Text(params) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_text_params(params);
                }
            }
            Change::Font(font) => {
                self.config.font = font;
                let text = self.tuner.as_ref().map(|t| t.text);
                let shown = self.shown_config();
                if let Some(renderer) = &mut self.renderer
                    && renderer.configure(&shown).is_ok()
                    && let Some(text) = text
                {
                    renderer.set_text_params(text);
                }
                self.resize_session();
            }
        }
        self.render();
        true
    }

    fn update_blink(&mut self) {
        let blinking = self.focused
            && self
                .session
                .as_ref()
                .and_then(|s| s.frame().cursor)
                .is_some_and(|c| c.blinking);
        if blinking == self.blinking {
            return;
        }
        self.blinking = blinking;
        self.blink_on = true;
        // SAFETY: `self.hwnd` is live; the timer only posts WM_TIMER to this window.
        unsafe {
            if blinking {
                SetTimer(
                    Some(self.hwnd),
                    BLINK_TIMER,
                    GetCaretBlinkTime().max(100),
                    None,
                );
            } else {
                let _ = KillTimer(Some(self.hwnd), BLINK_TIMER);
            }
        }
    }

    fn grid(&self) -> GridSize {
        let (w, h) = self.client_size();
        grid_size(w, h, self.padding(), self.cell())
    }

    fn render(&mut self) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let message = match (&self.session, &self.unavailable) {
            (None, Some(message)) => Some(message.as_str()),
            _ => None,
        };
        let overlay = message.map_or_else(Vec::new, |m| wrap(m, renderer.overlay_columns()));
        #[cfg(debug_assertions)]
        let overlay = match &self.tuner {
            Some(tuner) if message.is_none() => {
                let cell = renderer.cell();
                let p = tuner.text;
                tuner.lines(&format!(
                    "dpi {}  cell {}x{}  {}  ct level {:.2}",
                    self.dpi, cell.width, cell.height, tuner.font.family, p.cleartype_level
                ))
            }
            _ => overlay,
        };
        let frame = self.session.as_mut().map(|session| {
            let _ = session.refresh();
            session.frame_mut()
        });
        let view = View {
            focused: self.focused,
            preedit: &self.preedit,
            overlay: &overlay,
            error: self.config_error.as_deref(),
            cursor_on: self.blink_on,
        };
        if renderer.render(frame, self.config.colors, view).is_err() {
            let (w, h) = self.client_size();
            self.renderer = Renderer::new(self.hwnd, w, h, self.dpi, &self.shown_config()).ok();
        }
        self.update_blink();
    }

    fn resize_session(&mut self) {
        let grid = self.grid();
        let cell = self.cell();
        if let Some(session) = &mut self.session {
            let _ = session.resize(grid, cell.width, cell.height);
        }
    }

    fn maximized(&self) -> bool {
        // SAFETY: `self.hwnd` is live.
        unsafe { IsZoomed(self.hwnd) }.as_bool()
    }

    fn mouse_geometry(&self) -> MouseGeometry {
        let (width, height) = self.client_size();
        let cell = self.cell();
        let grid = self.grid();
        let padding = self.padding();
        MouseGeometry {
            width,
            height,
            cell_width: cell.width,
            cell_height: cell.height,
            padding_left: padding,
            padding_top: padding,
            padding_right: width.saturating_sub(padding + u32::from(grid.cols) * cell.width),
            padding_bottom: height.saturating_sub(padding + u32::from(grid.rows) * cell.height),
        }
    }

    fn cursor_rect(&self) -> RECT {
        let cell = self.cell();
        let padding = self.padding() as i32;
        let (x, y) = self
            .session
            .as_ref()
            .and_then(|s| s.frame().cursor)
            .map_or((0, 0), |c| (i32::from(c.x), i32::from(c.y)));
        let left = padding + x * cell.width as i32;
        let top = padding + y * cell.height as i32;
        RECT {
            left,
            top,
            right: left + cell.width as i32,
            bottom: top + cell.height as i32,
        }
    }

    fn key_down(
        &mut self,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        chars: Chars,
    ) -> (Reply, After) {
        let handled = Reply::Value(LRESULT(0));
        let stroke = Stroke::new(wparam, lparam, true);
        let mods = keyboard::mods();
        if msg == WM_SYSKEYDOWN && stroke.vk == VK_F4.0 && mods.binding() == Mods::ALT {
            return (Reply::Default, After::Nothing);
        }
        if stroke.vk == VK_PROCESSKEY.0 || chars.dead {
            return (handled, After::Nothing);
        }
        #[cfg(debug_assertions)]
        if self.tune(stroke.key, mods) {
            return (handled, After::Nothing);
        }
        if keyboard::is_settings(stroke.key, mods) {
            let open = stroke.action == optimum_core::KeyAction::Press;
            self.settings_key_held = true;
            return (
                handled,
                if open {
                    After::OpenSettings
                } else {
                    After::Nothing
                },
            );
        }
        if let Some(command) = keyboard::command(stroke.vk, mods) {
            if self.swallowed.contains(&stroke.vk) {
                return (handled, After::Nothing);
            }
            if let Some(after) = self.command(command) {
                self.swallowed.push(stroke.vk);
                self.render();
                return (handled, after);
            }
        }
        self.blink_on = true;
        let ctrl_alt = Mods::CTRL | Mods::ALT;
        let stripped = (!keyboard::printable(&chars.text) && mods.intersects(ctrl_alt))
            .then(|| keyboard::translate(stroke, true, false))
            .flatten();
        let unshifted = keyboard::translate(stroke, false, true).and_then(single_char);
        let event = keyboard::event(stroke, mods, &chars.text, stripped.as_deref(), unshifted);
        if let Some(session) = &mut self.session {
            let _ = session.key(&event);
        }
        (handled, After::Nothing)
    }

    fn command(&mut self, command: Command) -> Option<After> {
        let zoom = match command {
            Command::ZoomIn => Some(Some(1.0)),
            Command::ZoomOut => Some(Some(-1.0)),
            Command::ZoomReset => Some(None),
            Command::Fullscreen => return Some(After::Fullscreen),
            Command::SystemMenu => return Some(After::SystemMenu),
            _ => None,
        };
        if let Some(step) = zoom {
            self.zoom(step);
            return Some(After::Nothing);
        }
        let session = self.session.as_mut()?;
        let selection = session.selection();
        let history = !session.alternate_screen();
        let after = match command {
            Command::CopyOrInterrupt | Command::Copy => match selection {
                Some(text) => {
                    session.clear_selection();
                    After::Copy(text)
                }
                None if command == Command::Copy => After::Nothing,
                None => return None,
            },
            Command::Paste => After::Paste,
            Command::SelectAll => {
                let _ = session.select_all();
                After::Nothing
            }
            Command::Escape => {
                selection?;
                session.clear_selection();
                After::Nothing
            }
            Command::Extend(adjust) => {
                selection?;
                let _ = session.adjust_selection(adjust);
                After::Nothing
            }
            Command::ScrollPage(pages) if history => {
                session.scroll_pages(pages);
                After::Nothing
            }
            Command::ScrollLine(lines) if history => {
                session.scroll(Scroll::Lines(lines));
                After::Nothing
            }
            Command::ScrollTop if history => {
                session.scroll(Scroll::Top);
                After::Nothing
            }
            Command::ScrollBottom if history => {
                session.scroll(Scroll::Bottom);
                After::Nothing
            }
            Command::ScrollPage(_)
            | Command::ScrollLine(_)
            | Command::ScrollTop
            | Command::ScrollBottom
            | Command::SystemMenu
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ZoomReset
            | Command::Fullscreen => return None,
        };
        Some(after)
    }

    fn key_up(&mut self, wparam: WPARAM, lparam: LPARAM) -> Reply {
        let stroke = Stroke::new(wparam, lparam, false);
        if self.settings_key_held && stroke.key == Key::Comma {
            self.settings_key_held = false;
            return Reply::Value(LRESULT(0));
        }
        if stroke.vk == VK_PROCESSKEY.0 {
            return Reply::Value(LRESULT(0));
        }
        if let Some(at) = self.swallowed.iter().position(|&vk| vk == stroke.vk) {
            self.swallowed.swap_remove(at);
            return Reply::Value(LRESULT(0));
        }
        let unshifted = keyboard::translate(stroke, false, true).and_then(single_char);
        let event = keyboard::event(stroke, keyboard::mods(), "", None, unshifted);
        if let Some(session) = &mut self.session {
            let _ = session.key(&event);
        }
        Reply::Value(LRESULT(0))
    }

    fn char(&mut self, unit: u16) {
        let units = match (self.high_surrogate.take(), unit) {
            (_, 0xd800..=0xdbff) => {
                self.high_surrogate = Some(unit);
                return;
            }
            (Some(high), 0xdc00..=0xdfff) => vec![high, unit],
            _ => vec![unit],
        };
        let text = String::from_utf16_lossy(&units);
        if let (true, Some(session)) = (keyboard::printable(&text), &mut self.session) {
            let _ = session.text(&text);
        }
    }

    fn composition(&mut self, composition: Composition) {
        if let (Some(result), Some(session)) = (composition.result, &mut self.session)
            && !result.is_empty()
        {
            let _ = session.text(&result);
        }
        if let Some(preedit) = composition.preedit {
            self.preedit = preedit;
        } else {
            self.preedit.clear();
        }
        self.render();
    }

    fn reports_mouse(&self, mods: Mods) -> bool {
        !mods.contains(Mods::SHIFT) && self.session.as_ref().is_some_and(Session::mouse_tracking)
    }

    fn report(
        &mut self,
        action: MouseAction,
        button: Option<MouseButton>,
        (x, y): (i32, i32),
        any_pressed: bool,
    ) {
        let mods = keyboard::mods();
        let geometry = self.mouse_geometry();
        if let Some(session) = &mut self.session {
            let event = MouseEvent {
                action,
                button,
                mods,
                x: x as f32,
                y: y as f32,
            };
            let _ = session.mouse(&event, geometry, any_pressed);
        }
    }

    fn pointer(&self, (x, y): (i32, i32)) -> Pointer {
        let padding = self.padding();
        let (col, row) = cell_at(x, y, padding, self.cell(), self.grid());
        Pointer {
            col,
            row,
            x: f64::from(x),
            y: f64::from(y),
        }
    }

    fn click_timing(&self) -> ClickTiming {
        // SAFETY: both queries have no preconditions.
        let (interval_ms, distance) =
            unsafe { (GetDoubleClickTime(), GetSystemMetrics(SM_CXDOUBLECLK)) };
        ClickTiming {
            time_ns: self.started.elapsed().as_nanos() as u64,
            interval_ns: u64::from(interval_ms) * 1_000_000,
            distance: f64::from(distance.max(2)) / 2.0,
        }
    }

    fn selection_geometry(&self) -> SelectionGeometry {
        let (_, height) = self.client_size();
        SelectionGeometry {
            columns: u32::from(self.grid().cols),
            cell_width: self.cell().width,
            padding_left: self.padding(),
            screen_height: height,
        }
    }

    fn local_button(
        &mut self,
        button: MouseButton,
        action: MouseAction,
        point: (i32, i32),
    ) -> After {
        let pointer = self.pointer(point);
        let timing = self.click_timing();
        let Some(session) = &mut self.session else {
            return After::Nothing;
        };
        let shift = keyboard::mods().binding() == Mods::SHIFT;
        let after = match (button, action) {
            (MouseButton::Left, MouseAction::Press)
                if shift && session.extend_selection(pointer).unwrap_or(false) =>
            {
                self.extending = true;
                After::Nothing
            }
            (MouseButton::Left, MouseAction::Press) => {
                self.selecting = session.select_press(pointer, timing).is_ok();
                After::Nothing
            }
            (MouseButton::Left, MouseAction::Release) if self.extending => {
                self.extending = false;
                session.selection().map_or(After::Nothing, After::Copy)
            }
            (MouseButton::Left, MouseAction::Release) if self.selecting => {
                self.selecting = false;
                match session.select_release(pointer) {
                    Ok(Some(text)) => After::Copy(text),
                    _ => After::Nothing,
                }
            }
            (MouseButton::Right, MouseAction::Press) => After::Paste,
            _ => After::Nothing,
        };
        self.render();
        after
    }

    fn mouse_button(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> (Reply, After) {
        let x_button = if ((wparam.0 >> 16) & 0xffff) as u16 == XBUTTON1 {
            MouseButton::Back
        } else {
            MouseButton::Forward
        };
        let (button, action) = match msg {
            WM_LBUTTONDOWN => (MouseButton::Left, MouseAction::Press),
            WM_LBUTTONUP => (MouseButton::Left, MouseAction::Release),
            WM_RBUTTONDOWN => (MouseButton::Right, MouseAction::Press),
            WM_RBUTTONUP => (MouseButton::Right, MouseAction::Release),
            WM_MBUTTONDOWN => (MouseButton::Middle, MouseAction::Press),
            WM_MBUTTONUP => (MouseButton::Middle, MouseAction::Release),
            WM_XBUTTONDOWN => (x_button, MouseAction::Press),
            _ => (x_button, MouseAction::Release),
        };
        let held = held_buttons(wparam);
        let point = point_from_lparam(lparam);
        let local = if self.reports_mouse(keyboard::mods()) && !self.selecting && !self.extending {
            self.report(action, Some(button), point, held.is_some());
            After::Nothing
        } else {
            self.local_button(button, action, point)
        };
        let capture = match (action, held) {
            (MouseAction::Press, _) => After::Capture,
            (MouseAction::Release, None) => After::ReleaseCapture,
            _ => After::Nothing,
        };
        let reply = if matches!(msg, WM_XBUTTONDOWN | WM_XBUTTONUP) {
            LRESULT(1)
        } else {
            LRESULT(0)
        };
        (Reply::Value(reply), capture.and(local))
    }

    fn mouse_move(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let held = held_buttons(wparam);
        let point = point_from_lparam(lparam);
        if (self.selecting || self.extending) && held == Some(MouseButton::Left) {
            let pointer = self.pointer(point);
            let geometry = self.selection_geometry();
            if let Some(session) = &mut self.session {
                let _ = if self.extending {
                    session.extend_selection(pointer).map(|_| ())
                } else {
                    session.select_drag(pointer, geometry)
                };
            }
            self.render();
        } else if self.reports_mouse(keyboard::mods()) {
            self.report(MouseAction::Motion, held, point, held.is_some());
        }
    }

    fn mouse_wheel(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) {
        let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
        let (x, y) = point_from_lparam(lparam);
        let mut point = POINT { x, y };
        // SAFETY: `self.hwnd` is live and `point` is a valid in/out pointer.
        let _ = unsafe { ScreenToClient(self.hwnd, &mut point) };
        let vertical = msg == WM_MOUSEWHEEL;
        let total = if vertical {
            &mut self.wheel.0
        } else {
            &mut self.wheel.1
        };
        *total += delta;
        let steps = *total / WHEEL_DELTA;
        *total %= WHEEL_DELTA;
        if steps == 0 {
            return;
        }
        if vertical && wparam.0 & MK_CONTROL.0 as usize != 0 {
            self.zoom(Some(steps as f32));
            return;
        }
        let tracking = self.session.as_ref().is_some_and(Session::mouse_tracking);
        if !tracking {
            if vertical && let Some(session) = &mut self.session {
                let _ = session.wheel(steps, wheel_lines());
                self.render();
            }
            return;
        }
        let button = match (vertical, steps > 0) {
            (true, true) => MouseButton::WheelUp,
            (true, false) => MouseButton::WheelDown,
            (false, true) => MouseButton::WheelRight,
            (false, false) => MouseButton::WheelLeft,
        };
        let held = held_buttons(wparam).is_some();
        for _ in 0..steps.unsigned_abs() {
            self.report(MouseAction::Press, Some(button), (point.x, point.y), held);
        }
    }

    fn paste(&mut self, text: &str) {
        if let Some(session) = &mut self.session {
            let _ = session.paste(text);
        }
    }

    fn hold_tick(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        if session.expire_hold(Instant::now()) {
            self.render();
        }
        if !self.session.as_ref().is_some_and(Session::held) {
            // SAFETY: `self.hwnd` is live; killing a timer that is not set is harmless.
            let _ = unsafe { KillTimer(Some(self.hwnd), HOLD_TIMER) };
        }
    }

    fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(session) = &mut self.session {
            let _ = session.focus(focused);
        }
        self.render();
    }

    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM, input: Input) -> (Reply, After) {
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let chars = match input {
                    Input::Chars(chars) => chars,
                    _ => Chars::default(),
                };
                self.key_down(msg, wparam, lparam, chars)
            }
            WM_KEYUP | WM_SYSKEYUP => (self.key_up(wparam, lparam), After::Nothing),
            WM_CHAR | WM_SYSCHAR => {
                self.char(wparam.0 as u16);
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_DEADCHAR | WM_SYSDEADCHAR => (Reply::Value(LRESULT(0)), After::Nothing),
            WM_IME_SETCONTEXT => (
                Reply::DefaultWith(LPARAM(lparam.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize))),
                After::Nothing,
            ),
            WM_IME_STARTCOMPOSITION => (
                Reply::Value(LRESULT(0)),
                After::PlaceIme(self.cursor_rect()),
            ),
            WM_IME_COMPOSITION => {
                if let Input::Composition(composition) = input {
                    self.composition(composition);
                }
                (
                    Reply::Value(LRESULT(0)),
                    After::PlaceIme(self.cursor_rect()),
                )
            }
            WM_IME_ENDCOMPOSITION => {
                self.preedit.clear();
                self.render();
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_SETFOCUS => {
                self.focus(true);
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_KILLFOCUS => {
                self.focus(false);
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN
            | WM_MBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP => {
                self.mouse_button(msg, wparam, lparam)
            }
            WM_MOUSEMOVE => {
                self.mouse_move(wparam, lparam);
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_TIMER if wparam.0 == BLINK_TIMER => {
                self.blink_on = !self.blink_on;
                self.render();
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_TIMER if wparam.0 == HOLD_TIMER => {
                self.hold_tick();
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                self.mouse_wheel(msg, wparam, lparam);
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_SETCURSOR if (lparam.0 & 0xffff) as u32 == HTCLIENT => {
                let tracking = self.session.as_ref().is_some_and(Session::mouse_tracking);
                let cursor = if tracking { self.arrow } else { self.ibeam };
                (Reply::Value(LRESULT(1)), After::SetCursor(cursor))
            }
            WM_NCCALCSIZE if wparam.0 != 0 => {
                window::calc_size(
                    lparam,
                    self.maximized() && self.fullscreen.is_none(),
                    self.dpi,
                );
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_NCHITTEST if self.fullscreen.is_some() => {
                (Reply::Value(LRESULT(HTCLIENT as isize)), After::Nothing)
            }
            WM_NCHITTEST => {
                let mut rect = RECT::default();
                // SAFETY: `self.hwnd` is live; `rect` is a valid out-pointer.
                let _ = unsafe { GetWindowRect(self.hwnd, &mut rect) };
                let border = frame_thickness(self.dpi).0;
                let drag = self.padding() as i32;
                let hit = hit_test(
                    point_from_lparam(lparam),
                    rect,
                    border,
                    drag,
                    self.maximized(),
                );
                (Reply::Value(LRESULT(hit as isize)), After::Nothing)
            }
            WM_NCACTIVATE => (Reply::DefaultWith(LPARAM(-1)), After::Nothing),
            WM_ERASEBKGND => (Reply::Value(LRESULT(1)), After::Nothing),
            WM_SIZE => {
                let (w, h) = (
                    (lparam.0 & 0xffff) as u32,
                    ((lparam.0 >> 16) & 0xffff) as u32,
                );
                if let Some(renderer) = &mut self.renderer
                    && renderer.resize(w, h).is_err()
                {
                    self.renderer =
                        Renderer::new(self.hwnd, w, h, self.dpi, &self.shown_config()).ok();
                }
                self.resize_session();
                self.render();
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_PAINT => {
                self.render();
                // SAFETY: `self.hwnd` is live; validating the whole client area ends this paint cycle.
                let _ = unsafe { ValidateRect(Some(self.hwnd), None) };
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_DPICHANGED => {
                self.dpi = (wparam.0 & 0xffff) as u32;
                if let Some(renderer) = &mut self.renderer
                    && renderer.set_dpi(self.dpi).is_err()
                {
                    self.renderer = None;
                }
                self.resize_session();
                self.render();
                // SAFETY: for WM_DPICHANGED, lParam points to the suggested window RECT.
                let suggested = unsafe { *(lparam.0 as *const RECT) };
                (
                    Reply::Value(LRESULT(0)),
                    After::DpiChanged(suggested, self.dpi),
                )
            }
            WM_GETMINMAXINFO => {
                // SAFETY: for WM_GETMINMAXINFO, lParam points to a writable MINMAXINFO.
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                info.ptMinTrackSize.x = scale(window::MIN_WIDTH, self.dpi) as i32;
                info.ptMinTrackSize.y = scale(window::MIN_HEIGHT, self.dpi) as i32;
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            WM_APP_EVENTS => (Reply::Value(LRESULT(0)), self.process_events()),
            WM_CLOSE => (Reply::Value(LRESULT(0)), After::Destroy),
            WM_DESTROY => {
                self.session = None;
                self.renderer = None;
                *self
                    .events
                    .hwnd
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = None;
                // SAFETY: called on the thread that runs the message loop.
                unsafe { PostQuitMessage(0) };
                (Reply::Value(LRESULT(0)), After::Nothing)
            }
            m if m == self.activate_message && m != 0 => {
                (Reply::Value(LRESULT(0)), After::Activate)
            }
            _ => (Reply::Default, After::Nothing),
        }
    }

    fn process_events(&mut self) -> After {
        let mut redraw = false;
        let mut after = After::Nothing;
        for event in self.events.drain() {
            match event {
                AppEvent::ConfigChanged => after = self.reload().and(after),
                AppEvent::Started(pty, size) => match Session::new(pty, size, &self.config) {
                    Ok(mut session) => {
                        for output in self.pending_output.drain(..) {
                            let _ = session.feed(&output);
                        }
                        self.session = Some(session);
                        self.resize_session();
                        redraw = true;
                    }
                    Err(e) => self.unavailable = Some(e.to_string()),
                },
                AppEvent::WslUnavailable(status) => {
                    self.unavailable = status.message().map(str::to_owned);
                }
                AppEvent::SpawnFailed(e) => self.unavailable = Some(e),
                AppEvent::Pty(PtyEvent::Output(bytes)) => match &mut self.session {
                    Some(session) => {
                        let _ = session.feed(&bytes);
                        redraw = true;
                    }
                    None => self.pending_output.push(bytes),
                },
                AppEvent::Pty(PtyEvent::Exited(_)) => after = After::Destroy,
            }
        }
        if let Some(session) = &mut self.session {
            if let Some(text) = session.take_clipboard() {
                after = After::Copy(text).and(after);
            }
            if session.held() {
                // SAFETY: `self.hwnd` is live; re-arming the same timer id only resets it.
                unsafe { SetTimer(Some(self.hwnd), HOLD_TIMER, HOLD_POLL_MS, None) };
            }
        }
        if redraw && !matches!(after, After::Destroy) {
            self.render();
        }
        after
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        // SAFETY: for WM_NCCREATE, lParam points to the CREATESTRUCTW whose lpCreateParams is the App passed to CreateWindowExW.
        unsafe {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let app = create.lpCreateParams.cast::<App>();
            (*app).hwnd = hwnd;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
    }

    // SAFETY: GWLP_USERDATA is either null or the App pointer stored at WM_NCCREATE, which outlives the window.
    let app = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut App;
    if app.is_null() {
        // SAFETY: plain default processing for messages that arrive before WM_NCCREATE.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    let input = match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => Input::Chars(keyboard::take_chars(hwnd)),
        WM_IME_COMPOSITION => Input::Composition(ime::read(hwnd, lparam)),
        _ => Input::None,
    };

    // SAFETY: the App is only accessed from this thread, and this mutable borrow ends before any call below that can re-enter the window procedure.
    let (reply, after) = unsafe { (*app).handle(msg, wparam, lparam, input) };

    // SAFETY: `app` is the live App of this window; `perform` borrows it only for calls that do not re-enter.
    unsafe { perform(hwnd, app, after) };

    // SAFETY: `hwnd` is the live window this procedure was called for.
    unsafe {
        match reply {
            Reply::Value(value) => value,
            Reply::Default => DefWindowProcW(hwnd, msg, wparam, lparam),
            Reply::DefaultWith(lparam) => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn perform(hwnd: HWND, app: *mut App, after: After) {
    // SAFETY: `hwnd` is the live window; `app` is borrowed only for `paste`, after the clipboard has been read and closed.
    unsafe {
        match after {
            After::Nothing => {}
            After::DpiChanged(rect, dpi) => {
                window::set_icons(hwnd, dpi);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            After::Activate => {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
            }
            After::Destroy => {
                let _ = DestroyWindow(hwnd);
            }
            After::Capture => {
                SetCapture(hwnd);
            }
            After::ReleaseCapture => {
                let _ = ReleaseCapture();
            }
            After::PlaceIme(cursor) => ime::place(hwnd, cursor),
            After::SetCursor(cursor) => {
                SetCursor(cursor);
            }
            After::Copy(text) => clipboard::set(hwnd, &text),
            After::Paste => {
                if let Some(text) = clipboard::get(hwnd) {
                    (*app).paste(&text);
                }
            }
            After::Fullscreen => {
                let saved = (*app).fullscreen.take();
                let next = match saved {
                    Some(placement) => {
                        window::leave_fullscreen(hwnd, &placement);
                        None
                    }
                    None => window::enter_fullscreen(hwnd),
                };
                (*app).fullscreen = next;
            }
            After::SystemMenu => {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_SYSCOMMAND,
                    WPARAM(SC_KEYMENU as usize),
                    LPARAM(i32::from(b' ') as isize),
                );
            }
            After::OpenSettings => {
                settings::open();
                (*app).watch_settings();
                let after = (*app).reload();
                perform(hwnd, app, after);
            }
            After::Translucency(config) => window::set_translucency(hwnd, config),
            After::Both(first, next) => {
                perform(hwnd, app, *first);
                perform(hwnd, app, *next);
            }
        }
    }
}

fn wheel_lines() -> u32 {
    let mut lines = 3u32;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one u32 into `lines`.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some((&raw mut lines).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    lines.clamp(1, 100)
}

fn held_buttons(wparam: WPARAM) -> Option<MouseButton> {
    let flags = wparam.0 as u32;
    [
        (MK_LBUTTON, MouseButton::Left),
        (MK_MBUTTON, MouseButton::Middle),
        (MK_RBUTTON, MouseButton::Right),
        (MK_XBUTTON1, MouseButton::Back),
        (MK_XBUTTON2, MouseButton::Forward),
    ]
    .into_iter()
    .find(|(flag, _)| flags & flag.0 != 0)
    .map(|(_, button)| button)
}

fn single_char(text: String) -> Option<char> {
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}
