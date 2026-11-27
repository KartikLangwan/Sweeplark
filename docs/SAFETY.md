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
projects and similar files are *flagged* and only deleted when the user ticks them one by one.
Unknown file types may get an AI second opinion. **The AI can only make things safer**: it can
mark any file risky, or clear an *unknown* type as junk, but it can never clear a file the rules
classify as user data.

## Right before deleting

Every file is checked again: still inside its folder, not a link, same size and modified time
as in the preview, and the related app (e.g. Chrome) is not running. Files in use are skipped.

## Every change is recorded

`ledger.db` stores each change. Startup changes store the exact previous registry value and can
be undone. Deleted cache does not come back; the app says so instead of promising an undo.

## Things we deliberately do not do

* No registry cleaner, no "RAM booster", no disabling Windows Defender, no deleting Prefetch.
* Secure delete is not offered for SSDs as a guarantee (wear levelling makes overwriting
  unreliable); BitLocker is the honest answer.
* "Efficiency mode" uses the documented Windows API (EcoQoS + idle priority); it lasts until the
  app exits and never closes apps.
