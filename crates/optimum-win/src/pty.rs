use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use optimum_core::{GridSize, Pty, PtyEvent};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{COORD, HPCON};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::core::PWSTR;

use crate::conpty::{Api, ConptySource};
use crate::wsl;

const READ_CHUNK: usize = 64 * 1024;

pub struct WinPty {
    input: OwnedHandle,
    console: Arc<Mutex<Option<PseudoConsole>>>,
    source: ConptySource,
}

impl WinPty {
    pub fn spawn_wsl(
        size: GridSize,
        on_event: impl FnMut(PtyEvent) + Send + 'static,
    ) -> io::Result<Self> {
        Self::spawn(
            *Api::get(),
            &wsl::command_line(),
            &wsl::environment(),
            size,
            on_event,
        )
    }

    pub fn conpty_source(&self) -> ConptySource {
        self.source
    }

    pub(crate) fn spawn(
        api: Api,
        command_line: &str,
        environment: &[u16],
        size: GridSize,
        on_event: impl FnMut(PtyEvent) + Send + 'static,
    ) -> io::Result<Self> {
        let (input_read, input) = pipe()?;
        let (output, output_write) = pipe()?;

        let mut hpc = HPCON::default();
        // SAFETY: both pipe ends are valid handles owned by this function; `hpc` is a valid out-pointer.
        unsafe { (api.create)(coord(size), input_read.0, output_write.0, 0, &mut hpc) }
            .ok()
            .map_err(io::Error::other)?;
        let console = PseudoConsole { api, hpc };

        let process = start_process(&console, command_line, environment)?;
        drop(input_read);
        drop(output_write);

        let console = Arc::new(Mutex::new(Some(console)));
        let reader = thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || read_output(output, on_event))?;
        let waiter_console = Arc::clone(&console);
        thread::Builder::new()
            .name("pty-waiter".into())
            .spawn(move || wait_for_exit(process, waiter_console, reader))?;

        Ok(Self {
            input,
            console,
            source: api.source,
        })
    }
}

impl Pty for WinPty {
    fn write(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let mut written = 0;
            // SAFETY: `self.input` is the live write end of the input pipe; `bytes` is a valid slice.
            unsafe { WriteFile(self.input.0, Some(bytes), Some(&mut written), None) }
                .map_err(io::Error::other)?;
            bytes = &bytes[written as usize..];
        }
        Ok(())
    }

    fn resize(&mut self, size: GridSize) -> io::Result<()> {
        let console = self.console.lock().unwrap_or_else(PoisonError::into_inner);
        match console.as_ref() {
            Some(console) => console.resize(size),
            None => Ok(()),
        }
    }
}

impl Drop for WinPty {
    fn drop(&mut self) {
        let console = self
            .console
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        drop(console);
    }
}

struct PseudoConsole {
    api: Api,
    hpc: HPCON,
}

// SAFETY: an HPCON is a process-wide handle; ConPTY functions may be called on it from any thread.
unsafe impl Send for PseudoConsole {}

impl PseudoConsole {
    fn resize(&self, size: GridSize) -> io::Result<()> {
        // SAFETY: `self.hpc` is open until `self` is dropped.
        unsafe { (self.api.resize)(self.hpc, coord(size)) }
            .ok()
            .map_err(io::Error::other)
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        // SAFETY: `self.hpc` came from CreatePseudoConsole and is closed exactly once, here.
        unsafe { (self.api.close)(self.hpc) }
    }
}

struct OwnedHandle(HANDLE);

// SAFETY: kernel object handles are not tied to the thread that opened them.
unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by `self` and closed exactly once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    // SAFETY: `read` and `write` are valid out-pointers; no security attributes, default buffer size.
    unsafe { CreatePipe(&mut read, &mut write, None, 0) }.map_err(io::Error::other)?;
    Ok((OwnedHandle(read), OwnedHandle(write)))
}

fn coord(size: GridSize) -> COORD {
    let clamp = |v: u16| v.clamp(1, i16::MAX as u16) as i16;
    COORD {
        X: clamp(size.cols),
        Y: clamp(size.rows),
    }
}

