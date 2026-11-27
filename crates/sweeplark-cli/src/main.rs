//! `sweeplark` – Sweeplark from the command line (also used for scripting and scheduled cleaning).

use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use sweeplark_core::advisor;
use sweeplark_core::cleaner::{self, PreviewStatus, RuleSet, ScanContext, ScanOptions};
use sweeplark_core::health;
use sweeplark_core::ledger::Ledger;
use sweeplark_core::optimize;
use sweeplark_core::paths::AppPaths;
use sweeplark_core::platform::processes::ProcessList;
use sweeplark_core::platform::registry::system_registry;
use sweeplark_core::platform::system::is_admin;
use sweeplark_core::settings::Settings;
use sweeplark_core::startup::{self, Advice, KnownApps, StartupFolders};
use sweeplark_core::util::format_bytes;
use sweeplark_core::{Risk, TrustLevel};

#[derive(Parser)]
#[command(name = "sweeplark", version, about = "Sweeplark: small bird, clean PC.")]
struct Cli {
    /// Output JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PC health: CPU, RAM, disks, battery, security.
    Health,
    /// Find junk files (nothing is deleted).
    Scan {
        /// Show every risk level (Expert).
        #[arg(long)]
        all: bool,
        /// Include browser history/cookies cleaners.
        #[arg(long)]
        privacy: bool,
    },
    /// Scan, show the preview, then clean what you choose.
    Clean {
        /// Only these cleaner ids (comma separated). Default: all enabled.
        #[arg(long, value_delimiter = ',')]
        only: Option<Vec<String>>,
        /// Include browser history/cookies cleaners.
        #[arg(long)]
        privacy: bool,
        /// Do not ask for confirmation (only safe cleaners run then).
        #[arg(long)]
        yes: bool,
    },
    /// Startup apps.
    Startup {
        #[command(subcommand)]
        action: Option<StartupCmd>,
    },
    /// Every change Sweeplark made.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Undo one change.
    Undo { id: i64 },
    /// One-Click: scan, show the plan, fix only the safe part.
    Optimize {
        #[arg(long)]
        yes: bool,
    },
    /// Ask a question, e.g. "Why is my PC slow?"
    Ask { question: Vec<String> },
    /// List all cleaner rules.
    Rules,
}

#[derive(Subcommand)]
enum StartupCmd {
    List,
    Disable { id: String },
    Enable { id: String },
}

struct App {
    paths: AppPaths,
    settings: Settings,
    rules: RuleSet,
    json: bool,
}

impl App {
    fn ledger(&self) -> Result<Ledger> {
        Ledger::open(&self.paths.ledger_db()).context("could not open the change ledger")
    }

    fn print_json<T: serde::Serialize>(&self, value: &T) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    }
}

fn confirm(question: &str) -> bool {
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok();
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes" | "haan" | "ha")
}

fn risk_label(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "safe",
        Risk::Ask => "ask",
        Risk::Expert => "expert",
    }
}

fn status_label(s: &PreviewStatus) -> String {
    match s {
        PreviewStatus::Ready => String::new(),
        PreviewStatus::Empty => "nothing found".into(),
        PreviewStatus::AppRunning { apps } => format!("close first: {}", apps.join(", ")),
        PreviewStatus::NeedsAdmin => "needs admin".into(),
    }
}

fn print_scan(report: &cleaner::ScanReport) {
    println!("{:<34} {:>7} {:>10}", "Cleaner", "Risk", "Size");
    for c in &report.cleaners {
        let size = if c.files > 0 { format_bytes(c.bytes) } else { "-".into() };
        println!("{:<34} {:>7} {:>10}  {}", c.name, risk_label(c.risk), size, status_label(&c.status));
        for b in &c.blocked {
            println!("    blocked by safety: {b}");
        }
    }
    println!(
        "\nTotal: {} ({} files) in {} ms",
        format_bytes(report.total_bytes),
        report.total_files,
        report.duration_ms
    );
}

fn cmd_health(app: &App) -> Result<()> {
    let h = health::snapshot(true);
    if app.json {
        return app.print_json(&h);
    }
    println!("{} | {} | uptime {} hours", h.os, h.host_name, h.uptime_secs / 3600);
    println!("CPU   {} ({} cores) {:.0}%", h.cpu_brand, h.cpu_cores, h.cpu_percent);
    println!(
        "RAM   {} / {} ({:.0}%)",
        format_bytes(h.memory_used),
        format_bytes(h.memory_total),
        h.memory_percent()
    );
    for d in &h.disks {
        println!(
            "Disk  {:<6} {:<4} {} free / {} {}",
            d.mount,
            d.kind,
            format_bytes(d.free_bytes),
            format_bytes(d.total_bytes),
            if d.is_system { "(system)" } else { "" }
        );
    }
    println!("\nUsing the most RAM:");
    for a in h.top_memory.iter().take(5) {
        println!("  {:<28} {:>10}  ({} processes)", a.name, format_bytes(a.memory_bytes), a.processes);
    }
    if let Some(b) = &h.battery {
        println!("\nBattery {:?}% wear {:?}", b.percent, b.wear_percent().map(|w| format!("{w:.0}%")));
    }
    if let Some(s) = &h.security {
        println!(
            "\nDefender: antivirus {:?}, real-time {:?}; firewall: {:?}",
            s.defender_antivirus_enabled, s.defender_realtime_enabled, s.firewall_profiles
        );
    }
    Ok(())
}

