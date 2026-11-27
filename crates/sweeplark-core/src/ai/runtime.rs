//! Installing and running llama.cpp's `llama-server`.
//!
//! The model is never kept loaded: a [`ModelServer`] is started when an answer is needed and
//! the process is killed when it is dropped, giving all its RAM back to Windows.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::catalog::{Catalog, ModelSpec, Tier};
use super::client::LlamaClient;
use super::download::{self, Progress};
use crate::{Error, Result};

/// Folder layout under the app's data folder: `ai/runtime/<tag>-<cpu|gpu>/`, `ai/models/`.
#[derive(Debug, Clone)]
pub struct AiPaths {
    pub root: PathBuf,
}

impl AiPaths {
    pub fn new(data_dir: &Path) -> Self {
        Self { root: data_dir.join("ai") }
    }

    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    pub fn model_file(&self, spec: &ModelSpec) -> PathBuf {
        self.models_dir().join(&spec.file)
    }

    pub fn runtime_dir(&self, tag: &str, gpu: bool) -> PathBuf {
        self.root.join("runtime").join(format!("{tag}-{}", if gpu { "gpu" } else { "cpu" }))
    }

    /// Disk space used by AI files.
    pub fn disk_usage(&self) -> u64 {
        walkdir::WalkDir::new(&self.root)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AiInstallState {
    pub runtime_installed: bool,
    pub runtime_gpu: bool,
    pub installed_tiers: Vec<Tier>,
    pub disk_bytes: u64,
}

fn server_exe_name() -> &'static str {
    if cfg!(windows) { "llama-server.exe" } else { "llama-server" }
}

/// Find `llama-server` inside a runtime folder. `SWEEPLARK_LLAMA_SERVER` overrides (development).
pub fn find_server(paths: &AiPaths, catalog: &Catalog) -> Option<(PathBuf, bool)> {
    if let Some(p) = std::env::var_os("SWEEPLARK_LLAMA_SERVER") {
        return Some((PathBuf::from(p), false));
    }
    for gpu in [true, false] {
        let dir = paths.runtime_dir(&catalog.runtime.llama_cpp_tag, gpu);
        let found = walkdir::WalkDir::new(&dir)
            .max_depth(3)
            .into_iter()
            .flatten()
            .find(|e| e.file_type().is_file() && e.file_name() == server_exe_name());
        if let Some(e) = found {
            return Some((e.into_path(), gpu));
        }
    }
    None
}

pub fn install_state(paths: &AiPaths, catalog: &Catalog) -> AiInstallState {
    let runtime = find_server(paths, catalog);
    AiInstallState {
        runtime_installed: runtime.is_some(),
        runtime_gpu: runtime.map(|r| r.1).unwrap_or(false),
        installed_tiers: catalog
            .models
            .iter()
            .filter(|m| std::fs::metadata(paths.model_file(m)).map(|md| md.len() == m.size).unwrap_or(false))
            .map(|m| m.tier)
            .collect(),
        disk_bytes: paths.disk_usage(),
    }
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    assets: Vec<GhAsset>,
}

/// Download and unpack llama.cpp for Windows (Vulkan build when `gpu`, else CPU build).
pub fn install_runtime(paths: &AiPaths, catalog: &Catalog, gpu: bool, progress: Progress) -> Result<PathBuf> {
    let rt = &catalog.runtime;
    let url = format!("https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/{}", rt.llama_cpp_tag);
    let mut resp = download::web_agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| Error::Other(format!("could not reach GitHub: {e}")))?;
    if resp.status().as_u16() != 200 {
        return Err(Error::Other(format!("GitHub answered HTTP {}", resp.status().as_u16())));
    }
    let release: GhRelease = resp.body_mut().read_json().map_err(|e| Error::Other(format!("bad release info: {e}")))?;
    let suffix = if gpu { &rt.gpu_asset_suffix } else { &rt.cpu_asset_suffix };
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(suffix.as_str()) && !a.name.starts_with("cudart"))
        .ok_or_else(|| Error::NotFound(format!("llama.cpp asset *{suffix}")))?;
    let sha = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or_else(|| Error::Other("release asset has no checksum; refusing to install".into()))?;
    let zip_path = paths.root.join("downloads").join(&asset.name);
    download::download(&asset.browser_download_url, &zip_path, Some(asset.size), sha, progress)?;
    let dir = paths.runtime_dir(&rt.llama_cpp_tag, gpu);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    download::extract_zip(&zip_path, &dir)?;
    let _ = std::fs::remove_file(&zip_path);
    find_server(paths, catalog).map(|(p, _)| p).ok_or_else(|| Error::NotFound("llama-server in the downloaded zip".into()))
}

/// Download one model file (resumable, hash-checked).
pub fn install_model(paths: &AiPaths, spec: &ModelSpec, progress: Progress) -> Result<PathBuf> {
    let dest = paths.model_file(spec);
    download::download(&spec.download_url(), &dest, Some(spec.size), &spec.sha256, progress)?;
    Ok(dest)
}

pub fn remove_model(paths: &AiPaths, spec: &ModelSpec) -> Result<()> {
    let p = paths.model_file(spec);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub context: u32,
    /// Layers on the GPU (0 = CPU only).
    pub gpu_layers: u32,
    pub threads: Option<usize>,
    pub startup_timeout: Duration,
}

impl ServerOptions {
    pub fn for_model(spec: &ModelSpec, gpu: bool) -> Self {
        Self {
            context: spec.context,
            gpu_layers: if gpu { 99 } else { 0 },
            threads: None,
            startup_timeout: Duration::from_secs(90),
        }
    }
}

/// A running model. Killed (and its memory freed) on drop.
pub struct ModelServer {
    child: Child,
    pub client: LlamaClient,
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

impl ModelServer {
    pub fn start(exe: &Path, model: &Path, opts: &ServerOptions) -> Result<Self> {
        let port = free_port()?;
        // Leave a core for the user so the PC stays responsive.
        let threads = opts.threads.unwrap_or_else(|| {
            std::thread::available_parallelism().map(|n| n.get().saturating_sub(1).clamp(1, 8)).unwrap_or(2)
        });
        let mut cmd = Command::new(exe);
        cmd.arg("--model")
            .arg(model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--ctx-size", &opts.context.to_string()])
            .args(["--n-gpu-layers", &opts.gpu_layers.to_string()])
            .args(["--threads", &threads.to_string()])
            .args(["--parallel", "1", "--no-webui", "--jinja"])
            .args(["--reasoning-budget", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
            cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
        }
        let child = cmd.spawn().map_err(|e| Error::Other(format!("could not start the AI engine: {e}")))?;
        let mut server = ModelServer { child, client: LlamaClient::new(port) };
        let started = Instant::now();
        loop {
            if let Ok(Some(status)) = server.child.try_wait() {
                return Err(Error::Other(format!("AI engine stopped while loading ({status})")));
            }
            if server.client.is_ready() {
                return Ok(server);
            }
            if started.elapsed() > opts.startup_timeout {
                return Err(Error::Other("AI engine took too long to load".into()));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for ModelServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
