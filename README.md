<p align="center">
  <img src="assets/banner.png" alt="Sweeplark — Small bird. Clean PC." width="820">
</p>

<p align="center">
  <b>A lightweight PC cleaner, optimizer and health monitor for Windows — with an offline AI assistant.</b><br>
  Safe work happens by itself, risky work asks first, and every change is recorded.
</p>

<p align="center">
  <img alt="Windows 10/11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0FB5AE">
  <img alt="Version" src="https://img.shields.io/badge/version-1.4-0B1B33">
  <img alt="Offline" src="https://img.shields.io/badge/AI-100%25%20offline-7DE8C8">
  <img alt="License" src="https://img.shields.io/badge/license-GPL--3.0-5B6B80">
</p>

---

## Download

Get the latest version from **[Releases](../../releases)**:

| File | For |
|---|---|
| `Sweeplark-x.y.z-Setup.exe` | **Recommended.** ~4 MB installer, no admin needed. Starts with Windows in the tray. Data in `%LOCALAPPDATA%\Sweeplark`, settings in `%APPDATA%\Sweeplark`. |
| `Sweeplark-x.y.z-Portable.exe` | Single exe, no install. Keeps everything in a `SweeplarkData` folder next to it. |

The app checks GitHub for new versions at startup and asks before updating.
Not code-signed yet: if SmartScreen appears, click **More info → Run anyway**.

## Why Sweeplark

| | |
|---|---|
| 🪶 **Light** | ~8 MB app, Rust core, no background services. The monitor samples every few minutes at ~0% CPU. |
| 🛡️ **Safe by design** | Preview before every action, personal folders are never touched, files that look like *your* data always need your tick. |
| 🧾 **Honest** | Every change goes into a ledger. Startup changes can be undone. No fake "RAM boost", no registry cleaner. |
| 🤖 **Offline AI** | A small local model (Lite / Balanced / High) explains your PC, writes daily reports and double-checks unclear files. Loaded only for seconds, then unloaded. |
| 🌐 **English & Hinglish** | English by default; Hinglish can be picked in Settings. |
| 🚫 **No tracking** | No ads, no account, no telemetry. |

## Features (v1.4)

- **Optimizer** — 100+ Windows tweaks with one-click *Apply recommended* and an off switch for every one: performance (background apps, foreground priority, multimedia scheduler, startup delay, NTFS), gaming (Game Mode, Game DVR, mouse acceleration, HAGS, fullscreen/windowed optimizations, MPO, network throttling), GPU driver tweaks for NVIDIA / AMD / Intel, power (Fast Startup, hibernation, Ultimate Performance), privacy (telemetry + services + tasks, ads, tips, activity history, location, Cortana/Bing search), Windows AI (Recall, Click to Do, Copilot, Edge/Paint/Notepad AI), services (legacy, Edge background, indexing, SysMain, printing, Xbox, Bluetooth, sharing, Remote Desktop) and Explorer / taskbar / system settings.
- **One-Click Optimize** — scan junk, startup, storage, health and security; fix only the safe part; report what changed.
- **Cleaner** — 40+ rules for Windows, browsers, apps, games, GPU shader caches and developer tools, with an approval gate for anything that looks like your files.
- **Monitor** — live CPU (per core, clock, temperature), GPU usage / VRAM (all vendors) and temperature (NVIDIA), memory, network and disk speed; process list with End and Eco (Efficiency mode).
- **Storage analyzer** — biggest folders and files and space by file type; remove to the Recycle Bin.
- **Startup manager** — Task Manager-style on/off with advice and undo.
- **Health** — disks and disk health, battery wear, Defender and firewall status.
- **Tools** — SFC, DISM, disk check, Windows Update repair, Explorer restart, icon cache rebuild, DNS flush, network reset, power plans, shutdown timer, hardware details (motherboard, BIOS, RAM sticks, GPU driver) and shortcuts to Windows tools.
- **Tray** — starts with Windows; hover shows CPU/GPU usage and temperature, RAM and free disk.
- **Background agent** — smart alerts, scheduled auto-clean with approval, daily and weekly reports.
- **Offline AI assistant** — answers from measured facts; three model sizes.
- **Updates** — checks GitHub Releases at startup and asks before updating.

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

## Developer

Made by **[Kartik Langwan](https://kartiklangwan.runs-on.dev)** — independent developer, Delhi, India.
I make websites, tools, apps and Discord bots that feel good to use.

## Credits

Optimizer tweak values follow Microsoft documentation and the open-source
[optimizerDuck](https://github.com/itsfatduck/optimizerDuck) project (GPL-3.0).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
