use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::slice;

use ghostty_vt_sys as sys;

use crate::{Error, Terminal};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub col: u16,
    pub row: u16,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionGeometry {
    pub columns: u32,
    pub cell_width: u32,
    pub padding_left: u32,
    pub screen_height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickTiming {
    pub time_ns: u64,
    pub interval_ns: u64,
    pub distance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjust {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    LineStart,
    LineEnd,
}

impl Adjust {
    fn raw(self) -> sys::GhosttySelectionAdjust {
        match self {
            Adjust::Left => sys::GHOSTTY_SELECTION_ADJUST_LEFT,
            Adjust::Right => sys::GHOSTTY_SELECTION_ADJUST_RIGHT,
            Adjust::Up => sys::GHOSTTY_SELECTION_ADJUST_UP,
            Adjust::Down => sys::GHOSTTY_SELECTION_ADJUST_DOWN,
            Adjust::Home => sys::GHOSTTY_SELECTION_ADJUST_HOME,
            Adjust::End => sys::GHOSTTY_SELECTION_ADJUST_END,
            Adjust::PageUp => sys::GHOSTTY_SELECTION_ADJUST_PAGE_UP,
            Adjust::PageDown => sys::GHOSTTY_SELECTION_ADJUST_PAGE_DOWN,
            Adjust::LineStart => sys::GHOSTTY_SELECTION_ADJUST_BEGINNING_OF_LINE,
            Adjust::LineEnd => sys::GHOSTTY_SELECTION_ADJUST_END_OF_LINE,
        }
    }
}

pub(crate) struct SelectionGesture {
    raw: NonNull<sys::GhosttySelectionGestureImpl>,
    press: NonNull<sys::GhosttySelectionGestureEventImpl>,
    drag: NonNull<sys::GhosttySelectionGestureEventImpl>,
    release: NonNull<sys::GhosttySelectionGestureEventImpl>,
}

impl SelectionGesture {
    pub(crate) fn new() -> Result<Self, Error> {
        let mut raw: sys::GhosttySelectionGesture = ptr::null_mut();
        // SAFETY: a null allocator selects the library default; `raw` is a valid out-pointer.
        Error::check(unsafe { sys::ghostty_selection_gesture_new(ptr::null(), &mut raw) })?;
        let raw = NonNull::new(raw).ok_or(Error::InvalidValue)?;
        let mut events = [ptr::null_mut(); 3];
        let kinds = [
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_PRESS,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_DRAG,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_RELEASE,
        ];
        for (event, kind) in events.iter_mut().zip(kinds) {
            // SAFETY: a null allocator selects the library default; `event` is a valid out-pointer.
            let created = Error::check(unsafe {
                sys::ghostty_selection_gesture_event_new(ptr::null(), event, kind)
            });
            if let Err(e) = created {
                // SAFETY: every handle freed here was created above and is not used again; null handles are no-ops, and the gesture has never touched a terminal.
                unsafe {
                    events
                        .iter()
                        .for_each(|e| sys::ghostty_selection_gesture_event_free(*e));
                    sys::ghostty_selection_gesture_free(raw.as_ptr(), ptr::null_mut());
                }
                return Err(e);
            }
        }
        let [press, drag, release] = events.map(NonNull::new);
        Ok(Self {
            raw,
            press: press.ok_or(Error::InvalidValue)?,
            drag: drag.ok_or(Error::InvalidValue)?,
            release: release.ok_or(Error::InvalidValue)?,
        })
    }

    pub(crate) unsafe fn destroy(&self, terminal: sys::GhosttyTerminal) {
        // SAFETY: each handle was created in `new` and is freed exactly once; the caller passes the live terminal this gesture was used with.
        unsafe {
            sys::ghostty_selection_gesture_event_free(self.press.as_ptr());
            sys::ghostty_selection_gesture_event_free(self.drag.as_ptr());
            sys::ghostty_selection_gesture_event_free(self.release.as_ptr());
            sys::ghostty_selection_gesture_free(self.raw.as_ptr(), terminal);
        }
    }

    fn apply(
        &self,
        terminal: sys::GhosttyTerminal,
        event: sys::GhosttySelectionGestureEvent,
    ) -> Result<Option<sys::GhosttySelection>, Error> {
        let mut selection = sys::GhosttySelection {
            size: size_of::<sys::GhosttySelection>(),
            ..Default::default()
        };
        // SAFETY: the gesture, the event and the terminal are live; `selection` is a sized out-struct.
        match Error::check(unsafe {
            sys::ghostty_selection_gesture_event(self.raw.as_ptr(), terminal, event, &mut selection)
        }) {
            Ok(()) => Ok(Some(selection)),
            Err(Error::NoValue) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn set<T>(
    event: sys::GhosttySelectionGestureEvent,
    option: sys::GhosttySelectionGestureEventOption,
    value: &T,
) -> Result<(), Error> {
    // SAFETY: `event` is live and `value` has the type `option` expects; the library copies it.
    Error::check(unsafe {
        sys::ghostty_selection_gesture_event_set(
            event,
            option,
            (value as *const T).cast::<c_void>(),
        )
    })
}

impl Terminal {
    pub fn select_press(&mut self, pointer: Pointer, timing: ClickTiming) -> Result<(), Error> {
        let event = self.gesture.press.as_ptr();
        let reference = self.viewport_ref(pointer.col, pointer.row)?;
        let position = sys::GhosttySurfacePosition {
            x: pointer.x,
            y: pointer.y,
        };
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF,
            &reference,
        )?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION,
            &position,
        )?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_TIME_NS,
            &timing.time_ns,
        )?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REPEAT_INTERVAL_NS,
            &timing.interval_ns,
        )?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REPEAT_DISTANCE,
            &timing.distance,
        )?;
        let selection = self.gesture.apply(self.raw.as_ptr(), event)?;
        self.set_selection(selection.as_ref())
    }

    pub fn select_drag(
        &mut self,
        pointer: Pointer,
        geometry: SelectionGeometry,
    ) -> Result<(), Error> {
        let event = self.gesture.drag.as_ptr();
        let reference = self.viewport_ref(pointer.col, pointer.row)?;
        let position = sys::GhosttySurfacePosition {
            x: pointer.x,
            y: pointer.y,
        };
        let geometry = sys::GhosttySelectionGestureGeometry {
            columns: geometry.columns.max(1),
            cell_width: geometry.cell_width.max(1),
            padding_left: geometry.padding_left,
            screen_height: geometry.screen_height.max(1),
        };
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF,
            &reference,
        )?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION,
            &position,
        )?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_GEOMETRY,
            &geometry,
        )?;
        if let Some(selection) = self.gesture.apply(self.raw.as_ptr(), event)? {
            self.set_selection(Some(&selection))?;
        }
        Ok(())
    }

    pub fn select_release(&mut self, pointer: Pointer) -> Result<(), Error> {
        let event = self.gesture.release.as_ptr();
        let reference = self.viewport_ref(pointer.col, pointer.row)?;
        set(
            event,
            sys::GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF,
            &reference,
        )?;
        self.gesture.apply(self.raw.as_ptr(), event)?;
        Ok(())
    }

    pub fn has_selection(&self) -> bool {
        self.selection_text().is_some_and(|text| !text.is_empty())
    }

    fn viewport_ref(&self, col: u16, row: u16) -> Result<sys::GhosttyGridRef, Error> {
        let mut point = sys::GhosttyPoint {
            tag: sys::GHOSTTY_POINT_TAG_VIEWPORT,
            ..Default::default()
        };
        point.value.coordinate = sys::GhosttyPointCoordinate {
            x: col,
            y: u32::from(row),
        };
        let mut reference = sys::GhosttyGridRef {
            size: size_of::<sys::GhosttyGridRef>(),
            ..Default::default()
        };
        // SAFETY: `self.raw` is live; `reference` is a sized out-struct.
        Error::check(unsafe {
            sys::ghostty_terminal_grid_ref(self.raw.as_ptr(), point, &mut reference)
        })?;
        Ok(reference)
    }

    fn set_selection(&mut self, selection: Option<&sys::GhosttySelection>) -> Result<(), Error> {
        let value = selection.map_or(ptr::null(), |s| (s as *const sys::GhosttySelection).cast());
        // SAFETY: `self.raw` is live; the terminal copies the selection immediately, and null clears it.
        Error::check(unsafe {
            sys::ghostty_terminal_set(
                self.raw.as_ptr(),
                sys::GHOSTTY_TERMINAL_OPT_SELECTION,
                value,
            )
        })
    }

    pub fn select_all(&mut self) -> Result<(), Error> {
        let mut selection = sys::GhosttySelection {
            size: size_of::<sys::GhosttySelection>(),
            ..Default::default()
        };
        // SAFETY: `self.raw` is live; `selection` is a sized out-struct.
        Error::check(unsafe {
            sys::ghostty_terminal_select_all(self.raw.as_ptr(), &mut selection)
        })?;
        self.set_selection(Some(&selection))
    }

    pub fn extend_selection(&mut self, pointer: Pointer) -> Result<bool, Error> {
        let Some(mut selection) = self.current_selection()? else {
            return Ok(false);
        };
        selection.end = self.viewport_ref(pointer.col, pointer.row)?;
        self.set_selection(Some(&selection))?;
        Ok(true)
    }

    fn current_selection(&self) -> Result<Option<sys::GhosttySelection>, Error> {
        let mut selection = sys::GhosttySelection {
            size: size_of::<sys::GhosttySelection>(),
            ..Default::default()
        };
        match self.get(sys::GHOSTTY_TERMINAL_DATA_SELECTION, &mut selection) {
            Ok(()) => Ok(Some(selection)),
            Err(Error::NoValue) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn adjust_selection(&mut self, adjust: Adjust) -> Result<bool, Error> {
        let Some(mut selection) = self.current_selection()? else {
            return Ok(false);
        };
        // SAFETY: `self.raw` is live; `selection` was just read from it and no mutating call happened since.
        Error::check(unsafe {
            sys::ghostty_terminal_selection_adjust(self.raw.as_ptr(), &mut selection, adjust.raw())
        })?;
        self.set_selection(Some(&selection))?;
        Ok(true)
    }

    pub fn clear_selection(&mut self) {
        let _ = self.set_selection(None);
    }

    pub fn selection_text(&self) -> Option<String> {
        let options = sys::GhosttyTerminalSelectionFormatOptions {
            size: size_of::<sys::GhosttyTerminalSelectionFormatOptions>(),
            emit: sys::GHOSTTY_FORMATTER_FORMAT_PLAIN,
            unwrap: true,
            trim: true,
            selection: ptr::null(),
        };
        let mut data: *mut u8 = ptr::null_mut();
        let mut len = 0;
        // SAFETY: `self.raw` is live; on success the library allocates `len` bytes at `data` with the default allocator.
        Error::check(unsafe {
            sys::ghostty_terminal_selection_format_alloc(
                self.raw.as_ptr(),
                ptr::null(),
                options,
                &mut data,
                &mut len,
            )
        })
        .ok()?;
        if data.is_null() {
            return Some(String::new());
        }
        // SAFETY: `data` holds `len` bytes allocated by the library, freed right after copying with the same (default) allocator.
        let text = unsafe {
            let text = String::from_utf8_lossy(slice::from_raw_parts(data, len)).into_owned();
            sys::ghostty_free(ptr::null(), data, len);
            text
        };
        Some(text)
    }
}
