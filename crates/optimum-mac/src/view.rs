use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{DefinedClass, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSEvent, NSResponder, NSTextInputClient, NSView,
    NSWindowDelegate,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSAttributedString, NSAttributedStringKey, NSNotFound,
    NSNotification, NSObject, NSPoint, NSRange, NSRect, NSSize, NSString, NSUInteger,
};
use optimum_core::{KeyAction, MouseAction, MouseButton};

use crate::app::{self, act, with};
use crate::keyboard;

thread_local! {
    static DELEGATE: OnceCell<Retained<AppDelegate>> = const { OnceCell::new() };
}

#[derive(Default)]
pub(crate) struct KeyInput {
    active: bool,
    text: String,
    composed: bool,
}

#[derive(Default)]
pub(crate) struct ViewIvars {
    input: RefCell<KeyInput>,
    swallowed: RefCell<Vec<u16>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and TerminalView does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ViewIvars]
    pub(crate) struct TerminalView;

    // SAFETY: each method matches the selector's Objective-C signature.
    impl TerminalView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            // SAFETY: forwards to NSView's implementation with the same argument.
            let _: () = unsafe { msg_send![super(self), setFrameSize: size] };
            act(app::App::resize);
        }

        #[unsafe(method(viewDidChangeBackingProperties))]
        fn did_change_backing(&self) {
            act(app::App::rescale);
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            let code = event.keyCode();
            let mods = keyboard::mods(event.modifierFlags().0);
            let handled = act(|app| app.command(code, mods)).unwrap_or(false);
            if handled {
                self.ivars().swallowed.borrow_mut().push(code);
            }
            handled
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let code = event.keyCode();
            let mods = keyboard::mods(event.modifierFlags().0);
            if act(|app| app.command(code, mods)).unwrap_or(false) {
                self.ivars().swallowed.borrow_mut().push(code);
                return;
            }
            let composing = with(|app| app.has_preedit()).unwrap_or(false);
            *self.ivars().input.borrow_mut() = KeyInput {
                active: true,
                ..KeyInput::default()
            };
            let events = NSArray::from_retained_slice(&[event.retain()]);
            self.interpretKeyEvents(&events);
            let input = std::mem::take(&mut *self.ivars().input.borrow_mut());
            if composing || input.composed {
                if !input.text.is_empty() {
                    act(|app| app.text(&input.text));
                }
                return;
            }
            let action = if event.isARepeat() {
                KeyAction::Repeat
            } else {
                KeyAction::Press
            };
            act(|app| app.key(event, action, &input.text));
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            let code = event.keyCode();
            let mut swallowed = self.ivars().swallowed.borrow_mut();
            if let Some(at) = swallowed.iter().position(|&c| c == code) {
                swallowed.swap_remove(at);
                return;
            }
            drop(swallowed);
            act(|app| app.key(event, KeyAction::Release, ""));
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.button(event, MouseButton::Left, MouseAction::Press);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.button(event, MouseButton::Left, MouseAction::Release);
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.button(event, MouseButton::Right, MouseAction::Press);
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.button(event, MouseButton::Right, MouseAction::Release);
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.button(event, MouseButton::Middle, MouseAction::Press);
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.button(event, MouseButton::Middle, MouseAction::Release);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let point = self.local(event);
            act(|app| app.mouse_drag(event, point, MouseButton::Left));
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            let point = self.local(event);
            act(|app| app.mouse_drag(event, point, MouseButton::Right));
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let point = self.local(event);
            act(|app| app.scroll(event, point));
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            crate::settings::open();
        }
    }

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for TerminalView {}

    // SAFETY: each method matches the protocol's Objective-C signature.
    unsafe impl NSTextInputClient for TerminalView {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, _range: NSRange) {
            let text = plain(string);
            let mut input = self.ivars().input.borrow_mut();
            let committing = with(|app| app.has_preedit()).unwrap_or(false);
            if committing {
                act(|app| app.set_preedit(""));
            }
            if input.active {
                input.text.push_str(&text);
                input.composed |= committing;
            } else {
                drop(input);
                act(|app| app.text(&text));
            }
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command(&self, _selector: Sel) {}

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, string: &AnyObject, _selected: NSRange, _replacement: NSRange) {
            let text = plain(string);
            self.ivars().input.borrow_mut().composed = true;
            act(|app| app.set_preedit(&text));
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            act(|app| app.set_preedit(""));
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            NSRange::new(NSNotFound as NSUInteger, 0)
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            let length = with(|app| app.preedit_length()).unwrap_or(0);
            if length == 0 {
                NSRange::new(NSNotFound as NSUInteger, 0)
            } else {
                NSRange::new(0, length)
            }
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            with(|app| app.has_preedit()).unwrap_or(false)
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(
            &self,
            _range: NSRange,
            _actual: *mut NSRange,
        ) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect(&self, _range: NSRange, _actual: *mut NSRange) -> NSRect {
            let local = with(|app| app.cursor_rect()).unwrap_or_default();
            let in_window = self.convertRect_toView(local, None);
            match self.window() {
                Some(window) => window.convertRectToScreen(in_window),
                None => in_window,
            }
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index(&self, _point: NSPoint) -> NSUInteger {
            NSNotFound as NSUInteger
        }
    }
);

impl TerminalView {
    pub(crate) fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ViewIvars::default());
        // SAFETY: NSView's designated initializer with a valid frame.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn local(&self, event: &NSEvent) -> NSPoint {
        self.convertPoint_fromView(event.locationInWindow(), None)
    }

    fn button(&self, event: &NSEvent, button: MouseButton, action: MouseAction) {
        let point = self.local(event);
        act(|app| app.mouse(event, point, button, action));
    }
}

fn plain(string: &AnyObject) -> String {
    if let Some(text) = string.downcast_ref::<NSString>() {
        return text.to_string();
    }
    if let Some(text) = string.downcast_ref::<NSAttributedString>() {
        return text.string().to_string();
    }
    String::new()
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and AppDelegate does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    pub(crate) struct AppDelegate;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: each method matches the protocol's Objective-C signature.
    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            app::start(self.mtm());
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window(&self, _sender: &NSApplication) -> bool {
            true
        }
    }

    // SAFETY: each method matches the protocol's Objective-C signature.
    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn did_become_key(&self, _notification: &NSNotification) {
            act(|app| app.focus(true));
        }

        #[unsafe(method(windowDidResignKey:))]
        fn did_resign_key(&self, _notification: &NSNotification) {
            act(|app| app.focus(false));
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn did_change_backing(&self, _notification: &NSNotification) {
            act(app::App::rescale);
        }

        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _notification: &NSNotification) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl AppDelegate {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        Self::shared(mtm)
    }

    pub(crate) fn shared(mtm: MainThreadMarker) -> Retained<Self> {
        DELEGATE.with(|cell| {
            cell.get_or_init(|| {
                let this = Self::alloc(mtm).set_ivars(());
                // SAFETY: NSObject's init has no preconditions.
                unsafe { msg_send![super(this), init] }
            })
            .clone()
        })
    }
}
