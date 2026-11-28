//! Local AI assistant.
//!
//! How it stays reliable on tiny models:
//! 1. The app picks the tools itself ([`router`]) and runs them ([`facts`]); the model only
//!    reads a short list of measured facts. No free-form tool calling needed.
//! 2. Bigger models may ask for extra tools, but only through a JSON grammar, so the reply
//!    is always valid.
//! 3. Every answer goes through [`guard`]: repeated text is cut, made-up numbers or bad
//!    advice make the app fall back to its own rule-based answer.
//! 4. The model never acts. Buttons shown with the answer come from the app's findings.

pub mod catalog;
pub mod client;
pub mod download;
pub mod facts;
pub mod guard;
pub mod prompt;
pub mod review;
pub mod router;
pub mod runtime;

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::json;

use self::catalog::{AnswerStyle, ModelSpec};
use self::client::{ChatMessage, LlamaClient, Sampling};
use self::facts::{Facts, Tool};
use crate::advisor::{self, Finding, Severity, SuggestedAction, Topic};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplySource {
    /// Written by the local model (and passed the guard).
    Model,
    /// The app's own rule-based answer (no model, or the model's answer was rejected).
    Rules,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantReply {
    pub text: String,
    pub source: ReplySource,
    /// The app's own findings shown under the answer.
    pub key_points: Vec<Finding>,
    pub tools: Vec<Tool>,
    pub actions: Vec<SuggestedAction>,
    pub model: Option<String>,
}

fn topics_for(tools: &BTreeSet<Tool>) -> Vec<Topic> {
    let mut t = Vec::new();
    for tool in tools {
        match tool {
            Tool::Performance | Tool::Startup => t.push(Topic::Performance),
            Tool::Storage => t.push(Topic::Storage),
            Tool::Security => t.push(Topic::Security),
            Tool::Battery => t.push(Topic::Battery),
            Tool::Heat => t.push(Topic::Heat),
            Tool::Graphics | Tool::Background | Tool::Optimizer => t.push(Topic::Performance),
            Tool::Specs | Tool::History | Tool::Problems | Tool::Apps => {}
        }
    }
    t
}

fn pick_key_points(findings: &[Finding], tools: &BTreeSet<Tool>) -> Vec<Finding> {
    let topics = topics_for(tools);
    let mut chosen: Vec<Finding> = findings
        .iter()
        .filter(|f| f.severity != Severity::Good)
        .filter(|f| topics.is_empty() || f.topics.iter().any(|t| topics.contains(t)))
        .take(4)
        .cloned()
        .collect();
    if chosen.is_empty() {
        chosen = findings.iter().take(1).cloned().collect();
    }
    chosen
}

/// Let a bigger model add tools for a vague question (grammar-constrained JSON).
fn model_pick_tools(client: &LlamaClient, question: &str) -> Vec<Tool> {
    let list: Vec<(&str, &str)> = Tool::ALL.iter().map(|t| (t.name(), t.about())).collect();
    let names: Vec<&str> = list.iter().map(|(n, _)| *n).collect();
    let schema = json!({
        "type": "object",
        "properties": { "checks": { "type": "array", "items": { "enum": names }, "maxItems": 3 } },
        "required": ["checks"]
    });
    let msgs = [ChatMessage::system(prompt::tool_pick_system(&list)), ChatMessage::user(question)];
    client
        .chat_json(&msgs, &schema, 60)
        .ok()
        .and_then(|v| v["checks"].as_array().cloned())
        .map(|a| a.iter().filter_map(|x| x.as_str().and_then(Tool::from_name)).collect())
        .unwrap_or_default()
}

/// Answer a question about this PC.
///
/// `model`: the running model, if AI is on. `on_token` receives streamed text; the returned
/// reply's `text` is final (it may differ if the guard rejected the streamed answer).
pub fn answer(
    question: &str,
    history: &[ChatMessage],
    facts: &Facts,
    model: Option<(&LlamaClient, &ModelSpec)>,
    on_token: &mut dyn FnMut(&str) -> bool,
) -> AssistantReply {
    let mut tools = router::route(question);
    if let Some((client, spec)) = model
        && spec.style == AnswerStyle::Full
        && router::is_vague(&tools)
    {
        tools.extend(model_pick_tools(client, question));
    }
    let facts_text: String =
        tools.iter().map(|t| format!("[{}]\n{}", t.name(), facts.run(*t))).collect::<Vec<_>>().join("\n");
    let findings = facts.findings();
    let key_points = pick_key_points(&findings, &tools);
    let mut actions: Vec<SuggestedAction> = Vec::new();
    for a in key_points.iter().filter_map(|f| f.action.clone()) {
        if !actions.contains(&a) {
            actions.push(a);
        }
    }
    // Actions for the new tools: buttons that open the right page (the user still decides).
    let extra = [
        (Tool::Background, SuggestedAction::Boost),
        (Tool::Optimizer, SuggestedAction::Optimize),
        (Tool::Apps, SuggestedAction::Apps),
    ];
    for (tool, action) in extra {
        if tools.contains(&tool) && !actions.contains(&action) {
            actions.push(action);
        }
    }
    let mut rules_text = advisor::ask(question, &findings, facts.lang).text;
    // Without a model, still show what the extra tools found.
    for t in [Tool::Background, Tool::Optimizer, Tool::Graphics, Tool::Apps] {
        if tools.contains(&t) {
            let line = facts.run(t);
            if !line.ends_with("not available.") {
                rules_text.push_str("\n\n");
                rules_text.push_str(&line);
            }
        }
    }
    let base = AssistantReply {
        text: rules_text.clone(),
        source: ReplySource::Rules,
        key_points: key_points.clone(),
        tools: tools.iter().copied().collect(),
        actions,
        model: None,
    };
    let Some((client, spec)) = model else { return base };

    let points: Vec<String> = key_points.iter().map(|f| format!("{} — {}", f.title, f.detail)).collect();
    let mut msgs = vec![ChatMessage::system(prompt::chat_system(spec.style, &points, &facts_text, facts.lang))];
    let keep = if spec.style == AnswerStyle::Brief { 2 } else { 6 };
    msgs.extend(history.iter().rev().take(keep).rev().cloned());
    msgs.push(ChatMessage::user(question));
    let sampling = Sampling { max_tokens: if spec.style == AnswerStyle::Brief { 90 } else { 240 }, temperature: 0.3 };
    let grounding = format!("{facts_text}\n{}", points.join("\n"));
    match client.chat_stream(&msgs, &sampling, on_token) {
        Ok(raw) => match guard::check_chat(&raw, &grounding) {
            Some(text) => AssistantReply { text, source: ReplySource::Model, model: Some(spec.name.clone()), ..base },
            None => base,
        },
        Err(_) => base,
    }
}

/// Short friendly text for a notification or summary. `None` if the model's text failed
/// the guard (callers then use their own template text).
pub fn narrate(client: &LlamaClient, kind: &str, points: &[String], facts: &str, lang: crate::Lang) -> Option<String> {
    let msgs = [
        ChatMessage::system(prompt::narrate_system(kind, points, facts, lang)),
        ChatMessage::user(format!("Write the {kind} now.")),
    ];
    let raw = client.chat(&msgs, &Sampling { max_tokens: 120, temperature: 0.3 }).ok()?;
    // Same strict check as chat: a report must never claim "I closed Chrome".
    guard::check_chat(&raw, &format!("{facts}\n{}", points.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::facts::test_support::FakePc;
    use super::*;

    #[test]
    fn without_model_uses_rules_and_actions() {
        let pc = FakePc;
        let f = Facts::new(&pc, crate::Lang::Hinglish);
        let r = answer("mera pc slow kyun hai", &[], &f, None, &mut |_| true);
        assert_eq!(r.source, ReplySource::Rules);
        assert!(r.tools.contains(&Tool::Performance));
        assert!(r.key_points.iter().any(|k| k.id == "ram.high"));
        assert!(!r.actions.is_empty());
        assert!(r.text.contains("RAM"));
    }
}
