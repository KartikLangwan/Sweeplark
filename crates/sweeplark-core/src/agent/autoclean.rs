//! Automatic cleanup with an approval gate.
//!
//! 1. Scan (same rules as manual cleaning).
//! 2. Files the rules are unsure about go to the local AI for a second opinion (if on).
//!    The AI can mark files risky, or clear *unknown* files; it can never clear a file the
//!    rules see as the user's data.
//! 3. Approval mode "ask always" (default): nothing is deleted until the user approves.
//!    "Risky only": safe junk is cleaned now; risky items wait for approval.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ai::review::{ReviewItem, Verdict};
use crate::cleaner::{self, CleanReport, PreviewStatus, RuleSet, ScanContext, ScanOptions, ScanReport};
use crate::ledger::Ledger;
use crate::platform::processes::ProcessList;
use crate::settings::{ApprovalMode, Settings};
use crate::util::format_bytes;
use crate::{Result, Risk};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalCleaner {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub risk: Risk,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalFile {
    pub path: String,
    pub bytes: u64,
    pub kind: String,
    pub review: Option<String>,
}

/// What an approval request asks for. Stored as JSON in the agent store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApprovalDetail {
    pub cleaners: Vec<ApprovalCleaner>,
    /// Files that look like the user's own data; each needs its own tick.
    pub risky_files: Vec<ApprovalFile>,
    pub total_bytes: u64,
    pub ai_reviewed: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AutoCleanOutcome {
    pub cleaned: Option<CleanReport>,
    pub approval: Option<(String, ApprovalDetail)>,
    pub nothing_to_do: bool,
}

pub type Reviewer<'a> = &'a mut dyn FnMut(&[ReviewItem]) -> Option<Vec<Verdict>>;

pub fn review_unknown(report: &mut ScanReport, reviewer: Reviewer) -> usize {
    let items: Vec<ReviewItem> = report
        .unknown_files()
        .into_iter()
        .take(crate::ai::review::MAX_ITEMS)
        .map(|(cleaner, path, bytes)| ReviewItem {
            cleaner: report.get(&cleaner).map(|c| c.name.clone()).unwrap_or(cleaner),
            path,
            bytes,
        })
        .collect();
    if items.is_empty() {
        return 0;
    }
    let Some(verdicts) = reviewer(&items) else { return 0 };
    for v in &verdicts {
        report.apply_review(&v.path, v.risky, &v.reason);
    }
    verdicts.len()
}

pub fn run(
    rules: &RuleSet,
    settings: &Settings,
    ctx: &ScanContext,
    ledger: &Ledger,
    reviewer: Option<Reviewer>,
) -> Result<AutoCleanOutcome> {
    let mut report = cleaner::scan(rules, ctx, &ScanOptions::from_settings(settings));
    let ai_reviewed = reviewer.map(|r| review_unknown(&mut report, r)).unwrap_or(0);

    let ready: Vec<&cleaner::CleanerPreview> =
        report.cleaners.iter().filter(|c| matches!(c.status, PreviewStatus::Ready)).collect();
    if ready.is_empty() {
        return Ok(AutoCleanOutcome { nothing_to_do: true, ..Default::default() });
    }
    let ask_always = settings.agent.approval == ApprovalMode::AskAlways;

    let mut out = AutoCleanOutcome::default();
    if !ask_always {
        let safe_ids: Vec<String> = ready.iter().filter(|c| c.risk == Risk::Safe).map(|c| c.id.clone()).collect();
        if !safe_ids.is_empty() {
            let res = cleaner::clean(&report, &safe_ids, false, &[], &ctx.guard, &ProcessList::current(), rules, Some(ledger))?;
            if res.deleted_files > 0 {
                out.cleaned = Some(res);
            }
        }
    }

    let mut detail = ApprovalDetail { ai_reviewed, ..Default::default() };
    for c in &ready {
        let needs = ask_always || c.risk != Risk::Safe;
        if needs {
            let safe_part = c.bytes - c.flagged_bytes;
            if safe_part > 0 {
                detail.cleaners.push(ApprovalCleaner {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    bytes: safe_part,
                    files: c.files - c.flagged_count(),
                    risk: c.risk,
                    note: c.note.clone(),
                });
            }
        }
        detail.risky_files.extend(c.flagged.iter().map(|f| ApprovalFile {
            path: f.path.clone(),
            bytes: f.bytes,
            kind: f.kind.clone(),
            review: f.review.clone(),
        }));
    }
    detail.total_bytes =
        detail.cleaners.iter().map(|c| c.bytes).sum::<u64>() + detail.risky_files.iter().map(|f| f.bytes).sum::<u64>();
    if !detail.cleaners.is_empty() || !detail.risky_files.is_empty() {
        let (size, n) = (format_bytes(detail.total_bytes), detail.risky_files.len());
        let lang = settings.language;
        let title = if n == 0 {
            crate::tr!(lang, "Approve cleaning {size}?", "{size} saaf karne ki ijazat chahiye")
        } else {
            crate::tr!(
                lang,
                "Approve cleaning {size}? ({n} files may be yours)",
                "{size} saaf karne ki ijazat chahiye ({n} files aapki ho sakti hain)"
            )
        };
        out.approval = Some((title, detail));
    }
    Ok(out)
}

/// Carry out an approval. `approved_files` = the risky files the user ticked.
/// Everything is scanned again first, so files that changed since are left alone.
pub fn apply_approval(
    detail: &ApprovalDetail,
    approved_files: &[String],
    rules: &RuleSet,
    settings: &Settings,
    ctx: &ScanContext,
    ledger: &Ledger,
) -> Result<CleanReport> {
    let allowed: Vec<PathBuf> = detail
        .risky_files
        .iter()
        .filter(|f| approved_files.contains(&f.path))
        .map(|f| PathBuf::from(&f.path))
        .collect();
    let mut ids: Vec<String> = detail.cleaners.iter().map(|c| c.id.clone()).collect();
    // Cleaners that only had risky files approved still need to be scanned.
    let mut opts = ScanOptions::from_settings(settings);
    opts.only = None;
    let full = cleaner::scan(rules, ctx, &opts);
    for c in &full.cleaners {
        if c.flagged.iter().any(|f| allowed.iter().any(|a| a.to_string_lossy() == f.path)) && !ids.contains(&c.id) {
            ids.push(c.id.clone());
        }
    }
    let selected_only_risky: Vec<String> =
        ids.iter().filter(|id| !detail.cleaners.iter().any(|c| &c.id == *id)).cloned().collect();
    // For cleaners added only for their risky files, delete just those files.
    let mut report = full;
    for c in report.cleaners.iter_mut().filter(|c| selected_only_risky.contains(&c.id)) {
        c.keep_only(&allowed);
    }
    cleaner::clean(&report, &ids, true, &allowed, &ctx.guard, &ProcessList::current(), rules, Some(ledger))
}
