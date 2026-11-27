# Cleaner rules

Cleaners are data, not code: `rules/cleaners/*.toml`, embedded at build time and validated by tests.

```toml
[[cleaner]]
id = "chrome.cache"              # namespaced, unique
name = "Google Chrome cache"
description = "Websites ki cached files. Login aur history safe rahenge."
category = "browser"             # system | browser | apps | games | developer | logs | recycle_bin | privacy
risk = "safe"                    # safe | ask | expert
running = ["chrome.exe"]         # skipped while any of these runs
needs_admin = false
privacy = false                  # browsing data: needs consent, never "safe"
default_enabled = true
inspect = false                  # flag files that look like user data (Temp folders)
note = "Shown before cleaning"   # optional

[[cleaner.target]]
path = "%LOCALAPPDATA%/Google/Chrome/User Data/*/Cache"   # %ENV% vars, * matches folder names
recursive = true
include = ["**/*"]               # globs relative to the target folder (empty = all)
exclude = []
min_age_hours = 0                # skip files changed more recently
remove_empty_dirs = false        # never removes the target folder itself

[[cleaner.target]]
kind = "recycle_bin"             # recycle-bin items older than the user's setting
```

Startup advice lives in `rules/startup/known.toml` (`keep` / `disable` with a short Hinglish note).
