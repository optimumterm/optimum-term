use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_foundation::NSString;

use crate::settings;

pub(crate) const BUNDLE_ID: &str = "dev.optimum.Optimum";

pub(crate) struct InstanceLock {
    _file: File,
}

pub(crate) fn acquire() -> Option<InstanceLock> {
    let dir = settings::directory()?;
    fs::create_dir_all(&dir).ok()?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("instance.lock"))
        .ok()?;
    // SAFETY: `file` is an open descriptor; a non-blocking exclusive lock only affects it.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    locked.then_some(InstanceLock { _file: file })
}

pub(crate) fn activate_existing() {
    let me = NSRunningApplication::currentApplication().processIdentifier();
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
        BUNDLE_ID,
    ));
    for app in apps.iter() {
        if app.processIdentifier() != me {
            app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
            return;
        }
    }
}
