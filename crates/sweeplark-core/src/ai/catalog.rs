//! The three model tiers and how to pick one for this PC.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Lite,
    Balanced,
    Power,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerStyle {
    /// Model writes 1-2 sentences; the app shows its own findings next to it.
    Brief,
    /// Model writes the whole answer from the facts.
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub tier: Tier,
    pub name: String,
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub license: String,
    pub context: u32,
    pub style: AnswerStyle,
    pub description: String,
}

impl ModelSpec {
    pub fn download_url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repo, self.revision, self.file)
    }

    /// Rough RAM the running model needs (weights + context + runtime).
    pub fn ram_needed(&self) -> u64 {
        self.size + u64::from(self.context) * 64 * 1024 + 300 * 1024 * 1024
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSpec {
    pub llama_cpp_tag: String,
    pub cpu_asset_suffix: String,
    pub gpu_asset_suffix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub runtime: RuntimeSpec,
    #[serde(rename = "model")]
    pub models: Vec<ModelSpec>,
}

impl Catalog {
    pub fn builtin() -> Self {
        toml::from_str(include_str!("../../../../rules/ai/models.toml")).expect("built-in model catalog broken")
    }

    pub fn get(&self, tier: Tier) -> &ModelSpec {
        self.models.iter().find(|m| m.tier == tier).expect("every tier is in the catalog")
    }
}

/// Pick a tier from total RAM and whether a dedicated graphics card is present.
pub fn recommend(total_ram: u64, dedicated_gpu: bool) -> Tier {
    let gb = total_ram as f64 / 1024f64.powi(3);
    if gb >= 15.0 || (dedicated_gpu && gb >= 11.0) {
        Tier::Power
    } else if gb >= 7.0 {
        Tier::Balanced
    } else {
        Tier::Lite
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn catalog_has_three_pinned_tiers() {
        let c = Catalog::builtin();
        for t in [Tier::Lite, Tier::Balanced, Tier::Power] {
            let m = c.get(t);
            assert_eq!(m.sha256.len(), 64);
            assert_eq!(m.revision.len(), 40, "pin to a commit, not a branch");
            assert!(m.download_url().starts_with("https://huggingface.co/"));
        }
        assert!(c.get(Tier::Lite).size < c.get(Tier::Balanced).size);
    }

    #[test]
    fn recommends_by_ram() {
        assert_eq!(recommend(4 * GB, false), Tier::Lite);
        assert_eq!(recommend(8 * GB - GB / 2, false), Tier::Balanced);
        assert_eq!(recommend(12 * GB, true), Tier::Power);
        assert_eq!(recommend(12 * GB, false), Tier::Balanced);
        assert_eq!(recommend(32 * GB, false), Tier::Power);
    }
}
