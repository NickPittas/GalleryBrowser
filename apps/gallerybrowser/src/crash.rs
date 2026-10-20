//! Panic reporting: a panic on any thread is logged with its thread, location and backtrace, and
//! appended to a crash log file once the cache directory is known. The profile keeps `panic = "unwind"`
//! so worker threads' `catch_unwind` guards keep the session alive.
//! Adapted from filmcraft `crates/ui-egui/src/crash.rs`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Where crash logs go; `None` until [`set_log_dir`] (stderr/log output only).
static LOG_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Installs the panic hook (idempotent). The previous hook still runs so panics reach stderr.
pub fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = info.payload();
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_owned());
            let at = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            let thread = std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned();
            let summary = format!("{msg} (at {at}, thread {thread})");
            log::error!("panic: {summary}");
            write_log(&summary);
            prev(info);
        }));
    });
}

/// Directs crash logs to `dir` (created on first write).
pub fn set_log_dir(dir: &Path) {
    *LOG_DIR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(dir.to_path_buf());
}

/// Records a non-panic fatal error (e.g. the window could not be created).
pub fn record(msg: &str) {
    write_log(msg);
}

fn log_path() -> Option<PathBuf> {
    let dir = LOG_DIR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()?;
    Some(dir.join(format!("crash-{}.log", unix_secs() / 86_400)))
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write_log(summary: &str) {
    let Some(path) = log_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let bt = std::backtrace::Backtrace::force_capture();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            f,
            "=== GalleryBrowser {} (unix {})\n{summary}\n{bt}\n",
            env!("CARGO_PKG_VERSION"),
            unix_secs()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_panic_is_written_to_the_crash_log() {
        let dir = tempfile::tempdir().unwrap();
        install();
        set_log_dir(dir.path());
        let r = std::thread::Builder::new()
            .name("gb-test-worker".into())
            .spawn(|| std::panic::panic_any("boom in a worker"))
            .unwrap()
            .join();
        assert!(r.is_err());
        let log = std::fs::read_to_string(log_path().unwrap()).unwrap();
        assert!(
            log.contains("boom in a worker") && log.contains("gb-test-worker"),
            "{log}"
        );
    }
}
