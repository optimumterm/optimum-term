use std::cell::RefCell;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use dispatch2::{DispatchQueue, DispatchTime};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadOnly, Message as _};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent, NSMenu,
    NSMenuItem, NSPasteboard, NSPasteboardTypeString, NSWindow, NSWindowButton,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWindowTabbingMode, NSWindowTitleVisibility,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString, ns_string};
use objc2_quartz_core::CAMetalLayer;
use optimum_core::{
    CellSize, ClickTiming, Config, GridSize, KeyAction, Mods, MouseAction, MouseButton, MouseEvent,
    MouseGeometry, Pointer, PtyEvent, Scroll, SelectionGeometry, Session, View, cell_at, grid_size,
    wrap,
};

use crate::keyboard::{self, Command};
use crate::renderer::Renderer;
use crate::view::{AppDelegate, TerminalView};
use crate::{UnixPty, instance, login_shell, settings};

const TICK: Duration = Duration::from_millis(250);
const DEFAULT_GRID: GridSize = GridSize {
    cols: 100,
    rows: 30,
};
const MIN_FONT_SIZE: f32 = 4.0;
const MAX_FONT_SIZE: f32 = 96.0;
const CLICK_DISTANCE: f64 = 4.0;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

static INBOX: Inbox = Inbox {
    messages: Mutex::new(Vec::new()),
    scheduled: AtomicBool::new(false),
};

struct Inbox {
    messages: Mutex<Vec<Message>>,
    scheduled: AtomicBool,
}

enum Message {
    Pty(PtyEvent),
    ConfigChanged,
}

pub(crate) enum After {
    Copy(String),
    Paste,
    Fullscreen,
    Close,
    Minimize,
    Quit,
    Drag(Retained<NSEvent>),
    Zoom,
}

pub(crate) struct App {
    window: Retained<NSWindow>,
    view: Retained<TerminalView>,
    renderer: Option<Renderer>,
    session: Option<Session<UnixPty>>,
    config: Config,
    config_error: Option<String>,
    failure: Option<String>,
    preedit: String,
    focused: bool,
    blink_on: bool,
    blink_at: Instant,
    zoom: f32,
    scale: f64,
    selecting: bool,
    extending: bool,
    wheel: f64,
    pending: Vec<After>,
}

pub fn run() -> ExitCode {
    let Some(mtm) = MainThreadMarker::new() else {
        return ExitCode::FAILURE;
    };
    let Some(lock) = instance::acquire() else {
        instance::activate_existing();
        return ExitCode::SUCCESS;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    install_menu(&app, mtm);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
    drop(lock);
    ExitCode::SUCCESS
}

pub(crate) fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| {
        let mut app = cell.try_borrow_mut().ok()?;
        Some(f(app.as_mut()?))
    })
}

pub(crate) fn act<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    let result = with(|app| {
        let result = f(app);
        (result, std::mem::take(&mut app.pending))
    })?;
    let (result, pending) = result;
    for after in pending {
        perform(after);
    }
    Some(result)
}

fn post(message: Message) {
    INBOX
        .messages
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(message);
    if !INBOX.scheduled.swap(true, Ordering::AcqRel) {
        DispatchQueue::main().exec_async(drain);
    }
}

fn drain() {
    INBOX.scheduled.store(false, Ordering::Release);
    let messages = std::mem::take(
        &mut *INBOX
            .messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    );
    act(|app| {
        for message in messages {
            app.handle(message);
        }
        app.render();
    });
}

fn tick() {
    act(App::tick);
    let _ = DispatchQueue::main().after(DispatchTime::NOW.time(TICK.as_nanos() as i64), tick);
}

