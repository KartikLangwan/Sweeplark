//! Sweeplark core engine.
//!
//! Design rules (see `docs/SAFETY.md`):
//! * Every scan is a preview first; nothing is changed until the caller applies a plan.
//! * Only [`Risk::Safe`] work may ever run automatically.
//! * Personal folders are never touched by the cleaner.
//! * Every change is written to the [`ledger::Ledger`], with revert steps where undo is possible.

pub mod advisor;
pub mod agent;
pub mod ai;
pub mod branding;
pub mod boost;
pub mod cleaner;
pub mod error;
pub mod health;
pub mod lang;
pub mod monitor;
pub mod ledger;
pub mod optimize;
pub mod paths;
pub mod platform;
pub mod revert;
pub mod risk;
pub mod safety;
pub mod settings;
pub mod startup;
pub mod storage;
pub mod tools;
pub mod tweaks;
pub mod update;
pub mod util;

pub use error::{Error, Result};
pub use lang::Lang;
pub use risk::{Risk, TrustLevel};
