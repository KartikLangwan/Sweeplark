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
    /// Last change, Unix seconds (files only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
}

fn unix(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
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

/// Progress callback: (done, total, what). `total = 0` means "unknown".
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64, &str);

/// Bytes in use on the drive that holds `path` (to show a percentage while walking a drive).
fn drive_used(path: &Path) -> u64 {
    let p = path.to_string_lossy().to_lowercase();
    sysinfo::Disks::new_with_refreshed_list()
        .ok()
        .and_then(|d| {
            d.iter()
                .filter(|d| p.starts_with(&d.mount_point().to_string_lossy().to_lowercase()))
                .max_by_key(|d| d.mount_point().as_os_str().len())
                .map(|d| d.total_space().saturating_sub(d.available_space()))
        })
        .unwrap_or(0)
}

/// Walk `root` for at most `budget`. Never follows links.
pub fn analyze(root: &Path, budget: Duration) -> StorageReport {
    analyze_with(root, budget, &mut |_, _, _| {})
}

/// [`analyze`] with progress. For a whole drive the percentage is bytes seen / bytes used.
pub fn analyze_with(root: &Path, budget: Duration, progress: Progress) -> StorageReport {
    let started = Instant::now();
    let is_drive = root.parent().is_none() || root.components().count() <= 1;
    let expected = if is_drive { drive_used(root) } else { 0 };
    let mut last_report = Instant::now();
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
        if last_report.elapsed() > Duration::from_millis(250) {
            last_report = Instant::now();
            let where_ = entry.path().strip_prefix(root).ok().and_then(|r| r.components().next()).map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
            if expected > 0 {
                progress(total.min(expected), expected, &where_);
            } else {
                progress(files, 0, &where_);
            }
        }
        *kinds.entry(kind(entry.path())).or_default() += size;
        // Files directly in the root are not a "folder".
        if let Ok(rel) = entry.path().strip_prefix(root)
            && rel.components().count() > 1
            && let Some(first) = rel.components().next()
        {
            let key = root.join(first.as_os_str());
            let e = folders.entry(key).or_default();
            e.0 += size;
            e.1 += 1;
        }
        if size >= 50 * 1024 * 1024 {
            big.push(SizedPath { path: entry.path().display().to_string(), bytes: size, files: 1, modified: unix(&meta) });
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
        .map(|(p, (bytes, files))| SizedPath { path: p.display().to_string(), bytes, files, modified: None })
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

/// Why a path may not be sent to the Recycle Bin from the Storage page, if it may not.
pub fn recycle_refusal(path: &Path) -> Option<&'static str> {
    if !path.is_absolute() || path.parent().is_none() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Some("not a normal file path");
    }
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let p = norm(path);
    let under = |root: &Path| {
        let r = norm(root);
        !r.is_empty() && (p == r || p.starts_with(&format!("{r}\\")))
    };
    // Windows and installed programs.
    for var in ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"] {
        if let Some(v) = std::env::var_os(var)
            && under(Path::new(&v))
        {
            return Some("Windows or program folder");
        }
    }
    // Whole profile folders (the folder itself, not files inside).
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(u) = directories::UserDirs::new() {
        roots.push(u.home_dir().to_path_buf());
        roots.extend([u.desktop_dir(), u.document_dir(), u.download_dir(), u.picture_dir(), u.video_dir(), u.audio_dir()].into_iter().flatten().map(Path::to_path_buf));
        roots.push(u.home_dir().join("AppData"));
        roots.push(u.home_dir().join("OneDrive"));
        // Keys and credentials.
        for secret in [".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker"] {
            roots.push(u.home_dir().join(secret));
        }
    }
    for var in ["APPDATA", "LOCALAPPDATA", "USERPROFILE", "PUBLIC"] {
        if let Some(v) = std::env::var_os(var) {
            roots.push(PathBuf::from(v));
        }
    }
    if roots.iter().any(|r| norm(r) == p) {
        return Some("an important folder");
    }
    None
}