fn perform(after: After) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    match after {
        After::Copy(text) => {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            // SAFETY: NSPasteboardTypeString is a valid pasteboard type constant.
            pasteboard.setString_forType(&NSString::from_str(&text), unsafe {
                NSPasteboardTypeString
            });
        }
        After::Paste => {
            let pasteboard = NSPasteboard::generalPasteboard();
            // SAFETY: NSPasteboardTypeString is a valid pasteboard type constant.
            let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString });
            if let Some(text) = text {
                let text = text.to_string();
                act(|app| app.paste(&text));
            }
        }
        After::Quit => NSApplication::sharedApplication(mtm).terminate(None),
        After::Fullscreen => {
            if let Some(window) = with(|app| app.window.clone()) {
                window.toggleFullScreen(None);
            }
        }
        After::Close => {
            if let Some(window) = with(|app| app.window.clone()) {
                window.performClose(None);
            }
        }
        After::Minimize => {
            if let Some(window) = with(|app| app.window.clone()) {
                window.performMiniaturize(None);
            }
        }
        After::Drag(event) => {
            if let Some(window) = with(|app| app.window.clone()) {
                window.performWindowDragWithEvent(&event);
            }
        }
        After::Zoom => {
            if let Some(window) = with(|app| app.window.clone()) {
                window.performZoom(None);
            }
        }
    }
}

pub(crate) fn start(mtm: MainThreadMarker) {
    let (config, config_error) = match settings::load() {
        Ok(config) => (config, None),
        Err(e) => (Config::default(), Some(e.to_string())),
    };
    let window = create_window(mtm, &config);
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0));
    let view = TerminalView::new(mtm, frame);
    let layer = CAMetalLayer::new();
    view.setLayer(Some(&layer));
    view.setWantsLayer(true);
    window.setContentView(Some(&view));
    window.makeFirstResponder(Some(&view));

    let scale = window.backingScaleFactor();
    layer.setContentsScale(scale);
    let renderer = Renderer::new(layer, 1, 1, scale, &config);
    if let Some(renderer) = &renderer {
        let cell = renderer.cell();
        let padding = f64::from(renderer.padding());
        let width = f64::from(u32::from(DEFAULT_GRID.cols) * cell.width) + 2.0 * padding;
        let height = f64::from(u32::from(DEFAULT_GRID.rows) * cell.height) + 2.0 * padding;
        window.setContentSize(NSSize::new(width / scale, height / scale));
    }
    window.center();
    window.makeKeyAndOrderFront(None);
    NSApplication::sharedApplication(mtm).activate();

    let app = App {
        window,
        view,
        renderer,
        session: None,
        config,
        config_error,
        failure: None,
        preedit: String::new(),
        focused: true,
        blink_on: true,
        blink_at: Instant::now(),
        zoom: 0.0,
        scale,
        selecting: false,
        extending: false,
        wheel: 0.0,
        pending: Vec::new(),
    };
    APP.with(|cell| *cell.borrow_mut() = Some(app));
    act(|app| {
        app.resize();
        app.spawn();
        app.render();
    });
    settings::watch(|| post(Message::ConfigChanged));
    tick();
}

fn create_window(mtm: MainThreadMarker, config: &Config) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0));
    // SAFETY: the window is kept alive by the App and is not released on close.
    let window = unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setReleasedWhenClosed(false);
        window
    };
    NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTitlebarAppearsTransparent(true);
    window.setTitle(ns_string!("Optimum"));
    window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenPrimary);
    hide_titlebar(&window);
    apply_opacity(&window, config.window.opacity);
    window.setDelegate(Some(ProtocolObject::from_ref(&*AppDelegate::shared(mtm))));
    window
}

fn hide_titlebar(window: &NSWindow) {
    for button in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = window.standardWindowButton(button) {
            button.setHidden(true);
        }
    }
    let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
        return;
    };
    // SAFETY: both are plain view hierarchy reads on live views.
    let container = unsafe { close.superview().and_then(|titlebar| titlebar.superview()) };
    if let Some(container) = container {
        container.setHidden(true);
    }
}

fn apply_opacity(window: &NSWindow, opacity: f32) {
    let opaque = opacity >= 1.0;
    window.setOpaque(opaque);
    if !opaque {
        window.setBackgroundColor(Some(&NSColor::clearColor()));
    }
}

fn install_menu(app: &NSApplication, mtm: MainThreadMarker) {
    let bar = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let menu = NSMenu::new(mtm);
    for (title, action, key) in [
        ("Settings…", objc2::sel!(openSettings:), ","),
        ("Hide Optimum", objc2::sel!(hide:), "h"),
        ("Quit Optimum", objc2::sel!(terminate:), "q"),
    ] {
        // SAFETY: the selectors exist on the responder chain (the view, NSApplication).
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        menu.addItem(&item);
    }
    app_item.setSubmenu(Some(&menu));
    bar.addItem(&app_item);
    app.setMainMenu(Some(&bar));
}

