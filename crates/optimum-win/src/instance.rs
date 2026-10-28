use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, PostMessageW,
    RegisterWindowMessageW,
};
use windows::core::{HSTRING, PCWSTR};

const ID: &str = "01bf42e0-0eca-4d06-8a99-579ea7c5038f";
const FIND_ATTEMPTS: u32 = 40;
const FIND_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) fn class_name() -> HSTRING {
    HSTRING::from(format!("Optimum.Window.{ID}"))
}

pub(crate) fn activate_message() -> u32 {
    let name = HSTRING::from(format!("Optimum.Activate.{ID}"));
    // SAFETY: `name` is a valid NUL-terminated wide string.
    unsafe { RegisterWindowMessageW(&name) }
}

pub(crate) struct InstanceLock(HANDLE);

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // SAFETY: the mutex handle is owned by `self` and closed exactly once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub(crate) fn acquire() -> Option<InstanceLock> {
    let name = HSTRING::from(format!("Local\\Optimum-{ID}"));
    // SAFETY: `name` is a valid wide string; GetLastError is read immediately after the call that set it.
    let (handle, existed) = unsafe {
        let handle = CreateMutexW(None, false, &name).ok()?;
        (handle, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    let lock = InstanceLock(handle);
    (!existed).then_some(lock)
}

pub(crate) fn activate_existing() {
    let class = class_name();
    let message = activate_message();
    for _ in 0..FIND_ATTEMPTS {
        // SAFETY: `class` is a valid wide string; a null window name matches any title.
        if let Ok(hwnd) = unsafe { FindWindowW(&class, PCWSTR::null()) } {
            let mut pid = 0;
            // SAFETY: `hwnd` was just returned by FindWindowW; `pid` is a valid out-pointer.
            unsafe {
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
            }
            return;
        }
        thread::sleep(FIND_INTERVAL);
    }
}
