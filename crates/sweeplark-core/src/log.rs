//! A small log file for bug reports: `<data>/logs/sweeplark.log`, rotated at 1 MB.
//! Only app events and errors are written; never file contents or personal data.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);
const MAX: u64 = 1024 * 1024;

/// Start logging into `dir`. Safe to call more than once.
pub fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut g) = LOG.lock() {
        *g = Some(dir.join("sweeplark.log"));
    }
}

/// Folder with the log files, once [`init`] ran.
pub fn dir() -> Option<PathBuf> {
    LOG.lock().ok()?.as_ref().and_then(|p| p.parent().map(Path::to_path_buf))
}

fn write(level: &str, msg: &str) {
    let Ok(guard) = LOG.lock() else { return };
    let Some(path) = guard.as_ref() else { return };
    if std::fs::metadata(path).map(|m| m.len() > MAX).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("old.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = msg.replace(['\r', '\n'], " ");
        let _ = writeln!(f, "{now} {level:5} {line}");
    }
}

pub fn info(msg: &str) {
    write("INFO", msg);
}

pub fn warn(msg: &str) {
    write("WARN", msg);
}

pub fn error(msg: &str) {
    write("ERROR", msg);
}

/// Last lines of the log (for the bug report box).
pub fn tail(lines: usize) -> String {
    let Some(path) = LOG.lock().ok().and_then(|g| g.clone()) else { return String::new() };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_and_tails() {
        let d = tempfile::tempdir().unwrap();
        super::init(d.path());
        super::warn("hello\nworld");
        assert!(super::tail(5).contains("hello world"));
    }
}
