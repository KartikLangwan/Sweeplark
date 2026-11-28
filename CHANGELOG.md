# Changelog

## 1.4.1

**Faster, lighter, safer.** A full audit of 1.4.0 with every finding fixed.

New
- **Apps page:** uninstall programs with their own uninstaller, then remove the folders they leave behind (to the Recycle Bin). Shows app updates from winget, one by one or all at once.
- **Duplicate finder** in Storage: finds files with exactly the same content (not just the same name). The oldest copy is kept, and at least one copy of every file always stays.
- **Background jobs:** scans, storage analysis, duplicates, cleaning, Boost and AI downloads keep running when you open another page. A progress bar with percentage and time left is shown everywhere, plus tips while you wait.
- **Game mode:** when a game runs full screen, heavy background apps are calmed, and put back when the game closes (off by default).
- **RAM guard:** frees RAM from background apps by itself when memory stays nearly full (off by default).
- **Deep RAM clean** (administrator): writes pending changes to disk and clears the standby list.
- **Restore point** before the first system-level Optimizer change of a session, plus a "Create restore point" tool.
- **Startup** now also lists scheduled tasks that start apps at sign-in.
- **Assistant** can now see Optimizer tweaks, the graphics card, background apps and installed apps, and gives buttons to open the right page. The chat stays when you switch pages.
- More cleaners: new Microsoft Teams, Spotify, Zoom, OneDrive logs, Adobe media cache, Brave shader cache. New tool: clean old Windows update files (DISM).
- "Restart as admin" banner wherever something needs administrator rights; Sweeplark comes back to the same page.
- Settings: export/import, "Never touch these apps" list, open logs, report a problem (prefilled GitHub issue, user name hidden).

Fixed
- AI model and update downloads no longer fail after 60 seconds on slower connections.
- Boost no longer slows down the app you were using (it used to treat Sweeplark's own window as "the app in front").
- Unticking all apps in Boost no longer boosts every app.
- Boost never touches Sweeplark's own WebView, voice chat, streaming, anti-cheat, security or driver helpers.
- Boost and Efficiency mode can be undone (old priority and power plan come back); Efficiency mode off hands control back to Windows.
- "Fix Windows Update" no longer leaves update services stopped when run twice.
- Ending an app asks first, and can never end Sweeplark's own window.
- Quitting while the AI is answering no longer freezes; the AI engine always closes with Sweeplark, even after a crash.
- The window no longer freezes during undo, settings changes or live stats (all run in the background now).
- A damaged settings file is kept as a backup instead of silently losing your whitelist.
- Files that look like yours, when you approve deleting them, go to the Recycle Bin.
- Storage refuses to remove Windows, program or whole profile folders, and drives without a Recycle Bin.
- CPU temperature: the motherboard sensor is no longer shown as the CPU temperature. The real CPU temperature is read from LibreHardwareMonitor when it runs.
- Scheduled task state is read in a way that works on every Windows language.
- Update check retries after 30 minutes when the PC was offline at start-up.
- Optimizer page loads faster; Home and Health refresh with the light live monitor.
- Windows tools are started by full path from System32.

Lighter
- Closing the window frees the WebView (~100 MB); Sweeplark keeps running in the tray. Starting with Windows creates no window at all.
- The tray reads CPU/RAM/GPU only while your mouse is over the icon.
- The local AI engine is started with a private key, so other programs can't use it.

Changed
- "Recommended" no longer includes Fast Startup off, background apps off or Edge AI policies (each has side effects; still available).
- Releases run all tests and the UI type check before building.

## 1.4.0

First public release: cleaner, Boost, Optimizer (104 tweaks), Monitor, Storage, Startup, Health, Tools, offline AI assistant, background agent, tray, updates.
