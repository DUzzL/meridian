//! Meridian's own log file.
//!
//! The launcher builds a native mod's registry crates apart from the game's
//! (`log` included), so the mod's `log` macros reach no logger and the mod
//! API has no log service. Meridian installs one on its own `log` that
//! writes to `meridian.log` in its data folder; if the game's logger already
//! answers (a shared `log`), installing fails and nothing changes.

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// The file is started over when it grows past this.
const MAX_BYTES: u64 = 4 << 20;

struct FileLog(Mutex<Option<std::fs::File>>);

impl log::Log for FileLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut file = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = file.as_mut() {
            let now = chrono::Local::now().format("%H:%M:%S");
            let _ = writeln!(f, "[{now}] [{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {
        if let Some(f) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = f.flush();
        }
    }
}

/// Starts the log file in `dir`, once per process.
pub fn start(dir: &Path) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let path = dir.join("meridian.log");
        let _ = std::fs::create_dir_all(dir);
        let append = std::fs::metadata(&path).is_ok_and(|m| m.len() < MAX_BYTES);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&path)
            .ok();
        let logger: &'static FileLog = Box::leak(Box::new(FileLog(Mutex::new(file))));
        if log::set_logger(logger).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    });
}
