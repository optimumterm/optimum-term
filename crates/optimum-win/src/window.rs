use std::ffi::c_void;

use optimum_core::WindowConfig;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GWL_STYLE, GetWindowLongPtrW, GetWindowPlacement, HICON, HWND_TOP, ICON_BIG,
    ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSMICON, SWP_FRAMECHANGED,
    SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowLongPtrW,
    SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT, WM_SETICON, WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, NCCALCSIZE_PARAMS, SM_CXFRAME, SM_CXPADDEDBORDER, SM_CYFRAME,
};
use windows::core::{BOOL, PCWSTR, s, w};

pub(crate) const INITIAL_WIDTH: u32 = 1000;
pub(crate) const INITIAL_HEIGHT: u32 = 640;
pub(crate) const MIN_WIDTH: u32 = 240;
pub(crate) const MIN_HEIGHT: u32 = 160;

pub(crate) fn frame_thickness(dpi: u32) -> (i32, i32) {
    // SAFETY: GetSystemMetricsForDpi has no preconditions.
    unsafe {
        let padded = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        (
            GetSystemMetricsForDpi(SM_CXFRAME, dpi) + padded,
            GetSystemMetricsForDpi(SM_CYFRAME, dpi) + padded,
        )
    }
}

pub(crate) fn enable_frameless(hwnd: HWND) {
    let margins = MARGINS::default();
    let backdrop = DWMSBT_NONE;
    set_corners(hwnd, DWMWCP_ROUND);
    // SAFETY: `hwnd` is a live top-level window; each attribute value points to a value of the size passed. Older Windows versions reject the backdrop attribute, which is ignored.
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&raw const backdrop).cast::<c_void>(),
            size_of_val(&backdrop) as u32,
        );
    }
}

fn set_corners(hwnd: HWND, corners: DWM_WINDOW_CORNER_PREFERENCE) {
    // SAFETY: `hwnd` is a live top-level window; the value points to a corner preference of the size passed.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corners).cast::<c_void>(),
            size_of_val(&corners) as u32,
        )
    };
}

pub(crate) fn enter_fullscreen(hwnd: HWND) -> Option<WINDOWPLACEMENT> {
    let mut placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `hwnd` is a live top-level window; both out-structs carry their size.
    unsafe {
        GetWindowPlacement(hwnd, &mut placement).ok()?;
        let screen = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(screen, &mut monitor).as_bool() {
            return None;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = (style & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        set_corners(hwnd, DWMWCP_DONOTROUND);
        let area = monitor.rcMonitor;
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            area.left,
            area.top,
            area.right - area.left,
            area.bottom - area.top,
            SWP_FRAMECHANGED | SWP_NOOWNERZORDER,
        );
    }
    Some(placement)
}

pub(crate) fn leave_fullscreen(hwnd: HWND, placement: &WINDOWPLACEMENT) {
    // SAFETY: `hwnd` is a live top-level window; `placement` was read from it by `enter_fullscreen`.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = (style & !WS_POPUP.0) | WS_OVERLAPPEDWINDOW.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        set_corners(hwnd, DWMWCP_ROUND);
        let _ = SetWindowPlacement(hwnd, placement);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        );
    }
}

const ICON_ID: u16 = 1;

const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: i32 = 0;
const ACCENT_ENABLE_BLURBEHIND: i32 = 3;

#[repr(C)]
struct AccentPolicy {
    state: i32,
    flags: i32,
    gradient: u32,
    animation: i32,
}

