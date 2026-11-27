//! End-to-end checks with a real model. Skipped unless these are set:
//! SWEEPLARK_LLAMA_SERVER=/path/to/llama-server  SWEEPLARK_TEST_MODEL=/path/to/model.gguf
//! Run: cargo test -p sweeplark-core --test live_model -- --ignored --nocapture

use std::path::PathBuf;
use std::time::Instant;

use sweeplark_core::ai::catalog::{AnswerStyle, Catalog, Tier};
use sweeplark_core::ai::facts::{Facts, PcSource};
use sweeplark_core::ai::review::{self, ReviewItem};
use sweeplark_core::ai::runtime::{ModelServer, ServerOptions};
use sweeplark_core::ai::{self, ReplySource};
use sweeplark_core::cleaner::ScanReport;
use sweeplark_core::health::{self, HealthSnapshot};
use sweeplark_core::ledger::{ChangeRecord, LedgerSummary};
use sweeplark_core::startup::StartupItem;

struct RealPc;

impl PcSource for RealPc {
    fn health(&self) -> HealthSnapshot {
        let mut h = health::snapshot(false);
        // Make the test PC look busy so there is something to explain.
        h.memory_used = h.memory_total / 100 * 91;
        h.uptime_secs = 9 * 86_400;
        h
    }
    fn scan(&self) -> Option<ScanReport> {
        None
    }
    fn startup(&self) -> Vec<StartupItem> {
        Vec::new()
    }
    fn history(&self) -> Option<(LedgerSummary, Vec<ChangeRecord>)> {
        None
    }
}

fn setup() -> Option<(PathBuf, PathBuf)> {
    Some((std::env::var_os("SWEEPLARK_LLAMA_SERVER")?.into(), std::env::var_os("SWEEPLARK_TEST_MODEL")?.into()))
}

#[test]
#[ignore]
fn model_answers_reviews_and_unloads() {
    let Some((exe, model)) = setup() else { return };
    let catalog = Catalog::builtin();
    let name = model.file_name().unwrap().to_string_lossy().to_string();
    let spec = catalog.models.iter().find(|m| m.file == name).cloned().unwrap_or_else(|| catalog.get(Tier::Lite).clone());

    let t = Instant::now();
    let server = ModelServer::start(&exe, &model, &ServerOptions::for_model(&spec, false)).expect("server starts");
    println!("loaded {} in {:?}", spec.name, t.elapsed());

    let pc = RealPc;
    for q in ["mera pc slow kyun hai?", "tum kaun ho?", "is my pc safe from virus?"] {
        let facts = Facts::new(&pc, sweeplark_core::Lang::Hinglish);
        let t = Instant::now();
        let mut streamed = String::new();
        let r = ai::answer(q, &[], &facts, Some((&server.client, &spec)), &mut |p| {
            streamed.push_str(p);
            true
        });
        println!("\nQ: {q}\n[{:?} in {:?}] {}\nkey points: {:?}", r.source, t.elapsed(), r.text,
            r.key_points.iter().map(|k| &k.title).collect::<Vec<_>>());
        if r.source == ReplySource::Rules {
            println!("(model text rejected by guard: {streamed:?})");
        }
        assert!(!r.text.is_empty());
    }

    let items = vec![
        ReviewItem { cleaner: "Temp files".into(), path: r"C:\Users\amit\AppData\Local\Temp\Rar$EXa12\Invoice_March.pdf".into(), bytes: 1 << 20 },
        ReviewItem { cleaner: "Temp files".into(), path: r"C:\Users\amit\AppData\Local\Temp\msedge_url_fetcher_123\a.bin".into(), bytes: 5000 },
        ReviewItem { cleaner: "Temp files".into(), path: r"C:\Users\amit\AppData\Local\Temp\WhatsApp\IMG_2041.jpg".into(), bytes: 3 << 20 },
    ];
    let t = Instant::now();
    let verdicts = review::review(&server.client, &items);
    println!("\nreview in {:?}: {verdicts:#?}", t.elapsed());
    assert_eq!(verdicts.len(), 3);
    assert!(verdicts[0].risky, "invoice pdf must be risky");
    assert!(verdicts[2].risky, "photo must be risky");

    let narr = ai::narrate(&server.client, "daily summary",
        &["Aaj 1.2 GB junk saaf hua".into(), "RAM sabse zyada 91% tak gayi (chrome)".into()],
        "freed 1.2 GB; RAM peak 91%; top app chrome", sweeplark_core::Lang::Hinglish);
    println!("\nnarration: {narr:?}");

    drop(server);
    println!("model unloaded");
    let _ = AnswerStyle::Brief;
}