/// Send a file or folder to the Recycle Bin (recoverable). Windows, program folders and whole
/// profile folders are refused, and drives without a Recycle Bin (USB, network) too, because
/// Windows would delete there for good.
pub fn recycle(path: &Path) -> crate::Result<()> {
    if let Some(reason) = recycle_refusal(path) {
        return Err(crate::Error::Unsafe { path: path.into(), reason: reason.into() });
    }
    if !path.exists() {
        return Err(crate::Error::NotFound(path.display().to_string()));
    }
    if !crate::platform::system::has_recycle_bin(path) {
        return Err(crate::Error::Other(
            "This drive has no Recycle Bin (USB or network drive). Delete it in Explorer if you are sure.".into(),
        ));
    }
    crate::platform::system::to_recycle_bin(path)?;
    crate::log::info(&format!("moved to Recycle Bin: {}", path.display()));
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// Size of one copy.
    pub bytes: u64,
    /// All copies, oldest first (the first one is the suggested one to keep).
    pub files: Vec<SizedPath>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateReport {
    pub root: String,
    pub groups: Vec<DuplicateGroup>,
    /// Space freed if every extra copy went away.
    pub wasted_bytes: u64,
    pub scanned_files: u64,
    pub complete: bool,
    pub duration_ms: u64,
}

type Found = Vec<(PathBuf, Option<i64>)>;

fn hash_part(path: &Path, size: u64, full: bool) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    if full {
        loop {
            let n = f.read(&mut buf).ok()?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
    } else {
        let n = f.read(&mut buf).ok()?;
        h.update(&buf[..n]);
        if size > 128 * 1024 {
            f.seek(SeekFrom::End(-(64 * 1024))).ok()?;
            let n = f.read(&mut buf).ok()?;
            h.update(&buf[..n]);
        }
    }
    Some(h.finalize().into())
}

/// Find files with exactly the same content under `root` (size, then a quick hash of both
/// ends, then a full SHA-256). Read-only. Files smaller than `min_bytes` are ignored.
pub fn find_duplicates(root: &Path, min_bytes: u64, budget: Duration) -> DuplicateReport {
    find_duplicates_with(root, min_bytes, budget, &mut |_, _, _| {})
}

/// [`find_duplicates`] with progress: first files found (total unknown), then groups compared.
pub fn find_duplicates_with(root: &Path, min_bytes: u64, budget: Duration, progress: Progress) -> DuplicateReport {
    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut complete = true;
    let mut by_size: HashMap<u64, Found> = HashMap::new();
    let system_dirs: Vec<PathBuf> = ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    let mut scanned = 0u64;
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if started.elapsed() > budget {
            complete = false;
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        // Windows and program files are full of hard-linked copies that free nothing.
        if system_dirs.iter().any(|d| entry.path().starts_with(d)) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() < min_bytes.max(1) {
            continue;
        }
        scanned += 1;
        if last_report.elapsed() > Duration::from_millis(250) {
            last_report = Instant::now();
            progress(scanned, 0, "listing");
        }
        by_size.entry(meta.len()).or_default().push((entry.into_path(), unix(&meta)));
    }
    let mut groups = Vec::new();
    let candidates: Vec<(u64, Found)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    let n_cand = candidates.len() as u64;
    'outer: for (i, (size, files)) in candidates.into_iter().enumerate() {
        if last_report.elapsed() > Duration::from_millis(250) {
            last_report = Instant::now();
            progress(i as u64, n_cand, "comparing");
        }
        let mut quick: HashMap<[u8; 32], Found> = HashMap::new();
        for (p, m) in files {
            if started.elapsed() > budget * 2 {
                complete = false;
                break 'outer;
            }
            if let Some(h) = hash_part(&p, size, false) {
                quick.entry(h).or_default().push((p, m));
            }
        }
        for (_, cand) in quick.into_iter().filter(|(_, v)| v.len() > 1) {
            let mut full: HashMap<[u8; 32], Found> = HashMap::new();
            for (p, m) in cand {
                if let Some(h) = hash_part(&p, size, true) {
                    full.entry(h).or_default().push((p, m));
                }
            }
            for (_, mut same) in full.into_iter().filter(|(_, v)| v.len() > 1) {
                same.sort_by_key(|(_, m)| m.unwrap_or(i64::MAX));
                groups.push(DuplicateGroup {
                    bytes: size,
                    files: same
                        .into_iter()
                        .map(|(p, m)| SizedPath { path: p.display().to_string(), bytes: size, files: 1, modified: m })
                        .collect(),
                });
            }
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.bytes * (g.files.len() as u64 - 1)));
    groups.truncate(300);
    let wasted_bytes = groups.iter().map(|g| g.bytes * (g.files.len() as u64 - 1)).sum();
    DuplicateReport {
        root: root.display().to_string(),
        groups,
        wasted_bytes,
        scanned_files: scanned,
        complete,
        duration_ms: started.elapsed().as_millis() as u64,
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

    #[test]
    fn finds_exact_duplicates_only() {
        let d = tempfile::tempdir().unwrap();
        let a = vec![7u8; 300 * 1024];
        let mut b = a.clone();
        b[150 * 1024] = 1; // same size and same ends, different middle
        std::fs::write(d.path().join("a1.bin"), &a).unwrap();
        std::fs::create_dir_all(d.path().join("sub")).unwrap();
        std::fs::write(d.path().join("sub/a2.bin"), &a).unwrap();
        std::fs::write(d.path().join("b.bin"), &b).unwrap();
        let r = find_duplicates(d.path(), 1024, Duration::from_secs(10));
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].files.len(), 2);
        assert_eq!(r.wasted_bytes, 300 * 1024);
    }

    #[test]
    fn refuses_important_folders() {
        assert!(recycle_refusal(Path::new("relative/x")).is_some());
        if let Some(home) = directories::UserDirs::new() {
            assert!(recycle_refusal(home.home_dir()).is_some());
            assert!(recycle_refusal(&home.home_dir().join("some-file.iso")).is_none());
        }
    }
}
