use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Serialize;
use walkdir::WalkDir;

use super::classify::{FileClass, classify};
use super::recycle::{self, RecycleEntry};
use super::{Category, CleanerDef, RuleSet, Target, TargetKind};
use crate::ledger::{ChangeStatus, Ledger, NewChange};
use crate::paths::expand_vars;
use crate::platform::processes::ProcessList;
use crate::safety::SafetyGuard;
use crate::settings::Settings;
use crate::util::{age_hours, format_bytes};
use crate::{Result, Risk};

type EnvLookup = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Everything the engine needs to know about the machine. Built once per scan.
pub struct ScanContext {
    pub guard: SafetyGuard,
    pub processes: ProcessList,
    pub is_admin: bool,
    pub now: SystemTime,
    pub os: String,
    pub env: EnvLookup,
    pub recycle_dirs: Vec<PathBuf>,
    pub recycle_min_age_hours: u64,
    /// Language for cleaner names and descriptions in the preview.
    pub lang: crate::Lang,
}

impl ScanContext {
    pub fn system(settings: &Settings) -> Self {
        Self {
            guard: SafetyGuard::for_current_user(settings.whitelist.clone()),
            processes: ProcessList::current(),
            is_admin: crate::platform::system::is_admin(),
            now: SystemTime::now(),
            os: std::env::consts::OS.to_string(),
            env: Box::new(crate::paths::env_lookup),
            recycle_dirs: recycle::current_user_recycle_dirs(),
            recycle_min_age_hours: u64::from(settings.recycle_bin_min_age_days) * 24,
            lang: settings.language,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_risk: Risk,
    /// User agreed to browsing-data cleaners.
    pub include_privacy: bool,
    /// Only these cleaner ids (ignores `default_enabled`).
    pub only: Option<Vec<String>>,
    /// Cleaner ids turned off / on by the user (on = overrides `default_enabled = false`).
    pub disabled: Vec<String>,
    pub enabled: Vec<String>,
}

impl ScanOptions {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            max_risk: s.trust_level.max_risk(),
            include_privacy: s.privacy_consent,
            only: None,
            disabled: s.disabled_cleaners.clone(),
            enabled: s.enabled_cleaners.clone(),
        }
    }

