use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::ptr;
use std::thread::{self, JoinHandle};

use optimum_core::{GridSize, Pty, PtyEvent};

use crate::shell::{ENVIRONMENT, LoginShell};

const READ_CHUNK: usize = 64 * 1024;
const FALLBACK_LANG: &str = "en_US.UTF-8";

pub struct UnixPty {
    master: File,
}

impl UnixPty {
    pub fn spawn_login_shell(
        shell: &LoginShell,
        size: GridSize,
        on_event: impl FnMut(PtyEvent) + Send + 'static,
    ) -> io::Result<Self> {
        let mut command = Command::new(&shell.program);
        command.arg0(shell.argv0());
        if let Some(home) = &shell.home {
            command.current_dir(home);
        }
        if env::var_os("LANG").is_none() {
            command.env("LANG", FALLBACK_LANG);
        }
        Self::spawn(command, size, on_event)
    }

    pub(crate) fn spawn(
        mut command: Command,
        size: GridSize,
        on_event: impl FnMut(PtyEvent) + Send + 'static,
    ) -> io::Result<Self> {
        let (master, slave) = open(size)?;
        command
            .envs(ENVIRONMENT)
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        // SAFETY: the hook only calls setsid and ioctl, which are async-signal-safe and touch the child alone.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        drop(command);

        let output = File::from(master.try_clone()?);
        let reader = thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || read_output(output, on_event))?;
        thread::Builder::new()
            .name("pty-waiter".into())
            .spawn(move || wait_for_exit(child, reader))?;

        Ok(Self {
            master: File::from(master),
        })
    }
}

impl Pty for UnixPty {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.master.write_all(bytes)
    }

    fn resize(&mut self, size: GridSize) -> io::Result<()> {
        let winsize = winsize(size);
        // SAFETY: `self.master` is an open pty master and `winsize` is a valid struct for TIOCSWINSZ.
        let status = unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &winsize) };
        if status == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn winsize(size: GridSize) -> libc::winsize {
    libc::winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

fn open(size: GridSize) -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master = -1;
    let mut slave = -1;
    let winsize = winsize(size);
    // SAFETY: `master` and `slave` are valid out-pointers; no name buffer or termios is requested.
    let status = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            &winsize as *const libc::winsize as *mut libc::winsize,
        )
    };
    if status == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty succeeded, so both descriptors are open and owned by nobody else.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    close_on_exec(&master)?;
    close_on_exec(&slave)?;
    Ok((master, slave))
}

fn close_on_exec(fd: &OwnedFd) -> io::Result<()> {
    // SAFETY: `fd` is an open descriptor; FD_CLOEXEC only affects it.
    let status = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    if status == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn read_output<F: FnMut(PtyEvent)>(mut output: File, mut on_event: F) -> F {
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        match output.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => on_event(PtyEvent::Output(buf[..read].to_vec())),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    on_event
}

fn wait_for_exit<F: FnMut(PtyEvent)>(mut child: Child, reader: JoinHandle<F>) {
    let code = child
        .wait()
        .ok()
        .and_then(|status| status.code())
        .map(|code| code as u32);
    if let Ok(mut on_event) = reader.join() {
        on_event(PtyEvent::Exited(code));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    use super::*;

    const SIZE: GridSize = GridSize { cols: 80, rows: 24 };

    fn spawn(script: &str) -> (UnixPty, Receiver<PtyEvent>) {
        let (tx, rx) = mpsc::channel();
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        let pty = UnixPty::spawn(command, SIZE, move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        (pty, rx)
    }

    fn collect(rx: &Receiver<PtyEvent>, until: &str) -> (String, Option<Option<u32>>) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut output = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left).expect("no output in time") {
                PtyEvent::Output(bytes) => {
                    output.extend(bytes);
                    let text = String::from_utf8_lossy(&output);
                    if !until.is_empty() && text.contains(until) {
                        return (text.into(), None);
                    }
                }
                PtyEvent::Exited(code) => {
                    return (String::from_utf8_lossy(&output).into(), Some(code));
                }
            }
        }
    }

    fn run(script: &str) -> (String, Option<u32>) {
        let (_pty, rx) = spawn(script);
        let (output, code) = collect(&rx, "");
        (output, code.expect("process exited"))
    }

    #[test]
    fn runs_a_process_on_a_terminal() {
        let (output, code) = run("test -t 0 && test -t 1 && echo optimum-tty");
        assert!(output.contains("optimum-tty"), "{output:?}");
        assert_eq!(code, Some(0));
    }

    #[test]
    fn exit_code_is_reported() {
        let (_, code) = run("exit 7");
        assert_eq!(code, Some(7));
    }

    #[test]
    fn child_sees_terminal_environment() {
        let (output, _) = run("echo $TERM $COLORTERM $TERM_PROGRAM");
        assert!(
            output.contains("xterm-256color truecolor Optimum"),
            "{output:?}"
        );
    }

    #[test]
    fn child_sees_initial_size_and_resize() {
        let (mut pty, rx) = spawn("stty size; read line; stty size");
        let (output, _) = collect(&rx, "24 80");
        assert!(output.contains("24 80"), "{output:?}");
        pty.resize(GridSize {
            cols: 100,
            rows: 40,
        })
        .unwrap();
        pty.write(b"\n").unwrap();
        let (output, _) = collect(&rx, "40 100");
        assert!(output.contains("40 100"), "{output:?}");
    }

    #[test]
    fn input_reaches_the_child() {
        let (mut pty, rx) = spawn("read line; echo got-$line");
        pty.write(b"abc\n").unwrap();
        let (output, _) = collect(&rx, "got-abc");
        assert!(output.contains("got-abc"), "{output:?}");
    }
}
