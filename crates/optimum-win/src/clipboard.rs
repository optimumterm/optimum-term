use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

const ATTEMPTS: u32 = 10;

fn open(hwnd: HWND) -> bool {
    for _ in 0..ATTEMPTS {
        // SAFETY: OpenClipboard has no memory preconditions; it fails while another process holds the clipboard.
        if unsafe { OpenClipboard(Some(hwnd)) }.is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

pub(crate) fn set(hwnd: HWND, text: &str) {
    let units: Vec<u16> = text
        .replace('\n', "\r\n")
        .encode_utf16()
        .chain([0])
        .collect();
    if !open(hwnd) {
        return;
    }
    // SAFETY: the clipboard is open by this thread; the global block holds exactly `units` and is handed to the system on success, freed by us otherwise.
    unsafe {
        let _ = EmptyClipboard();
        if let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) {
            let target = GlobalLock(memory).cast::<u16>();
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
            } else {
                target.copy_from_nonoverlapping(units.as_ptr(), units.len());
                let _ = GlobalUnlock(memory);
                if SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))).is_err() {
                    let _ = GlobalFree(Some(memory));
                }
            }
        }
        let _ = CloseClipboard();
    }
}

pub(crate) fn get(hwnd: HWND) -> Option<String> {
    if !open(hwnd) {
        return None;
    }
    // SAFETY: the clipboard is open by this thread; the locked block is a NUL-terminated UTF-16 string owned by the system, read before unlocking and closing.
    let text = unsafe {
        let text = GetClipboardData(u32::from(CF_UNICODETEXT.0))
            .ok()
            .and_then(|handle| {
                let memory = HGLOBAL(handle.0);
                let source = GlobalLock(memory).cast::<u16>();
                if source.is_null() {
                    return None;
                }
                let mut len = 0;
                while *source.add(len) != 0 {
                    len += 1;
                }
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(source, len));
                let _ = GlobalUnlock(memory);
                Some(text)
            });
        let _ = CloseClipboard();
        text
    };
    text.map(|t| t.replace("\r\n", "\n"))
}