    /// Only safe cleaners that are on by default: what One-Click may run by itself.
    pub fn safe_only(s: &Settings) -> Self {
        Self { max_risk: Risk::Safe, include_privacy: false, ..Self::from_settings(s) }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreviewStatus {
    Ready,
    Empty,
    AppRunning { apps: Vec<String> },
    NeedsAdmin,
}

#[derive(Debug, Clone)]
enum EntryKind {
    File,
    Recycle { r_path: PathBuf },
}

#[derive(Debug, Clone)]
struct PlannedEntry {
    path: PathBuf,
    base: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
    kind: EntryKind,
    class: FileClass,
    /// Short reason from the AI review, if it looked at this file.
    review: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleFile {
    pub path: String,
    pub bytes: u64,
}

/// A file kept back for the user's approval.
#[derive(Debug, Clone, Serialize)]
pub struct FlaggedFile {
    pub path: String,
    pub bytes: u64,
    /// "document", "photo", ... or "unknown".
    pub kind: String,
    /// Why the AI thought it is risky (when the AI reviewed it).
    pub review: Option<String>,
}

/// The preview for one cleaner: what *would* be deleted.
#[derive(Debug, Clone, Serialize)]
pub struct CleanerPreview {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    pub risk: Risk,
    pub needs_admin: bool,
    pub privacy: bool,
    pub note: Option<String>,
    pub status: PreviewStatus,
    pub files: u64,
    pub bytes: u64,
    /// Biggest files, so the user can see what goes.
    pub sample: Vec<SampleFile>,
    /// Folders that were refused by the safety guard (shown for transparency).
    pub blocked: Vec<String>,
    /// Files that look like the user's own data: only deleted after approval.
    pub flagged: Vec<FlaggedFile>,
    pub flagged_bytes: u64,
    #[serde(skip)]
    entries: Vec<PlannedEntry>,
    #[serde(skip)]
    empty_dir_bases: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub cleaners: Vec<CleanerPreview>,
    pub total_files: u64,
    pub total_bytes: u64,
    pub duration_ms: u64,
}

impl ScanReport {
    pub fn get(&self, id: &str) -> Option<&CleanerPreview> {
        self.cleaners.iter().find(|c| c.id == id)
    }

    /// Files whose type is unknown, for an AI second opinion: (cleaner id, path, bytes).
    pub fn unknown_files(&self) -> Vec<(String, PathBuf, u64)> {
        self.cleaners
            .iter()
            .flat_map(|c| {
                c.entries
                    .iter()
                    .filter(|e| e.class == FileClass::Unknown && e.review.is_none())
                    .map(move |e| (c.id.clone(), e.path.clone(), e.size))
            })
            .collect()
    }

    /// Record an AI verdict. The AI may clear an *unknown* file as junk, or mark any file
    /// as risky. It can never clear a file the rules see as the user's data.
    pub fn apply_review(&mut self, path: &Path, risky: bool, reason: &str) {
        for c in &mut self.cleaners {
            if let Some(e) = c.entries.iter_mut().find(|e| e.path == path) {
                e.review = Some(reason.chars().take(120).collect());
                match (risky, e.class) {
                    (false, FileClass::Unknown) => e.class = FileClass::Junk,
                    (true, FileClass::Junk) | (true, FileClass::Unknown) => e.class = FileClass::UserData("ai: risky"),
                    _ => {}
                }
                c.refresh_flags();
            }
        }
    }

    pub fn flagged_count(&self) -> usize {
        self.cleaners.iter().map(|c| c.flagged.len()).sum()
    }

    pub fn ready_bytes(&self, max_risk: Risk) -> u64 {
        self.cleaners
            .iter()
            .filter(|c| matches!(c.status, PreviewStatus::Ready) && c.risk <= max_risk)
            .map(|c| c.bytes - c.flagged_bytes)
            .sum()
    }
}

const SAMPLE_SIZE: usize = 25;
const FLAGGED_LIST_MAX: usize = 200;

impl CleanerPreview {
    /// Drop every planned file except `paths` (used when only some files were approved).
    pub fn keep_only(&mut self, paths: &[PathBuf]) {
        self.entries.retain(|e| paths.contains(&e.path));
        self.files = self.entries.len() as u64;
        self.bytes = self.entries.iter().map(|e| e.size).sum();
        self.refresh_flags();
    }

    /// Number of files kept back for approval (not only the first ones listed).
    pub fn flagged_count(&self) -> u64 {
        self.entries.iter().filter(|e| e.class != FileClass::Junk).count() as u64
    }

    fn refresh_flags(&mut self) {
        let flagged: Vec<&PlannedEntry> = self.entries.iter().filter(|e| e.class != FileClass::Junk).collect();
        self.flagged_bytes = flagged.iter().map(|e| e.size).sum();
        self.flagged = flagged
            .into_iter()
            .take(FLAGGED_LIST_MAX)
            .map(|e| FlaggedFile {
                path: e.path.display().to_string(),
                bytes: e.size,
                kind: match e.class {
                    FileClass::UserData(k) => k.to_string(),
                    _ => "unknown".to_string(),
                },
                review: e.review.clone(),
            })
            .collect();
    }
}

fn has_glob(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

fn build_globset(globs: &[String]) -> Option<GlobSet> {
    if globs.is_empty() {
        return None;
    }
    let mut b = GlobSetBuilder::new();
    for g in globs {
        if let Ok(glob) = GlobBuilder::new(g).case_insensitive(cfg!(windows)).literal_separator(false).build() {
            b.add(glob);
        }
    }
    b.build().ok()
}

/// Resolve a target path (env vars + folder wildcards) to existing folders.
fn resolve_bases(target: &Target, ctx: &ScanContext) -> Vec<PathBuf> {
    let lookup = |name: &str| (ctx.env)(name).map(|v| if has_glob(&v) { glob::Pattern::escape(&v) } else { v });
    let Some(expanded) = expand_vars(&target.path, &lookup) else { return Vec::new() };
    let candidates: Vec<PathBuf> = if has_glob(&expanded) {
        let opts = glob::MatchOptions { case_sensitive: !cfg!(windows), ..Default::default() };
        glob::glob_with(&expanded, opts).map(|it| it.flatten().collect()).unwrap_or_default()
    } else {
        vec![PathBuf::from(expanded)]
    };
    candidates
        .into_iter()
        .filter(|p| std::fs::symlink_metadata(p).map(|m| m.is_dir() && !m.file_type().is_symlink()).unwrap_or(false))
        .map(|p| long_path(&p))
        .collect()
}

/// Full long-form path (no 8.3 short names like `JOHNSM~1`), so the safety guard compares
/// like with like. `\\?\C:\...` from canonicalize becomes `C:\...`; network paths stay as given.
fn long_path(p: &Path) -> PathBuf {
    match std::fs::canonicalize(p) {
        Ok(c) => {
            let s = c.to_string_lossy();
            match s.strip_prefix(r"\\?\") {
                Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
                Some(_) => p.to_path_buf(),
                None => c,
            }
        }
        Err(_) => p.to_path_buf(),
    }
}

/// No folder between `base` and `file` may be a link or junction (it could have been
/// swapped in after the preview to point somewhere else).
fn no_links_between(base: &Path, file: &Path) -> bool {
    let mut cur = file.parent();
    while let Some(dir) = cur {
        if dir == base || !dir.starts_with(base) {
            return dir == base;
        }
        match std::fs::symlink_metadata(dir) {
            Ok(m) if !m.file_type().is_symlink() => {}
            _ => return false,
        }
        cur = dir.parent();
    }
    false
}

fn scan_files_target(
    target: &Target,
    inspect: bool,
    ctx: &ScanContext,
    seen: &mut HashSet<PathBuf>,
    preview: &mut CleanerPreview,
) {
    let include = build_globset(&target.include);
    let exclude = build_globset(&target.exclude);
    for base in resolve_bases(target, ctx) {
        if let Err(e) = ctx.guard.check_base(&base) {
            preview.blocked.push(e.to_string());
            continue;
        }
        if target.remove_empty_dirs {
            preview.empty_dir_bases.push(base.clone());
        }
        let walker = WalkDir::new(&base)
            .follow_links(false)
            .min_depth(1)
            .max_depth(if target.recursive { usize::MAX } else { 1 });
        for entry in walker.into_iter().flatten() {
            let ft = entry.file_type();
            if ft.is_symlink() || !ft.is_file() {
                continue;
            }
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(&base) else { continue };
            if include.as_ref().is_some_and(|g| !g.is_match(rel)) {
                continue;
            }
            if exclude.as_ref().is_some_and(|g| g.is_match(rel)) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let modified = meta.modified().ok();
            if target.min_age_hours > 0 {
                match modified {
                    Some(m) if age_hours(m, ctx.now) >= target.min_age_hours => {}
                    _ => continue,
                }
            }
            if ctx.guard.check_file(&base, path).is_err() {
                continue;
            }
            if !seen.insert(path.to_path_buf()) {
                continue;
            }
            preview.entries.push(PlannedEntry {
                path: path.to_path_buf(),
                base: base.clone(),
                size: meta.len(),
                modified,
                kind: EntryKind::File,
                class: if inspect { classify(path) } else { FileClass::Junk },
                review: None,
            });
        }
    }
}

fn scan_recycle_target(target: &Target, ctx: &ScanContext, preview: &mut CleanerPreview) {
    let min_age = ctx.recycle_min_age_hours.max(target.min_age_hours);
    for dir in &ctx.recycle_dirs {
        if let Err(e) = ctx.guard.check_base(dir) {
            preview.blocked.push(e.to_string());
            continue;
        }
        for RecycleEntry { i_path, r_path, size, deleted_at, .. } in recycle::scan_recycle_dir(dir) {
            let Some(deleted_at) = deleted_at else { continue };
            if age_hours(deleted_at, ctx.now) < min_age {
                continue;
            }
            preview.entries.push(PlannedEntry {
                path: i_path,
                base: dir.clone(),
                size,
                modified: None,
                kind: EntryKind::Recycle { r_path },
                // The whole recycle-bin cleaner already needs confirmation.
                class: FileClass::Junk,
                review: None,
            });
        }
    }
}

fn wanted(def: &CleanerDef, opts: &ScanOptions) -> bool {
    if def.risk > opts.max_risk || (def.privacy && !opts.include_privacy) {
        return false;
    }
    match &opts.only {
        Some(ids) => ids.iter().any(|i| i == &def.id),
        None => {
            !opts.disabled.contains(&def.id) && (def.default_enabled || opts.enabled.contains(&def.id))
        }
    }
}

/// Preview one cleaner.
pub fn scan_cleaner(def: &CleanerDef, ctx: &ScanContext) -> CleanerPreview {
    let mut preview = CleanerPreview {
        id: def.id.clone(),
        name: ctx.lang.pick_opt(&def.name, def.name_hi.as_deref()).to_string(),
        description: ctx.lang.pick_opt(&def.description, def.description_hi.as_deref()).to_string(),
        category: def.category,
        risk: def.risk,
        needs_admin: def.needs_admin,
        privacy: def.privacy,
        note: def.note.as_deref().map(|n| ctx.lang.pick_opt(n, def.note_hi.as_deref()).to_string()),
        status: PreviewStatus::Empty,
        files: 0,
        bytes: 0,
        sample: Vec::new(),
        blocked: Vec::new(),
        flagged: Vec::new(),
        flagged_bytes: 0,
        entries: Vec::new(),
        empty_dir_bases: Vec::new(),
    };
    if def.needs_admin && !ctx.is_admin {
        preview.status = PreviewStatus::NeedsAdmin;
        return preview;
    }
    let mut seen = HashSet::new();
    for target in &def.targets {
        match target.kind {
            TargetKind::Files => scan_files_target(target, def.inspect, ctx, &mut seen, &mut preview),
            TargetKind::RecycleBin => scan_recycle_target(target, ctx, &mut preview),
        }
    }
    preview.files = preview.entries.len() as u64;
    preview.bytes = preview.entries.iter().map(|e| e.size).sum();
    preview.refresh_flags();
    let mut biggest: Vec<&PlannedEntry> = preview.entries.iter().collect();
    biggest.sort_by_key(|e| std::cmp::Reverse(e.size));
    preview.sample = biggest
        .into_iter()
        .take(SAMPLE_SIZE)
        .map(|e| SampleFile {
            path: match &e.kind {
                EntryKind::File => e.path.display().to_string(),
                EntryKind::Recycle { r_path } => r_path.display().to_string(),
            },
            bytes: e.size,
        })
        .collect();
    let running: Vec<String> = ctx.processes.running(&def.running).into_iter().map(String::from).collect();
    preview.status = if !running.is_empty() && preview.files > 0 {
        PreviewStatus::AppRunning { apps: running }
    } else if preview.files > 0 {
        PreviewStatus::Ready
    } else {
        PreviewStatus::Empty
    };
    preview
}

/// Preview every wanted cleaner. Changes nothing on disk.
pub fn scan(rules: &RuleSet, ctx: &ScanContext, opts: &ScanOptions) -> ScanReport {
    scan_with(rules, ctx, opts, &mut |_, _, _| {})
}

/// [`scan`] with progress: (cleaners done, cleaners total, name of the one being looked at).
pub fn scan_with(rules: &RuleSet, ctx: &ScanContext, opts: &ScanOptions, progress: &mut dyn FnMut(u64, u64, &str)) -> ScanReport {
    let started = Instant::now();
    let defs: Vec<&CleanerDef> = rules.for_os(&ctx.os).filter(|d| wanted(d, opts)).collect();
    let n = defs.len() as u64;
    let cleaners: Vec<CleanerPreview> = defs
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            progress(i as u64, n, ctx.lang.pick_opt(&d.name, d.name_hi.as_deref()));
            scan_cleaner(d, ctx)
        })
        .collect();
    progress(n, n, "");
    ScanReport {
        // Only what cleaning removes by itself: files that look like the user's wait for a tick.
        total_files: cleaners.iter().filter(|c| matches!(c.status, PreviewStatus::Ready)).map(|c| c.files - c.flagged_count()).sum(),
        total_bytes: cleaners.iter().filter(|c| matches!(c.status, PreviewStatus::Ready)).map(|c| c.bytes - c.flagged_bytes).sum(),
        duration_ms: started.elapsed().as_millis() as u64,
        cleaners,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CleanOutcome {
    pub id: String,
    pub name: String,
    pub deleted_files: u64,
    pub freed_bytes: u64,
    /// Locked by a running program; left alone.
    pub in_use: u64,
    /// Changed since the preview; left alone to be safe.
    pub changed: u64,
    /// Looked like the user's own files and were not approved; left alone.
    pub kept_for_approval: u64,
    /// Approved files of the user that went to the Recycle Bin (can be restored).
    pub recycled: u64,
    pub skipped_reason: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CleanReport {
    pub outcomes: Vec<CleanOutcome>,
    pub freed_bytes: u64,
    pub deleted_files: u64,
    pub ledger_id: Option<i64>,
}

enum DeleteResult {
    Deleted(u64),
    InUse,
    Changed,
    Gone,
    Error(String),
}

fn is_in_use(e: &std::io::Error) -> bool {
    // 32 = ERROR_SHARING_VIOLATION, 33 = ERROR_LOCK_VIOLATION
    matches!(e.raw_os_error(), Some(32) | Some(33)) || e.kind() == std::io::ErrorKind::ResourceBusy
}

fn remove_file_clearing_readonly(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let meta = std::fs::symlink_metadata(path)?;
            let mut perms = meta.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                std::fs::set_permissions(path, perms)?;
                std::fs::remove_file(path)
            } else {
                Err(e)
            }
        }
        other => other,
    }
}

fn delete_entry(e: &PlannedEntry, guard: &SafetyGuard) -> DeleteResult {
    if guard.check_file(&e.base, &e.path).is_err() || !no_links_between(&e.base, &e.path) {
        return DeleteResult::Changed;
    }
    let meta = match std::fs::symlink_metadata(&e.path) {
        Ok(m) => m,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return DeleteResult::Gone,
        Err(err) => return DeleteResult::Error(err.to_string()),
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return DeleteResult::Changed;
    }
    match &e.kind {
        EntryKind::File => {
            if meta.len() != e.size || (e.modified.is_some() && meta.modified().ok() != e.modified) {
                return DeleteResult::Changed;
            }
            // Files that look like the user's own (approved one by one) go to the Recycle Bin,
            // so a wrong tick can be undone. Junk is deleted for good.
            // Files only the AI called junk (the rules were unsure) are recycled as well.
            let recycle = e.class != FileClass::Junk || e.review.is_some();
            if recycle && crate::platform::system::has_recycle_bin(&e.path) {
                return match crate::platform::system::to_recycle_bin(&e.path) {
                    Ok(()) => DeleteResult::Deleted(e.size),
                    Err(err) => DeleteResult::Error(format!("{}: {err}", e.path.display())),
                };
            }
            if e.class != FileClass::Junk && cfg!(windows) {
                // No Recycle Bin on this drive (USB, network): never delete the user's file for good.
                return DeleteResult::Error(format!("{}: no Recycle Bin on this drive, left alone", e.path.display()));
            }
            match remove_file_clearing_readonly(&e.path) {
                Ok(()) => DeleteResult::Deleted(e.size),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => DeleteResult::Gone,
                Err(err) if is_in_use(&err) || err.kind() == std::io::ErrorKind::PermissionDenied => {
                    DeleteResult::InUse
                }
                Err(err) => DeleteResult::Error(format!("{}: {err}", e.path.display())),
            }
        }
        EntryKind::Recycle { r_path } => {
            if guard.check_file(&e.base, r_path).is_err() {
                return DeleteResult::Changed;
            }
            let mut freed = e.size;
            let r_result = match std::fs::symlink_metadata(r_path) {
                Ok(m) if m.file_type().is_symlink() => return DeleteResult::Changed,
                // std's remove_dir_all never follows symlinks/junctions.
                Ok(m) if m.is_dir() => std::fs::remove_dir_all(r_path),
                Ok(_) => remove_file_clearing_readonly(r_path),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    freed = 0; // only the small $I record was left
                    Ok(())
                }
                Err(err) => Err(err),
            };
            match r_result.and_then(|()| remove_file_clearing_readonly(&e.path)) {
                Ok(()) => DeleteResult::Deleted(freed),
                Err(err) if is_in_use(&err) => DeleteResult::InUse,
                Err(err) => DeleteResult::Error(format!("{}: {err}", r_path.display())),
            }
        }
    }
}

fn remove_empty_dirs(base: &Path, guard: &SafetyGuard) {
    if guard.check_base(base).is_err() {
        return;
    }
    for entry in WalkDir::new(base).follow_links(false).min_depth(1).contents_first(true).into_iter().flatten() {
        let ft = entry.file_type();
        if ft.is_dir() && !ft.is_symlink() && guard.check_file(base, entry.path()).is_ok() {
            // Fails harmlessly when the folder is not empty.
            let _ = std::fs::remove_dir(entry.path());
        }
    }
}

/// Delete what a preview found, for the selected cleaner ids.
///
/// `confirmed` must be true for anything above [`Risk::Safe`]: the caller has shown the
/// preview and the user said yes. Flagged files (look like user data) are only deleted
/// when listed in `approved_flagged`. Running programs are checked again right before deleting.
#[allow(clippy::too_many_arguments)]
pub fn clean(
    report: &ScanReport,
    selected: &[String],
    confirmed: bool,
    approved_flagged: &[PathBuf],
    guard: &SafetyGuard,
    processes: &ProcessList,
    rules: &RuleSet,
    ledger: Option<&Ledger>,
) -> Result<CleanReport> {
    let mut out = CleanReport::default();
    let mut max_risk = Risk::Safe;
    let approved: HashSet<&PathBuf> = approved_flagged.iter().collect();
    for preview in report.cleaners.iter().filter(|p| selected.contains(&p.id)) {
        let mut outcome = CleanOutcome { id: preview.id.clone(), name: preview.name.clone(), ..Default::default() };
        if !matches!(preview.status, PreviewStatus::Ready) {
            outcome.skipped_reason = Some("nothing to clean or not ready".into());
            out.outcomes.push(outcome);
            continue;
        }
        if !preview.risk.may_auto_apply() && !confirmed {
            outcome.skipped_reason = Some("needs your confirmation".into());
            out.outcomes.push(outcome);
            continue;
        }
        if let Some(def) = rules.get(&preview.id) {
            let running = processes.running(&def.running);
            if !running.is_empty() {
                outcome.skipped_reason = Some(format!("close {} first", running.join(", ")));
                out.outcomes.push(outcome);
                continue;
            }
        }
        for entry in &preview.entries {
            if entry.class != FileClass::Junk && !approved.contains(&entry.path) {
                outcome.kept_for_approval += 1;
                continue;
            }
            match delete_entry(entry, guard) {
                DeleteResult::Deleted(bytes) => {
                    if entry.class != FileClass::Junk {
                        outcome.recycled += 1;
                    }
                    outcome.deleted_files += 1;
                    outcome.freed_bytes += bytes;
                }
                DeleteResult::InUse => outcome.in_use += 1,
                DeleteResult::Changed => outcome.changed += 1,
                DeleteResult::Gone => {}
                DeleteResult::Error(msg) => {
                    if outcome.errors.len() < 10 {
                        outcome.errors.push(msg);
                    }
                }
            }
        }
        for base in &preview.empty_dir_bases {
            remove_empty_dirs(base, guard);
        }
        max_risk = max_risk.max(preview.risk);
        out.freed_bytes += outcome.freed_bytes;
        out.deleted_files += outcome.deleted_files;
        out.outcomes.push(outcome);
    }

    let cleaned: Vec<&CleanOutcome> = out.outcomes.iter().filter(|o| o.deleted_files > 0).collect();
    if let Some(ledger) = ledger
        && !cleaned.is_empty()
    {
        let had_problems = out.outcomes.iter().any(|o| !o.errors.is_empty());
        // Files are already gone: a ledger problem is logged, never turned into a failure
        // that hides what was cleaned.
        let recorded = ledger.record(&NewChange {
            category: "clean".into(),
            title: format!("Cleaned {} ({} files)", format_bytes(out.freed_bytes), out.deleted_files),
            risk: max_risk,
            status: if had_problems { ChangeStatus::Partial } else { ChangeStatus::Applied },
            bytes_freed: out.freed_bytes,
            items: out.deleted_files,
            details: serde_json::to_value(&cleaned).unwrap_or_default(),
            revert: Vec::new(),
        });
        match recorded {
            Ok(id) => out.ledger_id = Some(id),
            Err(e) => crate::log::warn(&format!("cleaned, but History could not record it: {e}")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use super::*;
    use crate::cleaner::recycle::make_i_file_v2;
    use crate::util::system_time_to_filetime;

    struct Fixture {
        dir: tempfile::TempDir,
        rules: RuleSet,
    }

    impl Fixture {
        fn new(rules_toml: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_string_lossy().replace('\\', "/");
            let mut rules = RuleSet::default();
            rules.add_file("t.toml", &rules_toml.replace("ROOT", &root)).unwrap();
            Self { dir, rules }
        }

        fn root(&self) -> &Path {
            self.dir.path()
        }

        fn ctx(&self) -> ScanContext {
            let root = self.root().to_path_buf();
            ScanContext {
                guard: SafetyGuard::new(
                    vec![root.join("home/Documents")],
                    vec![root.join("home"), root.join("home/AppData")],
                    vec![root.join("home/AppData/Temp/keep")],
                ),
                processes: ProcessList::default(),
                is_admin: false,
                now: SystemTime::now() + Duration::from_secs(48 * 3600),
                os: "windows".into(),
                env: Box::new(move |name| match name {
                    "LOCALAPPDATA" => Some(root.join("home/AppData").to_string_lossy().into_owned()),
                    _ => None,
                }),
                recycle_dirs: Vec::new(),
                recycle_min_age_hours: 0,
                lang: crate::Lang::En,
            }
        }

        fn write(&self, rel: &str, bytes: usize) -> PathBuf {
            let p = self.root().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, vec![b'x'; bytes]).unwrap();
            p
        }
    }

    const RULES: &str = r#"
[[cleaner]]
id = "t.temp"
name = "Temp"
description = "temp"
category = "system"
risk = "safe"
running = ["busy.exe"]
[[cleaner.target]]
path = "%LOCALAPPDATA%/Temp"
exclude = ["**/*.lock"]
remove_empty_dirs = true

[[cleaner]]
id = "t.browser"
name = "Browser"
description = "cache"
category = "browser"
risk = "safe"
[[cleaner.target]]
path = "%LOCALAPPDATA%/Browser/User Data/*/Cache"

[[cleaner]]
id = "t.cookies"
name = "Cookies"
description = "cookies"
category = "privacy"
risk = "ask"
privacy = true
default_enabled = false
[[cleaner.target]]
path = "%LOCALAPPDATA%/Browser/User Data/*/Network"
include = ["Cookies*"]

[[cleaner]]
id = "t.bad"
name = "Bad"
description = "points at the profile"
category = "system"
risk = "safe"
[[cleaner.target]]
path = "%LOCALAPPDATA%/../Documents"

[[cleaner]]
id = "t.docs"
name = "Docs"
description = "personal"
category = "system"
risk = "safe"
[[cleaner.target]]
path = "ROOT/home/Documents/cache"

[[cleaner]]
id = "t.admin"
name = "Admin"
description = "needs admin"
category = "system"
risk = "safe"
needs_admin = true
[[cleaner.target]]
path = "ROOT/win/Temp"
"#;

    fn opts() -> ScanOptions {
        ScanOptions { max_risk: Risk::Expert, include_privacy: false, only: None, disabled: vec![], enabled: vec![] }
    }

    #[test]
    fn preview_then_clean() {
        let f = Fixture::new(RULES);
        f.write("home/AppData/Temp/a.tmp", 100);
        f.write("home/AppData/Temp/sub/b.tmp", 50);
        f.write("home/AppData/Temp/app.lock", 10);
        f.write("home/AppData/Temp/keep/important.txt", 10);
        f.write("home/AppData/Browser/User Data/Default/Cache/data_0", 1000);
        f.write("home/AppData/Browser/User Data/Profile 1/Cache/data_1", 500);
        f.write("home/AppData/Browser/User Data/Default/Network/Cookies", 70);
        f.write("home/Documents/cache/thesis.docx", 999);

        let ctx = f.ctx();
        let report = scan(&f.rules, &ctx, &opts());
        let temp = report.get("t.temp").unwrap();
        assert_eq!(temp.files, 2, "lock file and whitelisted file are skipped");
        assert_eq!(temp.bytes, 150);
        assert_eq!(report.get("t.browser").unwrap().bytes, 1500);
        assert!(report.get("t.cookies").is_none(), "privacy needs consent");
        assert_eq!(report.get("t.bad").unwrap().files, 0, "'..' path is refused");
        let docs = report.get("t.docs").unwrap();
        assert_eq!(docs.files, 0);
        assert!(!docs.blocked.is_empty(), "personal folder shows up as blocked");
        assert!(matches!(report.get("t.admin").unwrap().status, PreviewStatus::NeedsAdmin));

        // Preview changed nothing.
        assert!(f.root().join("home/AppData/Temp/a.tmp").exists());

        let ledger = Ledger::open_in_memory().unwrap();
        let ids = vec!["t.temp".to_string(), "t.browser".to_string()];
        let res = clean(&report, &ids, false, &[], &ctx.guard, &ctx.processes, &f.rules, Some(&ledger)).unwrap();
        assert_eq!(res.freed_bytes, 1650);
        assert_eq!(res.deleted_files, 4);
        assert!(!f.root().join("home/AppData/Temp/a.tmp").exists());
        assert!(!f.root().join("home/AppData/Temp/sub").exists(), "empty sub folder removed");
        assert!(f.root().join("home/AppData/Temp").exists(), "target folder itself stays");
        assert!(f.root().join("home/AppData/Temp/app.lock").exists());
        assert!(f.root().join("home/AppData/Temp/keep/important.txt").exists());
        assert!(f.root().join("home/Documents/cache/thesis.docx").exists());
        let rec = &ledger.list(5).unwrap()[0];
        assert_eq!(rec.bytes_freed, 1650);
        assert!(!rec.can_undo);
    }

    #[test]
    fn privacy_with_consent_needs_confirmation() {
        let f = Fixture::new(RULES);
        f.write("home/AppData/Browser/User Data/Default/Network/Cookies", 70);
        f.write("home/AppData/Browser/User Data/Default/Network/Other", 70);
        let ctx = f.ctx();
        let o = ScanOptions { include_privacy: true, only: Some(vec!["t.cookies".into()]), ..opts() };
        let report = scan(&f.rules, &ctx, &o);
        assert_eq!(report.get("t.cookies").unwrap().files, 1);
        let ids = vec!["t.cookies".to_string()];
        let res = clean(&report, &ids, false, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 0);
        assert_eq!(res.outcomes[0].skipped_reason.as_deref(), Some("needs your confirmation"));
        let res = clean(&report, &ids, true, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 1);
    }

    #[test]
    fn running_app_blocks_cleaner() {
        let f = Fixture::new(RULES);
        f.write("home/AppData/Temp/a.tmp", 100);
        let mut ctx = f.ctx();
        ctx.processes = ProcessList::from_names(["Busy.exe"]);
        let report = scan(&f.rules, &ctx, &opts());
        assert!(matches!(report.get("t.temp").unwrap().status, PreviewStatus::AppRunning { .. }));
        assert_eq!(report.total_bytes, 0, "running cleaners are not counted");

        // App started after the preview: clean re-checks.
        let ctx2 = f.ctx();
        let report = scan(&f.rules, &ctx2, &opts());
        let busy = ProcessList::from_names(["busy.exe"]);
        let res = clean(&report, &["t.temp".into()], false, &[], &ctx2.guard, &busy, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 0);
        assert!(f.root().join("home/AppData/Temp/a.tmp").exists());
    }

    #[test]
    fn changed_files_are_left_alone() {
        let f = Fixture::new(RULES);
        let p = f.write("home/AppData/Temp/a.tmp", 100);
        let ctx = f.ctx();
        let report = scan(&f.rules, &ctx, &opts());
        fs::write(&p, vec![b'y'; 300]).unwrap();
        let res = clean(&report, &["t.temp".into()], false, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.outcomes[0].changed, 1);
        assert!(p.exists());
    }

    #[test]
    fn min_age_filter() {
        let toml = r#"
[[cleaner]]
id = "t.old"
name = "Old"
description = "old logs"
category = "logs"
risk = "safe"
[[cleaner.target]]
path = "%LOCALAPPDATA%/Logs"
min_age_hours = 24
"#;
        let f = Fixture::new(toml);
        f.write("home/AppData/Logs/new.log", 10);
        let mut ctx = f.ctx();
        ctx.now = SystemTime::now();
        assert_eq!(scan(&f.rules, &ctx, &opts()).get("t.old").unwrap().files, 0);
        ctx.now = SystemTime::now() + Duration::from_secs(25 * 3600);
        assert_eq!(scan(&f.rules, &ctx, &opts()).get("t.old").unwrap().files, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        let f = Fixture::new(RULES);
        let secret = f.write("home/Documents/secret.txt", 10);
        fs::create_dir_all(f.root().join("home/AppData/Temp")).unwrap();
        std::os::unix::fs::symlink(f.root().join("home/Documents"), f.root().join("home/AppData/Temp/link"))
            .unwrap();
        std::os::unix::fs::symlink(&secret, f.root().join("home/AppData/Temp/file-link")).unwrap();
        let ctx = f.ctx();
        let report = scan(&f.rules, &ctx, &opts());
        assert_eq!(report.get("t.temp").unwrap().files, 0);
        let res = clean(&report, &["t.temp".into()], true, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 0);
        assert!(secret.exists());
    }

    #[test]
    fn recycle_bin_respects_age() {
        let toml = r#"
[[cleaner]]
id = "t.recycle"
name = "Recycle"
description = "bin"
category = "recycle_bin"
risk = "ask"
[[cleaner.target]]
kind = "recycle_bin"
"#;
        let f = Fixture::new(toml);
        let bin = f.root().join("bin/S-1-5-21-1");
        fs::create_dir_all(&bin).unwrap();
        let old = SystemTime::now() - Duration::from_secs(40 * 24 * 3600);
        let fresh = SystemTime::now() - Duration::from_secs(3600);
        fs::write(bin.join("$IOLD.zip"), make_i_file_v2(4, system_time_to_filetime(old), "C:\\old.zip")).unwrap();
        fs::write(bin.join("$ROLD.zip"), b"data").unwrap();
        fs::write(bin.join("$INEW.txt"), make_i_file_v2(3, system_time_to_filetime(fresh), "C:\\new.txt")).unwrap();
        fs::write(bin.join("$RNEW.txt"), b"new").unwrap();
        fs::create_dir_all(bin.join("$RDIR")).unwrap();
        fs::write(bin.join("$RDIR/inner.txt"), b"inner").unwrap();
        fs::write(bin.join("$IDIR"), make_i_file_v2(5, system_time_to_filetime(old), "C:\\folder")).unwrap();

        let mut ctx = f.ctx();
        ctx.now = SystemTime::now();
        ctx.recycle_dirs = vec![bin.clone()];
        ctx.recycle_min_age_hours = 30 * 24;
        let report = scan(&f.rules, &ctx, &opts());
        let p = report.get("t.recycle").unwrap();
        assert_eq!(p.files, 2);
        assert_eq!(p.bytes, 9);
        let res = clean(&report, &["t.recycle".into()], true, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 2);
        assert!(!bin.join("$ROLD.zip").exists() && !bin.join("$IOLD.zip").exists());
        assert!(!bin.join("$RDIR").exists());
        assert!(bin.join("$RNEW.txt").exists(), "recent item stays");
    }
    #[test]
    fn user_files_in_temp_need_approval() {
        let toml = r#"
[[cleaner]]
id = "t.tempi"
name = "Temp"
description = "temp"
category = "system"
risk = "safe"
inspect = true
[[cleaner.target]]
path = "%LOCALAPPDATA%/Temp"
"#;
        let f = Fixture::new(toml);
        f.write("home/AppData/Temp/a.tmp", 10);
        let pdf = f.write("home/AppData/Temp/Rar$EX1/Invoice.pdf", 20);
        let odd = f.write("home/AppData/Temp/thing.xyz", 30);
        let ctx = f.ctx();
        let mut report = scan(&f.rules, &ctx, &opts());
        let p = report.get("t.tempi").unwrap();
        assert_eq!(p.files, 3);
        assert_eq!(p.flagged.len(), 2);
        assert_eq!(p.flagged_bytes, 50);
        assert_eq!(report.unknown_files().len(), 1);

        // AI clears the unknown file; it must not be able to clear the pdf.
        report.apply_review(&odd, false, "looks like cache");
        report.apply_review(&pdf, false, "trying to clear a document");
        assert_eq!(report.get("t.tempi").unwrap().flagged.len(), 1);

        let ids = vec!["t.tempi".to_string()];
        let res = clean(&report, &ids, false, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 2);
        assert_eq!(res.outcomes[0].kept_for_approval, 1);
        assert!(pdf.exists());

        let res = clean(&report, &ids, false, std::slice::from_ref(&pdf), &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 1);
        assert!(!pdf.exists());
    }

    #[test]
    fn ai_can_escalate_junk() {
        let toml = r#"
[[cleaner]]
id = "t.tempi"
name = "Temp"
description = "temp"
category = "system"
risk = "safe"
inspect = true
[[cleaner.target]]
path = "%LOCALAPPDATA%/Temp"
"#;
        let f = Fixture::new(toml);
        let log = f.write("home/AppData/Temp/important.log", 10);
        let ctx = f.ctx();
        let mut report = scan(&f.rules, &ctx, &opts());
        report.apply_review(&log, true, "name says important");
        let res = clean(&report, &["t.tempi".into()], false, &[], &ctx.guard, &ctx.processes, &f.rules, None).unwrap();
        assert_eq!(res.deleted_files, 0);
        assert!(log.exists());
    }

}
