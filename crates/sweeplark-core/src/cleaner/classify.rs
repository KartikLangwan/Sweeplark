//! What kind of file is this? Used for folders like Temp where people sometimes keep
//! real files (an opened email attachment, an extracted zip). Anything that looks like
//! the user's own data is never deleted without their approval.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "class", content = "kind", rename_all = "snake_case")]
pub enum FileClass {
    /// Clearly temporary / cache / log.
    Junk,
    /// Looks like the user's own file (document, photo, video, archive, installer ...).
    UserData(&'static str),
    /// Not sure. Reviewed by the AI when available, otherwise asks the user.
    Unknown,
}

const JUNK: &[&str] = &[
    "tmp", "temp", "log", "etl", "evtx", "dmp", "mdmp", "hdmp", "old", "chk", "gid", "cache", "crdownload",
    "partial", "part", "dat", "db-journal", "db-wal", "db-shm", "ldb", "blob", "idx", "pma", "lock", "pf", "cab",
    "wer", "trace", "diagsession", "pdb", "ilk", "pch", "ipch", "tlog", "lastbuildstate", "thumbs",
];

const USER_DATA: &[(&str, &[&str])] = &[
    ("document", &["doc", "docx", "xls", "xlsx", "xlsm", "ppt", "pptx", "pdf", "odt", "ods", "odp", "rtf", "csv", "txt", "md", "epub", "pages", "numbers", "key"]),
    ("photo", &["jpg", "jpeg", "png", "gif", "heic", "heif", "webp", "bmp", "tif", "tiff", "raw", "cr2", "nef", "arw", "dng", "psd", "svg"]),
    ("video", &["mp4", "mov", "mkv", "avi", "wmv", "webm", "m4v", "3gp", "flv"]),
    ("audio", &["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "wma", "amr"]),
    ("archive", &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"]),
    ("installer", &["exe", "msi", "msix", "appx", "apk"]),
    ("project", &["obj", "fbx", "stl", "sln", "csproj", "py", "ipynb", "java", "kt", "rs", "go", "cpp", "c", "h", "cs", "js", "ts", "php", "sql", "blend", "fig", "ai", "indd", "dwg", "prproj", "aep"]),
    ("save / data", &["sav", "save", "bak", "kdbx", "pst", "ost", "eml", "msg", "vcf", "ics", "json", "xml", "db", "sqlite"]),
];

pub fn classify(path: &Path) -> FileClass {
    let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) else {
        // Extension-less files inside Temp are almost always program scratch files.
        return FileClass::Junk;
    };
    if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('~')) {
        return FileClass::Junk;
    }
    if JUNK.contains(&ext.as_str()) {
        return FileClass::Junk;
    }
    for (kind, exts) in USER_DATA {
        if exts.contains(&ext.as_str()) {
            return FileClass::UserData(kind);
        }
    }
    FileClass::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        assert_eq!(classify(Path::new("a/b.tmp")), FileClass::Junk);
        assert_eq!(classify(Path::new("a/data_3")), FileClass::Junk);
        assert_eq!(classify(Path::new("a/~DF12.TMP")), FileClass::Junk);
        assert_eq!(classify(Path::new("a/Invoice.PDF")), FileClass::UserData("document"));
        assert_eq!(classify(Path::new("a/IMG_1.jpg")), FileClass::UserData("photo"));
        assert_eq!(classify(Path::new("a/setup.exe")), FileClass::UserData("installer"));
        assert_eq!(classify(Path::new("a/thing.xyz")), FileClass::Unknown);
    }
}
