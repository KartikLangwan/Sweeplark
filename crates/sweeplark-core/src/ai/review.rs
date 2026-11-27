//! AI second opinion on files the rules are unsure about. The model is grammar-forced to
//! answer in JSON. Anything it does not answer for counts as risky.

use std::path::PathBuf;

use serde_json::json;

use super::client::{ChatMessage, LlamaClient};
use super::prompt::review_system;
use crate::util::format_bytes;

#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub cleaner: String,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub path: PathBuf,
    pub risky: bool,
    pub reason: String,
}

const BATCH: usize = 10;
/// Cap per run, so a cleanup never keeps the model busy for long.
pub const MAX_ITEMS: usize = 60;

fn schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "n": { "type": "integer" },
                        "risk": { "enum": ["safe", "risky"] },
                        "why": { "type": "string", "maxLength": 80 }
                    },
                    "required": ["n", "risk", "why"]
                }
            }
        },
        "required": ["items"]
    })
}

/// Review items. Errors or missing answers make an item risky (the user will be asked).
pub fn review(client: &LlamaClient, items: &[ReviewItem]) -> Vec<Verdict> {
    let mut out = Vec::new();
    for chunk in items.iter().take(MAX_ITEMS).collect::<Vec<_>>().chunks(BATCH) {
        let list = chunk
            .iter()
            .enumerate()
            .map(|(i, it)| format!("{}. {}: {} ({})", i + 1, it.cleaner, it.path.display(), format_bytes(it.bytes)))
            .collect::<Vec<_>>()
            .join("\n");
        let msgs = [ChatMessage::system(review_system()), ChatMessage::user(format!("Items:\n{list}"))];
        let answer = client.chat_json(&msgs, &schema(), 90 * chunk.len() as u32).ok();
        for (i, it) in chunk.iter().enumerate() {
            let verdict = answer.as_ref().and_then(|v| {
                v["items"].as_array()?.iter().find(|x| x["n"].as_u64() == Some(i as u64 + 1)).map(|x| {
                    (x["risk"].as_str() != Some("safe"), x["why"].as_str().unwrap_or_default().to_string())
                })
            });
            let (risky, reason) = verdict.unwrap_or((true, "AI could not check this file".into()));
            out.push(Verdict { path: it.path.clone(), risky, reason });
        }
    }
    out
}
