// Integration test binary.
// Tests are organised into per-area submodule files under tests/integration/.
// Shared helpers live in tests/common/mod.rs (Cargo special-case: not a binary).

mod agent;
mod ai;
#[path = "../common/mod.rs"]
mod common;
mod language;
mod lint;
mod memory;
mod modules;
mod namespaces;
mod net;
mod schedule;
mod smoke;
mod strict;
mod tools;
mod util;
