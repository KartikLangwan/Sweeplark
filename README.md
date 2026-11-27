<p align="center">
  <img src="assets/banner.png" alt="Sweeplark — Small bird. Clean PC." width="820">
</p>

<p align="center">
  <b>A lightweight PC cleaner, optimizer and health monitor for Windows — with an offline AI assistant.</b><br>
  Safe work happens by itself, risky work asks first, and every change is recorded.
</p>

<p align="center">
  <img alt="Windows 10/11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0FB5AE">
  <img alt="Version" src="https://img.shields.io/badge/version-1.1-0B1B33">
  <img alt="Offline" src="https://img.shields.io/badge/AI-100%25%20offline-7DE8C8">
  <img alt="License" src="https://img.shields.io/badge/license-GPL--3.0-5B6B80">
</p>

---

## Why Sweeplark

| | |
|---|---|
| 🪶 **Light** | ~8 MB app, Rust core, no background services. The monitor samples every few minutes at ~0% CPU. |
| 🛡️ **Safe by design** | Preview before every action, personal folders are never touched, files that look like *your* data always need your tick. |
| 🧾 **Honest** | Every change goes into a ledger. Startup changes can be undone. No fake "RAM boost", no registry cleaner. |
| 🤖 **Offline AI** | A small local model (Lite / Balanced / High) explains your PC, writes daily reports and double-checks unclear files. Loaded only for seconds, then unloaded. |
| 🌐 **English & Hinglish** | English by default; Hinglish can be picked in Settings. |
| 🚫 **No tracking** | No ads, no account, no telemetry. |

## Features (v1.1)

- **One-Click Optimize** — scan junk, startup, storage, health and security; show the plan; fix only the safe part; report what changed.
- **Cleaner** — 40+ rules: Windows temp, thumbnails, crash dumps, error reports, Windows Update leftovers, Recycle Bin (older than N days), Chrome / Edge / Firefox / Brave / Opera / Vivaldi caches, Discord / Teams / Slack, Steam / Epic / Battle.net, GPU shader caches, npm / pip / Gradle / Cargo / Go caches, and opt-in browser history & cookies.
- **Startup manager** — turn apps on/off the same way Task Manager does, with advice for common apps and one-click undo.
- **Health** — CPU, RAM, disks, heavy apps, GPU, temperatures, battery wear, disk health, Defender & firewall status, Windows *Efficiency mode* for background hogs.
- **Background agent** — starts with Windows in the tray: smart alerts (sustained high RAM/CPU, low disk, new startup app, protection off), scheduled auto-clean with an approval gate, daily and weekly reports, quiet hours and a daily notification limit.
- **Assistant** — ask "Why is my PC slow?" and get an answer grounded in measured facts, with buttons to act.

## How the approval gate works

1. Auto-clean scans with the same rules as manual cleaning.
2. Files the rules are unsure about can get an AI second opinion. **The AI can only make things safer** — it can mark a file risky, never clear a file the rules see as yours.
3. Default mode **Ask always**: nothing is deleted until you approve (notification with a sound). Optional mode **Risky only**: cache/temp is cleaned automatically, the rest waits for you.

More in [`docs/SAFETY.md`](docs/SAFETY.md).

## Offline AI

| Tier | Model | Download | For |
|---|---|---|---|
| Lite | Qwen3.5 0.8B | 533 MB | 4–6 GB RAM |
| Balanced | Qwen3.5 2B | 1.28 GB | 8 GB RAM |
| High | Qwen3.5 4B | 2.74 GB | 16 GB RAM or a graphics card |

Runs on [llama.cpp](https://github.com/ggml-org/llama.cpp). Models are pinned and SHA-256 verified. Small models don't need tool calling: the app picks and runs the tools, the model only reads short facts, and a guard rejects made-up numbers. Details in [`docs/AI.md`](docs/AI.md).

## Project layout

```
crates/sweeplark-core   Rust engine: cleaner, startup, health, ledger, agent, AI
crates/sweeplark-cli    `sweeplark` command line
app/                    Desktop app: Tauri 2 (Rust) + Svelte 5 UI
rules/                  Cleaner rules, startup advice, AI model catalog (TOML data)
assets/brand            Logo, icon and brand kit
docs/                   Safety, AI and rule-format docs
```

## Build

Requirements: Windows 10/11, Rust (stable), Node.js 22.

```bash
cd app
npm ci
npx tauri build
```

The installer is written to `target/release/bundle/nsis/`. CI builds it on every push (`.github/workflows/ci.yml`).

Run the core tests (any OS):

```bash
cargo test
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