impl App {
    fn handle(&mut self, message: Message) {
        match message {
            Message::Pty(PtyEvent::Output(bytes)) => {
                if let Some(session) = &mut self.session {
                    let _ = session.feed(&bytes);
                    if let Some(text) = session.take_clipboard() {
                        self.pending.push(After::Copy(text));
                    }
                }
            }
            Message::Pty(PtyEvent::Exited(_)) => {
                self.session = None;
                self.pending.push(After::Quit);
            }
            Message::ConfigChanged => self.reload(),
        }
    }

    fn spawn(&mut self) {
        let grid = self.grid();
        let shell = login_shell();
        let spawned = UnixPty::spawn_login_shell(&shell, grid, |event| post(Message::Pty(event)))
            .map_err(|e| e.to_string())
            .and_then(|pty| Session::new(pty, grid, &self.config).map_err(|e| e.to_string()));
        match spawned {
            Ok(session) => self.session = Some(session),
            Err(e) => {
                self.failure = Some(format!("Could not start {}: {e}", shell.program.display()))
            }
        }
    }

    fn reload(&mut self) {
        let config = match settings::load() {
            Ok(config) => config,
            Err(e) => {
                self.config_error = Some(e.to_string());
                return;
            }
        };
        self.config_error = None;
        if config == self.config {
            return;
        }
        self.config = config;
        apply_opacity(&self.window, self.config.window.opacity);
        let shown = self.shown_config();
        if let Some(renderer) = &mut self.renderer {
            renderer.configure(&shown);
        }
        if let Some(session) = &mut self.session {
            let _ = session.configure(&self.config);
        }
        self.resize();
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
        if let Some(renderer) = &mut self.renderer {
            renderer.configure(&shown);
        }
        self.resize();
    }

    pub(crate) fn rescale(&mut self) {
        let scale = self.window.backingScaleFactor();
        if scale == self.scale {
            return;
        }
        self.scale = scale;
        if let Some(renderer) = &mut self.renderer {
            renderer.set_scale(scale);
        }
        self.resize();
    }

    fn size_px(&self) -> (u32, u32) {
        let bounds = self.view.bounds();
        (
            (bounds.size.width * self.scale).round().max(1.0) as u32,
            (bounds.size.height * self.scale).round().max(1.0) as u32,
        )
    }

    fn cell(&self) -> CellSize {
        self.renderer.as_ref().map_or(
            CellSize {
                width: 8,
                height: 16,
            },
            Renderer::cell,
        )
    }

    fn padding(&self) -> u32 {
        self.renderer.as_ref().map_or(0, Renderer::padding)
    }

    fn grid(&self) -> GridSize {
        let (w, h) = self.size_px();
        grid_size(w, h, self.padding(), self.cell())
    }

