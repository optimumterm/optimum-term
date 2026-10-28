use std::fs;
use std::io;
use std::path::PathBuf;
use std::thread;

use optimum_core::{Config, ConfigError, DEFAULT_TOML};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FindCloseChangeNotification,
    FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};

const NO_ASSOCIATION: isize = 32;

pub(crate) fn directory() -> Option<PathBuf> {
    // SAFETY: the returned string is copied and then freed with CoTaskMemFree, as the API requires.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string();
        CoTaskMemFree(Some(path.0 as *const _));
        Some(PathBuf::from(text.ok()?).join("Optimum"))
    }
}

pub(crate) fn path() -> Option<PathBuf> {
    directory().map(|dir| dir.join("config.toml"))
}

pub(crate) fn load() -> Result<Config, ConfigError> {
    let Some(path) = path() else {
        return Ok(Config::default());
    };
    match fs::read_to_string(&path) {
        Ok(text) => Config::parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(ConfigError {
            line: None,
            message: e.to_string(),
        }),
    }
}

pub(crate) fn ensure() -> io::Result<PathBuf> {
    let path = path().ok_or_else(|| io::Error::other("no AppData folder"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if !path.exists() {
        fs::write(&path, DEFAULT_TOML)?;
    }
    Ok(path)
}

pub(crate) fn open() {
    let Ok(path) = ensure() else {
        return;
    };
    let file = HSTRING::from(path.as_os_str());
    // SAFETY: plain shell calls with valid, NUL-terminated strings; the result is only compared.
    unsafe {
        let result = ShellExecuteW(None, w!("open"), &file, None, None, SW_SHOWNORMAL);
        if result.0 as isize <= NO_ASSOCIATION {
            ShellExecuteW(
                None,
                w!("open"),
                w!("notepad.exe"),
                &file,
                None,
                SW_SHOWNORMAL,
            );
        }
    }
}

pub(crate) fn watch(changed: impl Fn() + Send + 'static) -> bool {
    let Some(dir) = directory().filter(|d| d.is_dir()) else {
        return false;
    };
    let dir = HSTRING::from(dir.as_os_str());
    // SAFETY: `dir` is a valid path string; the handle is owned by the watcher thread below.
    let Ok(handle) = (unsafe {
        FindFirstChangeNotificationW(
            &dir,
            false,
            FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
        )
    }) else {
        return false;
    };
    let handle = handle.0 as usize;
    thread::spawn(move || {
        let handle = HANDLE(handle as *mut _);
        loop {
            // SAFETY: `handle` is the live change-notification handle owned by this thread.
            if unsafe { WaitForSingleObject(handle, INFINITE) } != WAIT_OBJECT_0 {
                break;
            }
            changed();
            // SAFETY: re-arms the same live handle.
            if unsafe { FindNextChangeNotification(handle) }.is_err() {
                break;
            }
        }
        // SAFETY: the handle is closed once, by its owning thread.
        let _ = unsafe { FindCloseChangeNotification(handle) };
    });
    true
}
