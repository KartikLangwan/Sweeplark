//! Minimal client for llama-server's OpenAI-compatible API on 127.0.0.1.

use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(c: impl Into<String>) -> Self {
        Self { role: "system".into(), content: c.into() }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Self { role: "user".into(), content: c.into() }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: c.into() }
    }
}

#[derive(Debug, Clone)]
pub struct Sampling {
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for Sampling {
    fn default() -> Self {
        Self { max_tokens: 260, temperature: 0.3 }
    }
}

#[derive(Clone)]
pub struct LlamaClient {
    base: String,
    agent: ureq::Agent,
    /// Random key the engine was started with, so other programs on the PC can't use it.
    key: Option<String>,
}

impl LlamaClient {
    pub fn new(port: u16) -> Self {
        Self::with_key(port, None)
    }

    pub fn with_key(port: u16, key: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .proxy(None) // local only: never send prompts through a proxy
            .timeout_connect(Some(Duration::from_secs(2)))
            .timeout_global(Some(Duration::from_secs(180)))
            .http_status_as_error(false)
            .build()
            .into();
        Self { base: format!("http://127.0.0.1:{port}"), agent, key }
    }

    pub fn is_ready(&self) -> bool {
        self.agent
            .get(format!("{}/health", self.base))
            .call()
            .map(|r| r.status().as_u16() == 200)
            .unwrap_or(false)
    }

    fn body(messages: &[ChatMessage], s: &Sampling, stream: bool) -> Value {
        // Settings tuned on Qwen3.5 small models: low temperature plus a presence penalty
        // stops them repeating themselves; thinking mode is off for fast, short answers.
        json!({
            "messages": messages,
            "stream": stream,
            "max_tokens": s.max_tokens,
            "temperature": s.temperature,
            "top_p": 0.8,
            "top_k": 20,
            "min_p": 0.0,
            "presence_penalty": 1.0,
            "repeat_penalty": 1.05,
            "chat_template_kwargs": { "enable_thinking": false },
        })
    }

    fn post(&self, body: &Value) -> Result<ureq::http::Response<ureq::Body>> {
        let mut req = self.agent.post(format!("{}/v1/chat/completions", self.base));
        if let Some(k) = &self.key {
            req = req.header("Authorization", &format!("Bearer {k}"));
        }
        let resp = req
            .send_json(body)
            .map_err(|e| Error::Other(format!("AI engine did not answer: {e}")))?;
        if resp.status().as_u16() != 200 {
            return Err(Error::Other(format!("AI engine error: HTTP {}", resp.status().as_u16())));
        }
        Ok(resp)
    }

    /// Whole answer at once.
    pub fn chat(&self, messages: &[ChatMessage], s: &Sampling) -> Result<String> {
        let mut resp = self.post(&Self::body(messages, s, false))?;
        let v: Value = resp.body_mut().read_json().map_err(|e| Error::Other(format!("bad AI reply: {e}")))?;
        Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string())
    }

    /// Stream the answer; `on_token` gets each piece and returns `false` to stop early.
    pub fn chat_stream(&self, messages: &[ChatMessage], s: &Sampling, on_token: &mut dyn FnMut(&str) -> bool) -> Result<String> {
        let resp = self.post(&Self::body(messages, s, true))?;
        let reader = BufReader::new(resp.into_body().into_reader());
        let mut full = String::new();
        for line in reader.lines() {
            let line = line.map_err(|e| Error::Other(format!("AI stream broke: {e}")))?;
            let Some(data) = line.strip_prefix("data: ") else { continue };
            if data.trim() == "[DONE]" {
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if let Some(piece) = v["choices"][0]["delta"]["content"].as_str() {
                full.push_str(piece);
                if !on_token(piece) {
                    break;
                }
            }
        }
        Ok(full)
    }

    /// Answer forced to match a JSON schema (grammar-constrained), so even tiny models
    /// return valid JSON.
    pub fn chat_json(&self, messages: &[ChatMessage], schema: &Value, max_tokens: u32) -> Result<Value> {
        let mut body = Self::body(messages, &Sampling { max_tokens, temperature: 0.0 }, false);
        body["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "answer", "schema": schema } });
        let mut resp = self.post(&body)?;
        let v: Value = resp.body_mut().read_json().map_err(|e| Error::Other(format!("bad AI reply: {e}")))?;
        let text = v["choices"][0]["message"]["content"].as_str().unwrap_or_default();
        serde_json::from_str(text).map_err(|e| Error::Other(format!("AI returned invalid JSON: {e}")))
    }
}
