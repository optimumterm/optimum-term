use std::env;
use std::ffi::c_void;
use std::mem;
use std::path::Path;
use std::sync::OnceLock;

use windows::Win32::Foundation::{HANDLE, HMODULE};
use windows::Win32::System::Console::{COORD, HPCON};
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{HRESULT, HSTRING, PCSTR, s, w};

type CreateFn = unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type ResizeFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type CloseFn = unsafe extern "system" fn(HPCON);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConptySource {
    Bundled,
    Inbox,
}

#[derive(Clone, Copy)]
pub(crate) struct Api {
    pub source: ConptySource,
    pub create: CreateFn,
    pub resize: ResizeFn,
    pub close: CloseFn,
}

impl Api {
    pub fn get() -> &'static Api {
        static API: OnceLock<Api> = OnceLock::new();
        API.get_or_init(|| Self::bundled().unwrap_or_else(Self::inbox))
    }

    fn bundled() -> Option<Api> {
        let exe = env::current_exe().ok()?;
        let dir = exe.parent()?;
        Self::bundled_in(dir)
    }

    pub fn bundled_in(dir: &Path) -> Option<Api> {
        let dll = dir.join("conpty.dll");
        if !dll.is_file() || !dir.join("OpenConsole.exe").is_file() {
            return None;
        }
        // SAFETY: `dll` is an absolute path to a regular file; the search flags restrict its dependencies to its own directory and System32.
        let module = unsafe {
            LoadLibraryExW(
                &HSTRING::from(dll.as_os_str()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .ok()?;
        Self::resolve(module, ConptySource::Bundled)
    }

    pub fn inbox() -> Api {
        // SAFETY: kernel32 is mapped into every Win32 process; the handle is not freed.
        let module = unsafe { GetModuleHandleW(w!("kernel32.dll")) }.expect("kernel32 not loaded");
        Self::resolve(module, ConptySource::Inbox)
            .expect("this Windows version has no ConPTY (needs Windows 10 1809 or later)")
    }

    fn resolve(module: HMODULE, source: ConptySource) -> Option<Api> {
        let create = symbol(module, s!("CreatePseudoConsole"))?;
        let resize = symbol(module, s!("ResizePseudoConsole"))?;
        let close = symbol(module, s!("ClosePseudoConsole"))?;
        // SAFETY: these exports have exactly the documented ConPTY signatures of CreateFn, ResizeFn and CloseFn.
        unsafe {
            Some(Api {
                source,
                create: mem::transmute::<*const c_void, CreateFn>(create),
                resize: mem::transmute::<*const c_void, ResizeFn>(resize),
                close: mem::transmute::<*const c_void, CloseFn>(close),
            })
        }
    }
}

fn symbol(module: HMODULE, name: PCSTR) -> Option<*const c_void> {
    // SAFETY: `module` is a loaded module that is never unloaded; `name` is a NUL-terminated literal.
    unsafe { GetProcAddress(module, name) }.map(|f| f as *const c_void)
}
