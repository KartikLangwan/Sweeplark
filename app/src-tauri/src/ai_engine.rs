//! Owns the local model process. Loads on demand, unloads when idle or when asked.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use sweeplark_core::ai::catalog::{Catalog, ModelSpec, Tier};
use sweeplark_core::ai::client::LlamaClient;
use sweeplark_core::ai::runtime::{self, AiInstallState, AiPaths, ModelServer, ServerOptions};
use sweeplark_core::settings::Settings;

struct Loaded {
    server: ModelServer,
    tier: Tier,
    last_used: Instant,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct InstallProgress {
    /// "runtime", "model", "done", "error"
    pub stage: String,
    pub tier: Option<Tier>,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

pub struct AiEngine {
    pub paths: AiPaths,
    pub catalog: Catalog,
    loaded: Mutex<Option<Loaded>>,
    pub progress: Mutex<Option<InstallProgress>>,
    pub cancel: AtomicBool,
}

impl AiEngine {
    pub fn new(data_dir: &Path, catalog: Catalog) -> Self {
        Self {
            paths: AiPaths::new(data_dir),
            catalog,
            loaded: Mutex::new(None),
            progress: Mutex::new(None),
            cancel: AtomicBool::new(false),
        }
    }

    pub fn install_state(&self) -> AiInstallState {
        runtime::install_state(&self.paths, &self.catalog)
    }

    pub fn loaded_tier(&self) -> Option<Tier> {
        self.loaded.lock().ok().and_then(|l| l.as_ref().map(|l| l.tier))
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded_tier().is_some()
    }

    /// The model the user picked, if AI is on and its files are present.
    pub fn usable_spec(&self, s: &Settings) -> Option<ModelSpec> {
        if !s.ai.enabled {
            return None;
        }
        let tier = s.ai.tier?;
        let st = self.install_state();
        (st.runtime_installed && st.installed_tiers.contains(&tier)).then(|| self.catalog.get(tier).clone())
    }

    /// Run `f` with the model loaded (loading it first if needed).
    pub fn with_model<T>(&self, s: &Settings, f: impl FnOnce(&LlamaClient, &ModelSpec) -> T) -> Result<T, String> {
        let spec = self.usable_spec(s).ok_or("AI is off or not installed")?;
        let mut guard = self.loaded.lock().map_err(|_| "AI engine busy")?;
        let needs_start = match guard.as_mut() {
            Some(l) => l.tier != spec.tier || !l.server.is_running(),
            None => true,
        };
        if needs_start {
            *guard = None; // stop the old one first: never two models in RAM
            let (exe, gpu_build) = runtime::find_server(&self.paths, &self.catalog).ok_or("AI engine not installed")?;
            let opts = ServerOptions::for_model(&spec, gpu_build && s.ai.use_gpu);
            let model = self.paths.model_file(&spec);
            let server = match ModelServer::start(&exe, &model, &opts) {
                Ok(s) => s,
                // GPU driver trouble: fall back to CPU once.
                Err(_) if opts.gpu_layers > 0 => {
                    ModelServer::start(&exe, &model, &ServerOptions { gpu_layers: 0, ..opts.clone() }).map_err(|e| e.to_string())?
                }
                Err(e) => return Err(e.to_string()),
            };
            *guard = Some(Loaded { server, tier: spec.tier, last_used: Instant::now() });
        }
        let loaded = guard.as_mut().expect("just loaded");
        let out = f(&loaded.server.client, &spec);
        loaded.last_used = Instant::now();
        Ok(out)
    }

    pub fn unload(&self) {
        if let Ok(mut g) = self.loaded.lock() {
            *g = None;
        }
    }

    pub fn unload_if_idle(&self, idle: Duration) {
        // try_lock: never wait behind a running answer.
        if let Ok(mut g) = self.loaded.try_lock()
            && g.as_ref().is_some_and(|l| l.last_used.elapsed() >= idle)
        {
            *g = None;
        }
    }

    pub fn set_progress(&self, p: InstallProgress) {
        if let Ok(mut g) = self.progress.lock() {
            *g = Some(p);
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}
