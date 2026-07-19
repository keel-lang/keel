//! `keel-rt-ffi` — the runtime-linkage layer for `keel-codegen`-compiled
//! binaries (`designs/llvm-compilation.md` §2.2, §2.6). Builds to a static
//! library (`libkeel_rt.a`) that a compiled program's linked binary embeds;
//! never links LLVM.
//!
//! Entry point: [`keel_rt_start`], called from the `main` that `keel-codegen`
//! emits.

pub mod host;
mod scheduler;

pub use host::CompiledHost;
pub use scheduler::keel_rt_start;
