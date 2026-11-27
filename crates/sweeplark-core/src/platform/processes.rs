use std::collections::BTreeSet;

/// Lower-case executable names of running processes (e.g. `chrome.exe`).
#[derive(Debug, Clone, Default)]
pub struct ProcessList {
    names: BTreeSet<String>,
}

impl ProcessList {
    pub fn from_names<I: IntoIterator<Item = S>, S: AsRef<str>>(names: I) -> Self {
        Self { names: names.into_iter().map(|n| n.as_ref().to_lowercase()).collect() }
    }

    pub fn current() -> Self {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
        let Ok(mut sys) = System::new_with_specifics(RefreshKind::nothing()) else {
            return Self::default();
        };
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        Self::from_names(sys.processes().values().map(|p| p.name().to_string_lossy().into_owned()))
    }

    /// Which of `wanted` are running. Matching ignores case and a missing `.exe`.
    pub fn running<'a>(&self, wanted: &'a [String]) -> Vec<&'a str> {
        wanted
            .iter()
            .filter(|w| {
                let w = w.to_lowercase();
                let bare = w.trim_end_matches(".exe");
                self.names.contains(&w)
                    || self.names.contains(bare)
                    || self.names.contains(&format!("{bare}.exe"))
            })
            .map(String::as_str)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_with_or_without_exe() {
        let p = ProcessList::from_names(["Chrome.exe", "firefox"]);
        let wanted = vec!["chrome.exe".to_string(), "firefox.exe".to_string(), "msedge.exe".to_string()];
        assert_eq!(p.running(&wanted), vec!["chrome.exe", "firefox.exe"]);
    }
}
