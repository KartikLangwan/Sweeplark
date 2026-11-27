//! Reading the Windows recycle bin directly, so items can be removed only after N days.
//!
//! Each deleted item has two entries in `X:\$Recycle.Bin\<SID>\`:
//! `$Ixxxxxx` (metadata: size, deletion time, original path) and `$Rxxxxxx` (the data).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::util::filetime_to_system_time;

#[derive(Debug, Clone)]
pub struct RecycleEntry {
    pub i_path: PathBuf,
    pub r_path: PathBuf,
    pub original_path: String,
    pub size: u64,
    pub deleted_at: Option<SystemTime>,
}

/// Parse a `$I` file. Returns (original size, deletion FILETIME, original path).
pub fn parse_i_file(bytes: &[u8]) -> Option<(u64, u64, String)> {
    if bytes.len() < 24 {
        return None;
    }
    let version = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    let size = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    let filetime = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
    let path_bytes: &[u8] = match version {
        // Windows Vista - 8.1: fixed 260 UTF-16 chars.
        1 => bytes.get(24..24 + 520)?,
        // Windows 10+: u32 length (chars, incl. NUL) then the path.
        2 => {
            let len = u32::from_le_bytes(bytes.get(24..28)?.try_into().ok()?) as usize;
            bytes.get(28..28 + len.checked_mul(2)?)?
        }
        _ => return None,
    };
    let wide: Vec<u16> = path_bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    Some((size, filetime, String::from_utf16_lossy(&wide[..end])))
}

/// List items in one user's recycle bin folder.
pub fn scan_recycle_dir(dir: &Path) -> Vec<RecycleEntry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(suffix) = name.strip_prefix("$I") else { continue };
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let i_path = entry.path();
        let Ok(bytes) = std::fs::read(&i_path) else { continue };
        let Some((size, filetime, original_path)) = parse_i_file(&bytes) else { continue };
        out.push(RecycleEntry {
            r_path: dir.join(format!("$R{suffix}")),
            i_path,
            original_path,
            size,
            deleted_at: filetime_to_system_time(filetime),
        });
    }
    out
}

/// Recycle bin folders of the current user on all drives.
pub fn current_user_recycle_dirs() -> Vec<PathBuf> {
    let Some(sid) = crate::platform::system::current_user_sid() else { return Vec::new() };
    ('A'..='Z')
        .map(|d| PathBuf::from(format!("{d}:\\$Recycle.Bin\\{sid}")))
        .filter(|p| p.is_dir())
        .collect()
}

#[cfg(test)]
pub(crate) fn make_i_file_v2(size: u64, filetime: u64, path: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&2u64.to_le_bytes());
    b.extend_from_slice(&size.to_le_bytes());
    b.extend_from_slice(&filetime.to_le_bytes());
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    b.extend_from_slice(&(wide.len() as u32).to_le_bytes());
    for c in wide {
        b.extend_from_slice(&c.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::system_time_to_filetime;

    #[test]
    fn parses_v2() {
        let ft = system_time_to_filetime(SystemTime::now());
        let bytes = make_i_file_v2(1234, ft, r"C:\Users\amit\old.zip");
        let (size, t, path) = parse_i_file(&bytes).unwrap();
        assert_eq!((size, t, path.as_str()), (1234, ft, r"C:\Users\amit\old.zip"));
    }

    #[test]
    fn parses_v1_and_rejects_garbage() {
        let mut b = Vec::new();
        b.extend_from_slice(&1u64.to_le_bytes());
        b.extend_from_slice(&10u64.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        let mut path: Vec<u16> = "D:\\a.txt".encode_utf16().collect();
        path.resize(260, 0);
        for c in path {
            b.extend_from_slice(&c.to_le_bytes());
        }
        assert_eq!(parse_i_file(&b).unwrap().2, "D:\\a.txt");
        assert!(parse_i_file(&[1, 2, 3]).is_none());
        let mut bad = make_i_file_v2(1, 1, "x");
        bad[0] = 9;
        assert!(parse_i_file(&bad).is_none());
        let mut truncated = make_i_file_v2(1, 1, "long path");
        truncated.truncate(30);
        assert!(parse_i_file(&truncated).is_none());
    }

    #[test]
    fn scans_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ft = system_time_to_filetime(SystemTime::now());
        std::fs::write(dir.path().join("$IABC.txt"), make_i_file_v2(5, ft, r"C:\x.txt")).unwrap();
        std::fs::write(dir.path().join("$RABC.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("desktop.ini"), b"").unwrap();
        let items = scan_recycle_dir(dir.path());
        assert_eq!(items.len(), 1);
        assert!(items[0].r_path.ends_with("$RABC.txt"));
    }
}
