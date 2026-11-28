//! Resumable downloads with SHA-256 verification. Nothing is used until the hash matches.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Progress callback: (downloaded bytes, total bytes). Return `false` to cancel.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64) -> bool;

pub(crate) fn web_agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // Per read, not for the whole body: a 2.5 GB model on a slow line takes many
        // minutes, but a connection that goes silent for a minute is dead.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .user_agent(concat!("sweeplark/", env!("CARGO_PKG_VERSION")))
        .http_status_as_error(false)
        .build()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Download `url` to `dest`. Resumes a previous partial download (`dest.part`).
pub fn download(url: &str, dest: &Path, expected_size: Option<u64>, sha256: &str, progress: Progress) -> Result<()> {
    if dest.exists() && sha256_file(dest)?.eq_ignore_ascii_case(sha256) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = dest.with_extension("part");
    let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if expected_size.is_some_and(|s| have > s) {
        fs::remove_file(&part)?;
        have = 0;
    }

    let agent = web_agent();
    let mut req = agent.get(url);
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let resp = req.call().map_err(|e| Error::Other(format!("download failed: {e}")))?;
    let status = resp.status().as_u16();
    let resumed = status == 206;
    if !(200..300).contains(&status) {
        return Err(Error::Other(format!("download failed: HTTP {status} for {url}")));
    }
    if !resumed {
        have = 0;
    }
    let body_len: Option<u64> = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let total = expected_size.or(body_len.map(|l| l + have)).unwrap_or(0);

    let mut out = OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&part)?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = have;
    loop {
        let n = reader.read(&mut buf).map_err(|e| Error::Other(format!("download interrupted: {e}")))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        if !progress(done, total) {
            out.flush()?;
            return Err(Error::Other("download cancelled".into()));
        }
    }
    out.flush()?;
    drop(out);

    let actual = sha256_file(&part)?;
    if !actual.eq_ignore_ascii_case(sha256) {
        let _ = fs::remove_file(&part);
        return Err(Error::Other(format!("downloaded file is damaged or changed (sha256 {actual})")));
    }
    fs::rename(&part, dest)?;
    Ok(())
}

/// Extract a zip into `dir`, refusing entries that would escape it.
pub fn extract_zip(zip_path: &Path, dir: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Error::Other(format!("bad zip: {e}")))?;
    fs::create_dir_all(dir)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| Error::Other(format!("bad zip: {e}")))?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dir.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p)?;
        }
        let mut f = File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn zip_roundtrip_and_no_escape() {
        let dir = tempfile::tempdir().unwrap();
        let zp = dir.path().join("t.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&zp).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("bin/llama-server.exe", opts).unwrap();
            w.write_all(b"exe").unwrap();
            w.start_file("../evil.txt", opts).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let out = dir.path().join("out");
        extract_zip(&zp, &out).unwrap();
        assert!(out.join("bin/llama-server.exe").exists());
        assert!(!dir.path().join("evil.txt").exists());
    }
}
