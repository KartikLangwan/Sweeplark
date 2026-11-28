# Safety rules

Sweeplark deletes files and changes Windows settings, so these rules are enforced in code
(`crates/sweeplark-core`) and covered by tests.

## What can run without asking

* Only cleaners marked `risk = "safe"` (caches, temp files, logs, thumbnails).
* Default approval mode is **ask always**: even safe automatic cleanups wait for the user's
  "Haan". The user can switch to **risky only**.
* Anything `ask` or `expert`, browsing data (`privacy`), and files that look like the user's
  own data always need an explicit approval.

## Files that are never touched

* Personal folders: Desktop, Documents, Downloads, Pictures, Videos, Music, Public, OneDrive,
  `source`/`repos` … (`safety.rs`, `SafetyGuard::for_current_user`).
* A cleaning target may live *inside* an important folder (e.g. `%LOCALAPPDATA%\Temp`) but may
  never be the folder itself, a parent of it, a drive root, `..` paths or relative paths.
* Symlinks and junctions are never followed or deleted.
* User whitelist (Settings → "Kabhi na chhuein").

## Files that look like user data

Temp folders are inspected (`inspect = true`): documents, photos, videos, archives, installers,
projects and similar files are *flagged* and only removed when the user ticks them one by one.
Ticked files go to the **Recycle Bin** (never deleted for good); on drives without a Recycle Bin
(USB, network) they are left alone.
Unknown file types may get an AI second opinion. **The AI can only make things safer**: it can
mark any file risky, or clear an *unknown* type as junk, but it can never clear a file the rules
classify as user data.

## Right before deleting

Every file is checked again: still inside its folder, not a link, same size and modified time
as in the preview, and the related app (e.g. Chrome) is not running. Files in use are skipped.

## Every change is recorded

`ledger.db` stores each change. Startup changes, Optimizer tweaks, Boost (old priority of every
calmed process, old power plan) and Efficiency mode store how to undo them. Deleted cache does not
come back; the app says so instead of promising an undo. Before the first system-level Optimizer
change of a session a Windows restore point is created (when running as administrator).

## Boost, Game mode and RAM guard

Never touched: the app you were using (the window behind Sweeplark when you press Boost, or the
full-screen game), its child processes, Windows processes, Sweeplark and everything it started
(its WebView and AI engine), the user's "never touch" list, and a built-in list of apps that break
when slowed: voice/video calls, streaming and recording, music players, anti-cheat, security
software, hardware/driver helpers and remote desktop tools. RAM trimming uses `EmptyWorkingSet`;
Windows pages memory back in when an app needs it.

## Storage and Apps

Removing anything from Storage or an app's leftovers always goes to the Recycle Bin. Windows,
Program Files, ProgramData and whole profile folders (home, Documents, Desktop, AppData ...) are
refused, and so are drives without a Recycle Bin. Leftovers are only looked for after the app's
own uninstaller has removed it, and only folders named exactly like the app or its publisher are
offered. Duplicates are compared by content (SHA-256), and at least one copy is always kept.

## Things we deliberately do not do

* No registry cleaner, no "RAM booster", no disabling Windows Defender, no deleting Prefetch.
* Secure delete is not offered for SSDs as a guarantee (wear levelling makes overwriting
  unreliable); BitLocker is the honest answer.
* "Efficiency mode" uses the documented Windows API (EcoQoS + idle priority); it lasts until the
  app exits and never closes apps.
