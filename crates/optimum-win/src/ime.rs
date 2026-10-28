use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_EXCLUDE, CFS_POINT, COMPOSITIONFORM, GCS_COMPSTR, GCS_RESULTSTR,
    IME_COMPOSITION_STRING, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
    ImmSetCandidateWindow, ImmSetCompositionWindow,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Composition {
    pub result: Option<String>,
    pub preedit: Option<String>,
}

pub(crate) fn read(hwnd: HWND, lparam: LPARAM) -> Composition {
    let flags = lparam.0 as u32;
    let has = |kind: IME_COMPOSITION_STRING| flags & kind.0 != 0;
    // SAFETY: `hwnd` is the live window that received WM_IME_COMPOSITION.
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_invalid() {
        return Composition::default();
    }
    let read = |kind: IME_COMPOSITION_STRING| {
        // SAFETY: a null buffer only queries the size in bytes of the requested string.
        let bytes = unsafe { ImmGetCompositionStringW(context, kind, None, 0) };
        if bytes <= 0 {
            return String::new();
        }
        let mut units = vec![0u16; bytes as usize / 2];
        // SAFETY: `units` holds exactly `bytes` bytes.
        let written = unsafe {
            ImmGetCompositionStringW(context, kind, Some(units.as_mut_ptr().cast()), bytes as u32)
        };
        units.truncate(written.max(0) as usize / 2);
        String::from_utf16_lossy(&units)
    };
    let composition = Composition {
        result: has(GCS_RESULTSTR).then(|| read(GCS_RESULTSTR)),
        preedit: has(GCS_COMPSTR).then(|| read(GCS_COMPSTR)),
    };
    // SAFETY: `context` was obtained from ImmGetContext for this window.
    let _ = unsafe { ImmReleaseContext(hwnd, context) };
    composition
}

pub(crate) fn place(hwnd: HWND, cursor: RECT) {
    // SAFETY: `hwnd` is the live window that owns the input context.
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_invalid() {
        return;
    }
    let origin = POINT {
        x: cursor.left,
        y: cursor.top,
    };
    let composition = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: origin,
        rcArea: RECT::default(),
    };
    let candidate = CANDIDATEFORM {
        dwIndex: 0,
        dwStyle: CFS_EXCLUDE,
        ptCurrentPos: origin,
        rcArea: cursor,
    };
    // SAFETY: `context` belongs to `hwnd`; both forms are fully initialised locals.
    unsafe {
        let _ = ImmSetCompositionWindow(context, &composition);
        let _ = ImmSetCandidateWindow(context, &candidate);
        let _ = ImmReleaseContext(hwnd, context);
    }
}
