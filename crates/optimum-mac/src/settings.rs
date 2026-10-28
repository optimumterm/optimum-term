use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime};

use optimum_core::{Config, ConfigError, DEFAULT_TOML};

const POLL: Duration = Duration::from_millis(400);

pub(crate) fn directory() -> Option<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join("Library/Application Support/Optimum"))
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
    let path = path().ok_or_else(|| io::Error::other("no home directory"))?;
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
    let _ = Command::new("/usr/bin/open").arg("-t").arg(&path).spawn();
}

pub(crate) fn watch(changed: impl Fn() + Send + 'static) {
    let Some(path) = path() else {
        return;
    };
    let stamp = move || fs::metadata(&path).and_then(|m| m.modified()).ok();
    let _ = thread::Builder::new()
        .name("settings-watch".into())
        .spawn(move || {
            let mut last: Option<SystemTime> = stamp();
            loop {
                thread::sleep(POLL);
                let now = stamp();
                if now != last {
                    last = now;
                    changed();
                }
            }
        });
}
