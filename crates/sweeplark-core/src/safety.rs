//! Path safety rules. The cleaner asks the guard before touching anything.

use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct SafetyGuard {
    /// Folders whose whole tree is off limits (Documents, Desktop, Pictures ...).
    personal: Vec<PathBuf>,
    /// Important folders: a cleaner may work *inside* them, but never on the folder
    /// itself or on any parent of it (user profile, Windows, Program Files, drive roots ...).
    critical: Vec<PathBuf>,
    /// Paths the user asked us to always skip.
    whitelist: Vec<PathBuf>,
}

/// Normalised form used for comparison: forward slashes, no trailing slash,
/// lower case on Windows (its file system is case-insensitive).
fn norm(p: &Path) -> PathBuf {
    let mut s = p.to_string_lossy().replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') && !s.ends_with(":/") {
        s.pop();
    }
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    PathBuf::from(s)
}

fn is_within(child: &Path, parent: &Path) -> bool {
    norm(child).starts_with(norm(parent))
}

impl SafetyGuard {
    pub fn new(personal: Vec<PathBuf>, critical: Vec<PathBuf>, whitelist: Vec<PathBuf>) -> Self {
        Self { personal, critical, whitelist }
    }

    /// Guard built from the real user's folders.
    pub fn for_current_user(whitelist: Vec<PathBuf>) -> Self {
        let mut personal = Vec::new();
        let mut critical = Vec::new();
        if let Some(user) = directories::UserDirs::new() {
            critical.push(user.home_dir().to_path_buf());
            for dir in [
                user.desktop_dir(),
                user.document_dir(),
                user.download_dir(),
                user.picture_dir(),
                user.video_dir(),
                user.audio_dir(),
                user.public_dir(),
            ]
            .into_iter()
            .flatten()
            {
                personal.push(dir.to_path_buf());
            }
            for name in ["OneDrive", "Favorites", "Contacts", "Saved Games", "Links", "source", "repos"] {
                personal.push(user.home_dir().join(name));
            }
        }
        for var in [
            "SystemRoot",
            "windir",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramData",
            "APPDATA",
            "LOCALAPPDATA",
            "USERPROFILE",
            "PUBLIC",
        ] {
            if let Ok(v) = std::env::var(var) {
                critical.push(PathBuf::from(v));
            }
        }
        if let Ok(root) = std::env::var("SystemRoot") {
            critical.push(Path::new(&root).join("System32"));
            critical.push(Path::new(&root).join("SysWOW64"));
            critical.push(Path::new(&root).join("WinSxS"));
        }
        for letter in 'A'..='Z' {
            critical.push(PathBuf::from(format!("{letter}:\\")));
        }
        critical.push(PathBuf::from("/"));
        Self { personal, critical, whitelist }
    }

    pub fn is_whitelisted(&self, path: &Path) -> bool {
        self.whitelist.iter().any(|w| is_within(path, w))
    }

    pub fn is_personal(&self, path: &Path) -> bool {
        self.personal.iter().any(|p| is_within(path, p))
    }

    /// Check that `base` may be used as the root of a cleaning target.
    pub fn check_base(&self, base: &Path) -> Result<()> {
        let unsafe_err = |reason: &str| Err(Error::Unsafe { path: base.to_path_buf(), reason: reason.into() });
        if !base.is_absolute() {
            return unsafe_err("path is not absolute");
        }
        if base.components().any(|c| matches!(c, Component::ParentDir)) {
            return unsafe_err("path contains '..'");
        }
        let normal_parts = base.components().filter(|c| matches!(c, Component::Normal(_))).count();
        if normal_parts < 2 {
            return unsafe_err("path is too close to the drive root");
        }
        if self.is_personal(base) {
            return unsafe_err("personal folder");
        }
        // base must not be a critical folder or a parent of one.
        if self.critical.iter().any(|c| is_within(c, base)) {
            return unsafe_err("important system or profile folder");
        }
        // base must not be a parent of a personal folder either.
        if self.personal.iter().any(|p| is_within(p, base)) {
            return unsafe_err("contains personal folders");
        }
        Ok(())
    }

    /// Check one file found while walking `base`.
    pub fn check_file(&self, base: &Path, file: &Path) -> Result<()> {
        if !is_within(file, base) {
            return Err(Error::Unsafe { path: file.to_path_buf(), reason: "outside the cleaning folder".into() });
        }
        if self.is_personal(file) {
            return Err(Error::Unsafe { path: file.to_path_buf(), reason: "personal folder".into() });
        }
        if self.is_whitelisted(file) {
            return Err(Error::Unsafe { path: file.to_path_buf(), reason: "on your whitelist".into() });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        PathBuf::from(if cfg!(windows) { "C:\\r" } else { "/r" })
    }

    fn guard(root: &Path) -> SafetyGuard {
        SafetyGuard::new(
            vec![root.join("home/docs"), root.join("home/Pictures")],
            vec![root.join("home"), root.join("home/appdata"), root.join("win")],
            vec![root.join("home/appdata/keep")],
        )
    }

    #[test]
    fn allows_cache_folders() {
        let root_buf = base();
        let root = root_buf.as_path();
        let g = guard(root);
        assert!(g.check_base(&root.join("home/appdata/Temp")).is_ok());
        assert!(g.check_base(&root.join("win/Temp")).is_ok());
    }

    #[test]
    fn blocks_critical_and_parents() {
        let root_buf = base();
        let root = root_buf.as_path();
        let g = guard(root);
        assert!(g.check_base(&root.join("home")).is_err());
        assert!(g.check_base(&root.join("home/appdata")).is_err());
        assert!(g.check_base(&root.join("home/appdata/")).is_err());
        assert!(g.check_base(root).is_err());
        assert!(g.check_base(Path::new("relative/path")).is_err());
        assert!(g.check_base(&root.join("home/appdata/Temp/../..")).is_err());
    }

    #[test]
    fn blocks_personal_trees() {
        let root_buf = base();
        let root = root_buf.as_path();
        let g = guard(root);
        assert!(g.check_base(&root.join("home/docs/cache")).is_err());
        assert!(g.check_file(&root.join("home/appdata/Temp"), &root.join("home/docs/a.txt")).is_err());
    }

    #[test]
    fn whitelist_and_outside_files() {
        let root_buf = base();
        let root = root_buf.as_path();
        let g = guard(root);
        let base = root.join("home/appdata");
        assert!(g.check_file(&base, &root.join("home/appdata/keep/x")).is_err());
        assert!(g.check_file(&root.join("win/Temp"), &root.join("win/System32/x.dll")).is_err());
        assert!(g.check_file(&root.join("win/Temp"), &root.join("win/Temp/x.tmp")).is_ok());
    }

    #[test]
    fn prefix_is_component_based() {
        let root_buf = base();
        let root = root_buf.as_path();
        let g = guard(root);
        // "/r/home/docs-cache" is not inside "/r/home/docs".
        assert!(!g.is_personal(&root.join("home/docs-cache/x")));
    }
}
