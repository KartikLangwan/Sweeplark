//! Storage analyzer: which folders and files take the most space. Read-only; removing a
//! file always goes to the Recycle Bin and only when the user clicks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct SizedPath {
    pub path: String,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageReport {
    pub root: String,
    pub total_bytes: u64,
    pub total_files: u64,
    /// Direct sub-folders of the root, biggest first.
    pub folders: Vec<SizedPath>,
    /// Biggest single files.
    pub big_files: Vec<SizedPath>,
    /// Bytes per kind: video, photo, audio, document, archive, installer, other.
    pub by_kind: Vec<(String, u64)>,
    pub complete: bool,
    pub duration_ms: u64,
}

fn kind(path: &Path) -> &'static str {
    match crate::cleaner::classify::classify(path) {
        crate::cleaner::classify::FileClass::UserData(k) => k,
        crate::cleaner::classify::FileClass::Junk => "temp / cache",
        crate::cleaner::classify::FileClass::Unknown => "other",
    }
}

/// Walk `root` for at most `budget`. Never follows links.
pub fn analyze(root: &Path, budget: Duration) -> StorageReport {
    let started = Instant::now();
    let mut folders: HashMap<PathBuf, (u64, u64)> = HashMap::new();
    let mut kinds: HashMap<&'static str, u64> = HashMap::new();
    let mut big: Vec<SizedPath> = Vec::new();
    let (mut total, mut files, mut complete) = (0u64, 0u64, true);
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if started.elapsed() > budget {
            complete = false;
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let size = meta.len();
        total += size;
        files += 1;
        *kinds.entry(kind(entry.path())).or_default() += size;
        if let Ok(rel) = entry.path().strip_prefix(root)
            && let Some(first) = rel.components().next()
        {
            let key = root.join(first.as_os_str());
            let e = folders.entry(key).or_default();
            e.0 += size;
            e.1 += 1;
        }
        if size >= 50 * 1024 * 1024 {
            big.push(SizedPath { path: entry.path().display().to_string(), bytes: size, files: 1 });
            if big.len() > 400 {
                big.sort_by_key(|b| std::cmp::Reverse(b.bytes));
                big.truncate(100);
            }
        }
    }
    big.sort_by_key(|b| std::cmp::Reverse(b.bytes));
    big.truncate(60);
    let mut folders: Vec<SizedPath> = folders
        .into_iter()
        .map(|(p, (bytes, files))| SizedPath { path: p.display().to_string(), bytes, files })
        .collect();
    folders.sort_by_key(|f| std::cmp::Reverse(f.bytes));
    folders.truncate(40);
    let mut by_kind: Vec<(String, u64)> = kinds.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    by_kind.sort_by_key(|k| std::cmp::Reverse(k.1));
    StorageReport {
        root: root.display().to_string(),
        total_bytes: total,
        total_files: files,
        folders,
        big_files: big,
        by_kind,
        complete,
        duration_ms: started.elapsed().as_millis() as u64,
    }
}

/// Send a file or folder to the Recycle Bin (recoverable). Personal-folder roots themselves
/// and anything inside Windows / Program Files are refused.
pub fn recycle(path: &Path) -> crate::Result<()> {
    let p = path.to_string_lossy().to_lowercase().replace('/', "\\");
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    let forbidden = [windows, r"c:\program files".into(), r"c:\program files (x86)".into(), r"c:\programdata".into()];
    if forbidden.iter().any(|f| p.starts_with(f.as_str())) || path.parent().is_none() {
        return Err(crate::Error::Unsafe { path: path.into(), reason: "system folder".into() });
    }
    #[cfg(windows)]
    {
        trash::delete(path).map_err(|e| crate::Error::Other(format!("could not move to Recycle Bin: {e}")))
    }
    #[cfg(not(windows))]
    {
        Err(crate::Error::NotSupported("recycle bin".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_folders_and_big_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Videos")).unwrap();
        std::fs::create_dir_all(d.path().join("Docs")).unwrap();
        std::fs::write(d.path().join("Videos/a.mp4"), vec![0u8; 60 * 1024 * 1024]).unwrap();
        std::fs::write(d.path().join("Docs/b.pdf"), vec![0u8; 1000]).unwrap();
        let r = analyze(d.path(), Duration::from_secs(10));
        assert!(r.complete);
        assert_eq!(r.total_files, 2);
        assert!(r.folders[0].path.ends_with("Videos"));
        assert_eq!(r.big_files.len(), 1);
        assert_eq!(r.by_kind[0].0, "video");
    }
}