fn start_process(
    console: &PseudoConsole,
    command_line: &str,
    environment: &[u16],
) -> io::Result<OwnedHandle> {
    let mut list_size = 0;
    // SAFETY: a null list with count 1 only queries the required size; the expected failure is ignored.
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut list_size) };
    let mut list_storage = vec![0usize; list_size.div_ceil(size_of::<usize>())];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(list_storage.as_mut_ptr().cast());
    // SAFETY: `list` points to at least `list_size` writable, pointer-aligned bytes.
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut list_size) }
        .map_err(io::Error::other)?;

    let result = (|| {
        // SAFETY: `list` is initialized; the HPCON value is passed by value as the attribute payload, as ConPTY requires.
        unsafe {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(console.hpc.0 as *const c_void),
                size_of::<HPCON>(),
                None,
                None,
            )
        }
        .map_err(io::Error::other)?;

        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        startup.lpAttributeList = list;

        let mut command: Vec<u16> = command_line.encode_utf16().chain([0]).collect();
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY: `command` is a mutable NUL-terminated buffer, `environment` a double-NUL-terminated UTF-16 block, and `startup` outlives the call.
        unsafe {
            CreateProcessW(
                None,
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                Some(environment.as_ptr().cast()),
                None,
                &startup.StartupInfo,
                &mut info,
            )
        }
        .map_err(io::Error::other)?;
        drop(OwnedHandle(info.hThread));
        Ok(OwnedHandle(info.hProcess))
    })();

    // SAFETY: `list` was successfully initialized above and is deleted exactly once.
    unsafe { DeleteProcThreadAttributeList(list) };
    result
}

fn read_output<F: FnMut(PtyEvent)>(output: OwnedHandle, mut on_event: F) -> F {
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let mut read = 0;
        // SAFETY: `output` is the live read end of the output pipe; `buf` is a valid writable slice.
        let ok = unsafe { ReadFile(output.0, Some(&mut buf), Some(&mut read), None) }.is_ok();
        if !ok || read == 0 {
            break;
        }
        on_event(PtyEvent::Output(buf[..read as usize].to_vec()));
    }
    on_event
}

fn wait_for_exit<F: FnMut(PtyEvent)>(
    process: OwnedHandle,
    console: Arc<Mutex<Option<PseudoConsole>>>,
    reader: JoinHandle<F>,
) {
    // SAFETY: `process` is a live process handle owned by this thread.
    unsafe { WaitForSingleObject(process.0, INFINITE) };
    let mut code = 0;
    // SAFETY: the process has exited; `code` is a valid out-pointer.
    let code = unsafe { GetExitCodeProcess(process.0, &mut code) }
        .ok()
        .map(|()| code);

    let console = console
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    drop(console);

    if let Ok(mut on_event) = reader.join() {
        on_event(PtyEvent::Exited(code));
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;

    const SIZE: GridSize = GridSize { cols: 80, rows: 24 };

    fn run(api: Api, command_line: &str, resize_to: Option<GridSize>) -> (String, Option<u32>) {
        let (tx, rx) = mpsc::channel();
        let mut pty = WinPty::spawn(api, command_line, &wsl::environment(), SIZE, move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        if let Some(size) = resize_to {
            pty.resize(size).unwrap();
        }

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut output = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left).expect("process did not exit in time") {
                PtyEvent::Output(bytes) => output.extend(bytes),
                PtyEvent::Exited(code) => return (String::from_utf8_lossy(&output).into(), code),
            }
        }
    }

    fn cmd(args: &str) -> String {
        format!(
            "{} /c {args}",
            env::var("ComSpec").unwrap_or("cmd.exe".into())
        )
    }

    fn bundled() -> Api {
        let exe = env::current_exe().unwrap();
        Api::bundled_in(exe.parent().unwrap()).expect("bundled ConPTY next to the test binary")
    }

    #[test]
    fn bundled_conpty_runs_a_process() {
        let api = bundled();
        assert_eq!(api.source, ConptySource::Bundled);
        let (output, code) = run(api, &cmd("echo optimum-bundled"), None);
        assert!(output.contains("optimum-bundled"), "{output:?}");
        assert_eq!(code, Some(0));
    }

    #[test]
    fn inbox_conpty_runs_a_process() {
        let api = Api::inbox();
        assert_eq!(api.source, ConptySource::Inbox);
        let (output, code) = run(api, &cmd("echo optimum-inbox"), None);
        assert!(output.contains("optimum-inbox"), "{output:?}");
        assert_eq!(code, Some(0));
    }

    #[test]
    fn exit_code_is_reported() {
        let (_, code) = run(bundled(), &cmd("exit 7"), None);
        assert_eq!(code, Some(7));
    }

    #[test]
    #[ignore = "needs a WSL distribution"]
    fn wsl_sees_terminal_environment() {
        let command = format!(
            "\"{}\" -e sh -c \"echo $TERM $COLORTERM $TERM_PROGRAM\"",
            wsl::exe().display()
        );
        let (output, code) = run(bundled(), &command, None);
        assert!(
            output.contains("xterm-256color truecolor Optimum"),
            "{output:?}"
        );
        assert_eq!(code, Some(0));
    }

    #[test]
    #[ignore = "needs a WSL distribution"]
    fn wsl_sees_resize() {
        let command = format!(
            "\"{}\" -e sh -c \"sleep 2; stty size\"",
            wsl::exe().display()
        );
        let size = GridSize {
            cols: 100,
            rows: 40,
        };
        let (output, code) = run(bundled(), &command, Some(size));
        assert!(output.contains("40 100"), "{output:?}");
        assert_eq!(code, Some(0));
    }
}