fn scan_options(app: &App, all: bool, privacy: bool, only: Option<Vec<String>>) -> ScanOptions {
    let mut o = ScanOptions::from_settings(&app.settings);
    if all {
        o.max_risk = Risk::Expert;
    }
    o.include_privacy = privacy || app.settings.privacy_consent;
    o.only = only;
    o
}

fn cmd_scan(app: &App, all: bool, privacy: bool) -> Result<()> {
    let ctx = ScanContext::system(&app.settings);
    let report = cleaner::scan(&app.rules, &ctx, &scan_options(app, all, privacy, None));
    if app.json {
        return app.print_json(&report);
    }
    print_scan(&report);
    Ok(())
}

fn cmd_clean(app: &App, only: Option<Vec<String>>, privacy: bool, yes: bool) -> Result<()> {
    let ctx = ScanContext::system(&app.settings);
    let report = cleaner::scan(&app.rules, &ctx, &scan_options(app, false, privacy, only));
    if !app.json {
        print_scan(&report);
    }
    let ready: Vec<String> = report
        .cleaners
        .iter()
        .filter(|c| matches!(c.status, PreviewStatus::Ready))
        .map(|c| c.id.clone())
        .collect();
    if ready.is_empty() {
        if !app.json {
            println!("Nothing to clean.");
        }
        return Ok(());
    }
    let has_risky = report.cleaners.iter().any(|c| ready.contains(&c.id) && c.risk != Risk::Safe);
    let confirmed = if yes {
        if has_risky && !app.json {
            println!("With --yes only 'safe' cleaners run; run without --yes for the rest.");
        }
        false
    } else {
        for c in report.cleaners.iter().filter(|c| ready.contains(&c.id)) {
            if let Some(note) = &c.note {
                println!("  ! {}: {note}", c.name);
            }
        }
        if !confirm(&format!("Clean {}?", format_bytes(report.total_bytes))) {
            println!("Nothing changed.");
            return Ok(());
        }
        true
    };
    let ledger = app.ledger()?;
    let res = cleaner::clean(&report, &ready, confirmed, &[], &ctx.guard, &ProcessList::current(), &app.rules, Some(&ledger))?;
    if app.json {
        return app.print_json(&res);
    }
    for o in &res.outcomes {
        let mut line = format!("{:<34} {:>10}", o.name, format_bytes(o.freed_bytes));
        if o.in_use > 0 {
            line.push_str(&format!("  {} files in use (skipped)", o.in_use));
        }
        if let Some(r) = &o.skipped_reason {
            line.push_str(&format!("  skipped: {r}"));
        }
        println!("{line}");
    }
    println!("\nFreed {}. Recorded as change #{:?}.", format_bytes(res.freed_bytes), res.ledger_id);
    Ok(())
}

fn cmd_startup(app: &App, action: Option<StartupCmd>) -> Result<()> {
    let reg = system_registry();
    let items = startup::list(reg.as_ref(), &StartupFolders::system(), &KnownApps::builtin().with_lang(app.settings.language))?;
    match action.unwrap_or(StartupCmd::List) {
        StartupCmd::List => {
            if app.json {
                return app.print_json(&items);
            }
            if items.is_empty() {
                println!("No startup apps found.");
            }
            for i in &items {
                let advice = match i.advice {
                    Advice::Keep => "keep",
                    Advice::Disable => "safe to turn off",
                    Advice::Neutral => "",
                };
                println!(
                    "[{}] {:<28} {:<26} {}\n     id: {}",
                    if i.enabled { "on " } else { "off" },
                    i.name,
                    i.source_label,
                    advice,
                    i.id
                );
            }
        }
        StartupCmd::Disable { id } | StartupCmd::Enable { id } if !items.iter().any(|i| i.id == id) => {
            bail!("startup id '{id}' not found; see `sweeplark startup list`")
        }
        cmd @ (StartupCmd::Disable { .. } | StartupCmd::Enable { .. }) => {
            let (id, enable) = match cmd {
                StartupCmd::Disable { id } => (id, false),
                StartupCmd::Enable { id } => (id, true),
                StartupCmd::List => unreachable!(),
            };
            let item = items.iter().find(|i| i.id == id).expect("checked above");
            let ledger = app.ledger()?;
            let change = startup::set_enabled_recorded(reg.as_ref(), &ledger, item, enable, is_admin(), app.settings.language)?;
            println!(
                "{} {}. To undo: sweeplark undo {change}",
                item.name,
                if enable { "turned on" } else { "turned off" }
            );
        }
    }
    Ok(())
}

