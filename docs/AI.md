# Offline AI

Sweeplark's assistant runs fully on the user's PC with [llama.cpp](https://github.com/ggml-org/llama.cpp)
(`llama-server`, MIT) and small open models from Hugging Face. Nothing is sent to the internet;
the only network use is the one-time download of the engine and the chosen model.

## Model tiers (`rules/ai/models.toml`)

| Tier | Model | Download | For |
|---|---|---|---|
| Lite | Qwen3.5 0.8B Q4_K_M | 533 MB | 4–6 GB RAM |
| Balanced | Qwen3.5 2B Q4_K_M | 1.28 GB | 8 GB RAM (default for most PCs) |
| High | Qwen3.5 4B Q4_K_M | 2.74 GB | 16 GB RAM or a graphics card |

All Apache-2.0. Files are pinned to a Hugging Face commit and verified with SHA-256. The engine
zip comes from a pinned llama.cpp release and is verified with the SHA-256 digest GitHub
publishes for the asset. The Vulkan build is used when a dedicated GPU is found, otherwise CPU.

## Loaded only when needed

The model process starts for a chat, a daily/weekly summary or a file review, then stops:

* background ticks unload it at the end of the tick;
* chat unloads it after `unload_after_secs` idle (default 2 min);
* only one model is ever loaded; it runs at below-normal priority on all cores but one.

## Why it works on tiny models (no tool calling needed)

1. **Router-first tools.** `ai/router.rs` picks tools from the question with keyword rules
   (English, Hinglish, Hindi). `ai/facts.rs` runs them and writes a few lines of measured facts.
   The model only has to *read*.
2. **Grammar-constrained JSON** (`response_format: json_schema`) for the bigger tiers to pick extra
   tools on vague questions, and for file reviews on every tier. Output is always valid JSON.
3. **Guard** (`ai/guard.rs`): strips `<think>`, cuts repetition, rejects answers with numbers that
   are not in the facts, forbidden advice (registry cleaners, disabling Defender …) or claims of
   having changed something. Rejected answers fall back to the app's own rule-based answer.
4. **Buttons come from the app**, never from the model.

The assistant introduces itself with `branding::APP_NAME`.

## Tested

`crates/sweeplark-core/tests/live_model.rs` runs the real engine and model end to end:

```
SWEEPLARK_LLAMA_SERVER=/path/llama-server SWEEPLARK_TEST_MODEL=/path/model.gguf \
  cargo test -p sweeplark-core --test live_model -- --ignored --nocapture
```

On a 4-core CPU: Lite loads in ~1.3 s and answers at ~25 tokens/s; Balanced loads in ~1.8 s,
~13 tokens/s. Both classify an invoice PDF and a WhatsApp photo in Temp as risky and Edge's temp
data as safe.