    pub(crate) fn resize(&mut self) {
        let (w, h) = self.size_px();
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(w, h);
        }
        let grid = self.grid();
        let cell = self.cell();
        if let Some(session) = &mut self.session {
            let _ = session.resize(grid, cell.width, cell.height);
        }
        self.render();
    }

    pub(crate) fn render(&mut self) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let overlay = match (&self.session, &self.failure) {
            (None, Some(message)) => wrap(message, renderer.overlay_columns()),
            _ => Vec::new(),
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
        renderer.render(frame, self.config.colors, view);
    }

    fn tick(&mut self) {
        let mut dirty = false;
        if let Some(session) = &mut self.session {
            dirty |= session.expire_hold(Instant::now());
        }
        let blinking = self
            .session
            .as_ref()
            .and_then(|s| s.frame().cursor)
            .is_some_and(|c| c.blinking);
        if blinking && self.focused && self.blink_at.elapsed() >= 2 * TICK {
            self.blink_on = !self.blink_on;
            self.blink_at = Instant::now();
            dirty = true;
        }
        if dirty {
            self.render();
        }
    }

    pub(crate) fn focus(&mut self, focused: bool) {
        self.focused = focused;
        self.blink_on = true;
        self.blink_at = Instant::now();
        if let Some(session) = &mut self.session {
            let _ = session.focus(focused);
        }
        self.render();
    }

    pub(crate) fn command(&mut self, code: u16, mods: Mods) -> bool {
        let Some(command) = keyboard::command(code, mods) else {
            return false;
        };
        let zoom = match command {
            Command::ZoomIn => Some(Some(1.0)),
            Command::ZoomOut => Some(Some(-1.0)),
            Command::ZoomReset => Some(None),
            _ => None,
        };
        if let Some(step) = zoom {
            self.zoom(step);
            return true;
        }
        match command {
            Command::Fullscreen => {
                self.pending.push(After::Fullscreen);
                return true;
            }
            Command::Settings => {
                settings::open();
                return true;
            }
            Command::Close => {
                self.pending.push(After::Close);
                return true;
            }
            Command::Minimize => {
                self.pending.push(After::Minimize);
                return true;
            }
            _ => {}
        }
        let Some(session) = &mut self.session else {
            return false;
        };
        let selection = session.selection();
        let history = !session.alternate_screen();
        match command {
            Command::Copy => {
                if let Some(text) = selection {
                    self.pending.push(After::Copy(text));
                }
            }
            Command::Paste => self.pending.push(After::Paste),
            Command::SelectAll => {
                let _ = session.select_all();
            }
            Command::Escape if selection.is_some() => session.clear_selection(),
            Command::Extend(adjust) if selection.is_some() => {
                let _ = session.adjust_selection(adjust);
            }
            Command::ScrollPage(pages) if history => session.scroll_pages(pages),
            Command::ScrollLine(lines) if history => session.scroll(Scroll::Lines(lines)),
            Command::ScrollTop if history => session.scroll(Scroll::Top),
            Command::ScrollBottom if history => session.scroll(Scroll::Bottom),
            _ => return false,
        }
        self.render();
        true
    }

    pub(crate) fn key(&mut self, event: &NSEvent, action: KeyAction, typed: &str) {
        let code = event.keyCode();
        let key = keyboard::key(code);
        let mods = keyboard::mods(event.modifierFlags().0);
        let ignoring = event
            .charactersIgnoringModifiers()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let unshifted = event
            .charactersByApplyingModifiers(objc2_app_kit::NSEventModifierFlags::empty())
            .and_then(|s| keyboard::single_char(&s.to_string()));
        let typed = if action == KeyAction::Release {
            ""
        } else {
            typed
        };
        let ignoring = if action == KeyAction::Release {
            ""
        } else {
            ignoring.as_str()
        };
        let key_event = keyboard::event(key, action, mods, typed, ignoring, unshifted);
        if action != KeyAction::Release {
            self.blink_on = true;
            self.blink_at = Instant::now();
        }
        if let Some(session) = &mut self.session {
            let _ = session.key(&key_event);
        }
        self.render();
    }

    pub(crate) fn text(&mut self, text: &str) {
        if let Some(session) = &mut self.session
            && keyboard::printable(text)
        {
            let _ = session.text(text);
        }
        self.render();
    }

    pub(crate) fn set_preedit(&mut self, text: &str) {
        self.preedit = text.to_owned();
        self.render();
    }

    pub(crate) fn preedit_length(&self) -> usize {
        self.preedit.encode_utf16().count()
    }

    pub(crate) fn has_preedit(&self) -> bool {
        !self.preedit.is_empty()
    }

    pub(crate) fn cursor_rect(&self) -> NSRect {
        let cell = self.cell();
        let padding = f64::from(self.padding());
        let (x, y) = self
            .session
            .as_ref()
            .and_then(|s| s.frame().cursor)
            .map_or((0, 0), |c| (c.x, c.y));
        let left = padding + f64::from(x) * f64::from(cell.width);
        let top = padding + f64::from(y) * f64::from(cell.height);
        NSRect::new(
            NSPoint::new(left / self.scale, top / self.scale),
            NSSize::new(
                f64::from(cell.width) / self.scale,
                f64::from(cell.height) / self.scale,
            ),
        )
    }

    pub(crate) fn paste(&mut self, text: &str) {
        if let Some(session) = &mut self.session {
            let _ = session.paste(text);
        }
        self.render();
    }

    fn point_px(&self, point: NSPoint) -> (i32, i32) {
        (
            (point.x * self.scale).round() as i32,
            (point.y * self.scale).round() as i32,
        )
    }

    fn pointer(&self, (x, y): (i32, i32)) -> Pointer {
        let (col, row) = cell_at(x, y, self.padding(), self.cell(), self.grid());
        Pointer {
            col,
            row,
            x: f64::from(x),
            y: f64::from(y),
        }
    }

    fn mouse_geometry(&self) -> MouseGeometry {
        let (width, height) = self.size_px();
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

    fn selection_geometry(&self) -> SelectionGeometry {
        let (_, height) = self.size_px();
        SelectionGeometry {
            columns: u32::from(self.grid().cols),
            cell_width: self.cell().width,
            padding_left: self.padding(),
            screen_height: height,
        }
    }

    fn reports_mouse(&self, mods: Mods) -> bool {
        !mods.contains(Mods::SHIFT) && self.session.as_ref().is_some_and(Session::mouse_tracking)
    }

    fn report(
        &mut self,
        action: MouseAction,
        button: Option<MouseButton>,
        (x, y): (i32, i32),
        mods: Mods,
        any_pressed: bool,
    ) {
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

    pub(crate) fn mouse(
        &mut self,
        event: &NSEvent,
        point: NSPoint,
        button: MouseButton,
        action: MouseAction,
    ) {
        let point = self.point_px(point);
        let mods = keyboard::mods(event.modifierFlags().0);
        let in_strip = point.1 < self.padding() as i32;
        if button == MouseButton::Left && action == MouseAction::Press && in_strip {
            let after = if event.clickCount() == 2 {
                After::Zoom
            } else {
                After::Drag(event.retain())
            };
            self.pending.push(after);
            return;
        }
        if self.reports_mouse(mods) && !self.selecting && !self.extending {
            self.report(
                action,
                Some(button),
                point,
                mods,
                action == MouseAction::Press,
            );
            return;
        }
        let pointer = self.pointer(point);
        let timing = ClickTiming {
            time_ns: (event.timestamp() * 1e9) as u64,
            interval_ns: (NSEvent::doubleClickInterval() * 1e9) as u64,
            distance: CLICK_DISTANCE * self.scale,
        };
        let Some(session) = &mut self.session else {
            return;
        };
        let shift = mods.binding() == Mods::SHIFT;
        match (button, action) {
            (MouseButton::Left, MouseAction::Press)
                if shift && session.extend_selection(pointer).unwrap_or(false) =>
            {
                self.extending = true;
            }
            (MouseButton::Left, MouseAction::Press) => {
                self.selecting = session.select_press(pointer, timing).is_ok();
            }
            (MouseButton::Left, MouseAction::Release) if self.extending => {
                self.extending = false;
                if let Some(text) = session.selection() {
                    self.pending.push(After::Copy(text));
                }
            }
            (MouseButton::Left, MouseAction::Release) if self.selecting => {
                self.selecting = false;
                if let Ok(Some(text)) = session.select_release(pointer) {
                    self.pending.push(After::Copy(text));
                }
            }
            (MouseButton::Right, MouseAction::Press) => self.pending.push(After::Paste),
            _ => {}
        }
        self.render();
    }

    pub(crate) fn mouse_drag(&mut self, event: &NSEvent, point: NSPoint, button: MouseButton) {
        let point = self.point_px(point);
        let mods = keyboard::mods(event.modifierFlags().0);
        if button == MouseButton::Left && (self.selecting || self.extending) {
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
        } else if self.reports_mouse(mods) {
            self.report(MouseAction::Motion, Some(button), point, mods, true);
        }
    }

    pub(crate) fn scroll(&mut self, event: &NSEvent, point: NSPoint) {
        let delta = event.scrollingDeltaY();
        let lines = if event.hasPreciseScrollingDeltas() {
            delta * self.scale / f64::from(self.cell().height.max(1))
        } else {
            delta
        };
        self.wheel += lines;
        let steps = self.wheel.trunc() as i32;
        self.wheel -= f64::from(steps);
        if steps == 0 {
            return;
        }
        let point = self.point_px(point);
        let mods = keyboard::mods(event.modifierFlags().0);
        if self.reports_mouse(mods) {
            let button = if steps > 0 {
                MouseButton::WheelUp
            } else {
                MouseButton::WheelDown
            };
            for _ in 0..steps.unsigned_abs() {
                self.report(MouseAction::Press, Some(button), point, mods, false);
            }
            return;
        }
        if let Some(session) = &mut self.session {
            let _ = session.wheel(steps, 1);
        }
        self.render();
    }
}
