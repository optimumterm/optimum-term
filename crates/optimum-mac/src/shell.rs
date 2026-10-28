use std::env;
use std::ffi::{CStr, CString, OsString};
use std::mem::MaybeUninit;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

const FALLBACK_SHELL: &str = "/bin/zsh";
const PASSWD_BUFFER: usize = 16 * 1024;

pub(crate) const ENVIRONMENT: [(&str, &str); 4] = [
    ("TERM", "xterm-256color"),
    ("COLORTERM", "truecolor"),
    ("TERM_PROGRAM", "Optimum"),
    ("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION")),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginShell {
    pub program: PathBuf,
    pub home: Option<PathBuf>,
}

impl LoginShell {
    pub fn argv0(&self) -> OsString {
        let name = self
            .program
            .file_name()
            .unwrap_or_else(|| self.program.as_os_str());
        let mut argv0 = OsString::from("-");
        argv0.push(name);
        argv0
    }
}

pub fn login_shell() -> LoginShell {
    let (shell, dir) = passwd_entry().unwrap_or_default();
    let program = [shell, env::var_os("SHELL").map(PathBuf::from)]
        .into_iter()
        .flatten()
        .find(|path| path.is_absolute() && is_executable(path))
        .unwrap_or_else(|| PathBuf::from(FALLBACK_SHELL));
    let home = dir
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .filter(|path| path.is_dir());
    LoginShell { program, home }
}

fn passwd_entry() -> Option<(Option<PathBuf>, Option<PathBuf>)> {
    let mut entry = MaybeUninit::<libc::passwd>::uninit();
    let mut buffer = vec![0 as libc::c_char; PASSWD_BUFFER];
    let mut found = ptr::null_mut();
    // SAFETY: `entry`, `buffer` and `found` are valid for the call; `buffer.len()` is its real size.
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() {
        return None;
    }
    // SAFETY: getpwuid_r succeeded, so `entry` is initialized and its strings point into `buffer`.
    let entry = unsafe { entry.assume_init() };
    Some((owned_path(entry.pw_shell), owned_path(entry.pw_dir)))
}

fn owned_path(text: *const libc::c_char) -> Option<PathBuf> {
    if text.is_null() {
        return None;
    }
    // SAFETY: `text` is a non-null NUL-terminated string inside the live passwd buffer.
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes();
    (!bytes.is_empty()).then(|| PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

fn is_executable(path: &Path) -> bool {
    let Ok(text) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `text` is a valid NUL-terminated path.
    unsafe { libc::access(text.as_ptr(), libc::X_OK) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_shell_is_an_executable_absolute_path() {
        let shell = login_shell();
        assert!(shell.program.is_absolute(), "{shell:?}");
        assert!(is_executable(&shell.program), "{shell:?}");
    }

    #[test]
    fn argv0_marks_a_login_shell() {
        let shell = LoginShell {
            program: PathBuf::from("/bin/zsh"),
            home: None,
        };
        assert_eq!(shell.argv0(), OsString::from("-zsh"));
    }
}