#[repr(C)]
struct CompositionAttribute {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

type SetWindowCompositionAttribute =
    unsafe extern "system" fn(HWND, *mut CompositionAttribute) -> BOOL;

pub(crate) fn set_translucency(hwnd: HWND, config: WindowConfig) {
    let blur = config.opacity < 1.0 && config.blur;
    let mut policy = AccentPolicy {
        state: if blur {
            ACCENT_ENABLE_BLURBEHIND
        } else {
            ACCENT_DISABLED
        },
        flags: 0,
        gradient: 0,
        animation: 0,
    };
    let mut data = CompositionAttribute {
        attribute: WCA_ACCENT_POLICY,
        data: (&raw mut policy).cast::<c_void>(),
        size: size_of::<AccentPolicy>(),
    };
    // SAFETY: user32 is always loaded in a GUI process; the export, when present, has the documented-by-convention signature `BOOL(HWND, WINDOWCOMPOSITIONATTRIBDATA*)`, and `data` points to a live accent policy of the size given.
    unsafe {
        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else {
            return;
        };
        let Some(export) = GetProcAddress(user32, s!("SetWindowCompositionAttribute")) else {
            return;
        };
        let set: SetWindowCompositionAttribute = std::mem::transmute(export);
        let _ = set(hwnd, &mut data);
    }
}

pub(crate) fn set_icons(hwnd: HWND, dpi: u32) {
    for (kind, metric) in [(ICON_BIG, SM_CXICON), (ICON_SMALL, SM_CXSMICON)] {
        // SAFETY: the icon is loaded from this executable's resources by integer id; WM_SETICON hands it to the window, and the icon it replaces is destroyed.
        unsafe {
            let Ok(module) = GetModuleHandleW(None) else {
                return;
            };
            let size = GetSystemMetricsForDpi(metric, dpi);
            let Ok(icon) = LoadImageW(
                Some(module.into()),
                PCWSTR(usize::from(ICON_ID) as *const u16),
                IMAGE_ICON,
                size,
                size,
                LR_DEFAULTCOLOR,
            ) else {
                continue;
            };
            let old = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(kind as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
            if old.0 != 0 {
                let _ = DestroyIcon(HICON(old.0 as *mut _));
            }
        }
    }
}

pub(crate) fn calc_size(lparam: LPARAM, maximized: bool, dpi: u32) {
    if !maximized {
        return;
    }
    let (fx, fy) = frame_thickness(dpi);
    // SAFETY: for WM_NCCALCSIZE with wParam TRUE, lParam points to a writable NCCALCSIZE_PARAMS.
    let params = unsafe { &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS) };
    let client = &mut params.rgrc[0];
    client.left += fx;
    client.right -= fx;
    client.top += fy;
    client.bottom -= fy;
}

pub(crate) fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xffff) as u16 as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
    (x, y)
}

pub(crate) fn hit_test(
    (x, y): (i32, i32),
    window: RECT,
    border: i32,
    drag_strip: i32,
    maximized: bool,
) -> u32 {
    if !maximized {
        let left = x < window.left + border;
        let right = x >= window.right - border;
        let top = y < window.top + border;
        let bottom = y >= window.bottom - border;
        let hit = match (top, bottom, left, right) {
            (true, _, true, _) => Some(HTTOPLEFT),
            (true, _, _, true) => Some(HTTOPRIGHT),
            (_, true, true, _) => Some(HTBOTTOMLEFT),
            (_, true, _, true) => Some(HTBOTTOMRIGHT),
            (true, ..) => Some(HTTOP),
            (_, true, ..) => Some(HTBOTTOM),
            (_, _, true, _) => Some(HTLEFT),
            (_, _, _, true) => Some(HTRIGHT),
            _ => None,
        };
        if let Some(hit) = hit {
            return hit;
        }
    }
    if y < window.top + drag_strip {
        return HTCAPTION;
    }
    HTCLIENT
}

pub(crate) fn rect_size(rect: &RECT) -> (u32, u32) {
    (
        (rect.right - rect.left).max(0) as u32,
        (rect.bottom - rect.top).max(0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: RECT = RECT {
        left: 100,
        top: 100,
        right: 900,
        bottom: 700,
    };

    fn hit(x: i32, y: i32, maximized: bool) -> u32 {
        hit_test((x, y), WINDOW, 8, 12, maximized)
    }

    #[test]
    fn corners_and_edges_resize() {
        assert_eq!(hit(101, 101, false), HTTOPLEFT);
        assert_eq!(hit(899, 101, false), HTTOPRIGHT);
        assert_eq!(hit(101, 699, false), HTBOTTOMLEFT);
        assert_eq!(hit(899, 699, false), HTBOTTOMRIGHT);
        assert_eq!(hit(500, 101, false), HTTOP);
        assert_eq!(hit(500, 699, false), HTBOTTOM);
        assert_eq!(hit(101, 400, false), HTLEFT);
        assert_eq!(hit(899, 400, false), HTRIGHT);
    }

    #[test]
    fn top_padding_drags_and_the_rest_is_client() {
        assert_eq!(hit(500, 110, false), HTCAPTION);
        assert_eq!(hit(500, 112, false), HTCLIENT);
        assert_eq!(hit(500, 400, false), HTCLIENT);
    }

    #[test]
    fn maximized_window_has_no_resize_borders() {
        assert_eq!(hit(101, 101, true), HTCAPTION);
        assert_eq!(hit(101, 400, true), HTCLIENT);
    }

    #[test]
    fn lparam_coordinates_are_signed() {
        let lparam = LPARAM(((-5i16 as u16 as isize) << 16) | (-20i16 as u16 as isize));
        assert_eq!(point_from_lparam(lparam), (-20, -5));
    }
}