fn cmd_history(app: &App, limit: usize) -> Result<()> {
    let ledger = app.ledger()?;
    let rows = ledger.list(limit)?;
    if app.json {
        return app.print_json(&rows);
    }
    let s = ledger.summary()?;
    println!("{} changes, {} freed in total.\n", s.changes, format_bytes(s.bytes_freed));
    for r in rows {
        println!(
            "#{:<4} {} {:<9} {}{}",
            r.id,
            r.at.format("%Y-%m-%d %H:%M"),
            format!("{:?}", r.status).to_lowercase(),
            r.title,
            if r.can_undo { "  (can undo)" } else { "" }
        );
    }
    Ok(())
}

fn cmd_undo(app: &App, id: i64) -> Result<()> {
    let ledger = app.ledger()?;
    let rec = ledger.undo(id, system_registry().as_ref())?;
    if app.json {
        return app.print_json(&rec);
    }
    println!("Undone: {}", rec.title);
    Ok(())
}

fn cmd_optimize(app: &App, yes: bool) -> Result<()> {
    let reg = system_registry();
    let plan = optimize::plan(&app.rules, &app.settings, reg.as_ref())?;
    if !app.json {
        println!("== Scan done ==");
        for f in &plan.findings {
            println!("[{:?}] {}\n    {}", f.severity, f.title, f.detail);
        }
        println!("\nSafe cleanup: {} ({} cleaners)", format_bytes(plan.auto_fix_bytes), plan.auto_fix_ids.len());
    }
    if plan.auto_fix_ids.is_empty() {
        if app.json {
            return app.print_json(&plan);
        }
        println!("Nothing to fix automatically.");
        return Ok(());
    }
    if !yes && !confirm("Fix the safe items now?") {
        println!("Nothing changed.");
        return Ok(());
    }
    let report = optimize::apply(&plan, &app.rules, &app.settings, &app.ledger()?)?;
    if app.json {
        return app.print_json(&report);
    }
    println!("\n== Report ==\nFreed {}, removed {} files.", format_bytes(report.clean.freed_bytes), report.clean.deleted_files);
    if let (Some(b), Some(a)) = (report.system_free_before, report.system_free_after) {
        println!("System drive free: {} -> {}", format_bytes(b), format_bytes(a));
    }
    if !plan.startup_suggestions.is_empty() {
        println!("For you to decide: {} startup apps can be turned off (`sweeplark startup`).", plan.startup_suggestions.len());
    }
    Ok(())
}

fn cmd_ask(app: &App, question: &str) -> Result<()> {
    let h = health::snapshot(true);
    let reg = system_registry();
    let items = startup::list(reg.as_ref(), &StartupFolders::system(), &KnownApps::builtin()).unwrap_or_default();
    let findings = advisor::analyze(&h, None, Some(&items), app.settings.language);
    let answer = advisor::ask(question, &findings, app.settings.language);
    if app.json {
        return app.print_json(&answer);
    }
    println!("{}", answer.text);
    Ok(())
}

fn cmd_rules(app: &App) -> Result<()> {
    if app.json {
        return app.print_json(&app.rules.cleaners);
    }
    for c in &app.rules.cleaners {
        println!(
            "{:<32} {:<7} {:<10} {}{}",
            c.id,
            risk_label(c.risk),
            format!("{:?}", c.category).to_lowercase(),
            c.name,
            if c.default_enabled { "" } else { "  (default off)" }
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    let settings = Settings::load(&paths.settings_file());
    let app = App { paths, settings, rules: RuleSet::builtin(), json: cli.json };
    if app.settings.trust_level == TrustLevel::Safe && !app.json {
        eprintln!("(Safe mode: only safe items are shown)");
    }
    match cli.command {
        Command::Health => cmd_health(&app),
        Command::Scan { all, privacy } => cmd_scan(&app, all, privacy),
        Command::Clean { only, privacy, yes } => cmd_clean(&app, only, privacy, yes),
        Command::Startup { action } => cmd_startup(&app, action),
        Command::History { limit } => cmd_history(&app, limit),
        Command::Undo { id } => cmd_undo(&app, id),
        Command::Optimize { yes } => cmd_optimize(&app, yes),
        Command::Ask { question } => cmd_ask(&app, &question.join(" ")),
        Command::Rules => cmd_rules(&app),
    }
}
