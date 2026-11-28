//! One-Click Optimize: scan everything, show the plan, fix only the safe part, report.

use serde::Serialize;

use crate::advisor::{self, Finding};
use crate::cleaner::{self, CleanReport, PreviewStatus, RuleSet, ScanContext, ScanOptions, ScanReport};
use crate::health::{self, HealthSnapshot};
use crate::ledger::Ledger;
use crate::platform::processes::ProcessList;
use crate::platform::registry::RegistryBackend;
use crate::settings::Settings;
use crate::startup::{self, Advice, KnownApps, StartupFolders, StartupItem};
use crate::{Result, Risk};

#[derive(Debug, Clone, Serialize)]
pub struct OptimizePlan {
    pub health: HealthSnapshot,
    /// Safe cleaners only: this is what "Theek karo" will run by itself.
    pub scan: ScanReport,
    pub startup: Vec<StartupItem>,
    pub findings: Vec<Finding>,
    pub auto_fix_ids: Vec<String>,
    pub auto_fix_bytes: u64,
    /// Things that need the user's decision (not done automatically).
    pub startup_suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizeReport {
    pub clean: CleanReport,
    pub system_free_before: Option<u64>,
    pub system_free_after: Option<u64>,
    /// Suggestions left for the user to decide on.
    pub left_for_you: Vec<Finding>,
}

pub fn plan(rules: &RuleSet, settings: &Settings, reg: &dyn RegistryBackend) -> Result<OptimizePlan> {
    let health = health::snapshot(true);
    let ctx = ScanContext::system(settings);
    let scan = cleaner::scan(rules, &ctx, &ScanOptions::safe_only(settings));
    // One scan, safe cleaners only, so the number on screen is exactly what one click frees.
    let known = KnownApps::builtin().with_lang(settings.language);
    let startup = startup::list(reg, &StartupFolders::system(), &known).unwrap_or_default();
    let findings = advisor::analyze(&health, Some(&scan), Some(&startup), settings.language);
    let auto_fix_ids: Vec<String> = scan
        .cleaners
        .iter()
        .filter(|c| c.risk == Risk::Safe && matches!(c.status, PreviewStatus::Ready))
        .map(|c| c.id.clone())
        .collect();
    // Files that look like the user's are never cleaned here, so they are not counted.
    let auto_fix_bytes = scan.cleaners.iter().filter(|c| auto_fix_ids.contains(&c.id)).map(|c| c.bytes - c.flagged_bytes).sum();
    let startup_suggestions =
        startup.iter().filter(|i| i.enabled && i.advice == Advice::Disable).map(|i| i.id.clone()).collect();
    Ok(OptimizePlan { health, scan, startup, findings, auto_fix_ids, auto_fix_bytes, startup_suggestions })
}

fn system_free() -> Option<u64> {
    health::snapshot(false).system_disk().map(|d| d.free_bytes)
}

/// Apply the safe part of a plan. Never runs anything above `Risk::Safe`.
pub fn apply(plan: &OptimizePlan, rules: &RuleSet, settings: &Settings, ledger: &Ledger) -> Result<OptimizeReport> {
    let before = plan.health.system_disk().map(|d| d.free_bytes);
    let guard = crate::safety::SafetyGuard::for_current_user(settings.whitelist.clone());
    let clean = cleaner::clean(
        &plan.scan,
        &plan.auto_fix_ids,
        false, // not confirmed: anything not safe is refused by the engine
        &[],   // flagged files (look like user data) are never deleted here
        &guard,
        &ProcessList::current(),
        rules,
        Some(ledger),
    )?;
    let left_for_you = plan
        .findings
        .iter()
        .filter(|f| f.id != "junk.found" && f.severity >= advisor::Severity::Info)
        .cloned()
        .collect();
    Ok(OptimizeReport { clean, system_free_before: before, system_free_after: system_free(), left_for_you })
}
