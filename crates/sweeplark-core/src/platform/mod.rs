//! Operating-system access. Everything Windows-specific lives behind this module so the
//! rest of the engine can be unit-tested on any OS.

pub mod ops;
pub mod processes;
pub mod registry;
pub mod system;
