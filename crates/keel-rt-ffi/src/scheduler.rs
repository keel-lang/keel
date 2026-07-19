//! `keel_rt_start` — the handshake between a `keel-codegen`-linked binary's
//! `main` and the Keel runtime. Boots a tokio runtime, constructs the same
//! `RuntimeContext` the interpreter uses, and calls back into the compiled
//! program's `keel_user_toplevel` (emitted by `keel-codegen`).

use std::sync::Arc;

use keel_runtime::runtime::context::RuntimeContext;

use crate::host::CompiledHost;

unsafe extern "C" {
    /// Emitted by `keel-codegen` for every compiled program (`func.rs`'s
    /// `emit_toplevel_function`). Returns the M1 exit-code-convention value.
    fn keel_user_toplevel() -> i32;
}

/// Called from the linked binary's `main`. Boots tokio + `RuntimeContext`,
/// then runs the compiled program's top-level code to completion.
///
/// # Safety
///
/// Must only be called once, from `main`, before any other keel-rt-ffi entry
/// point — it owns process-wide startup (the tokio runtime).
#[unsafe(no_mangle)]
pub extern "C" fn keel_rt_start() -> i32 {
    let tokio_rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("keel: failed to start async runtime: {e}");
            return 1;
        }
    };

    tokio_rt.block_on(async {
        let runtime = RuntimeContext::native();
        let _host = CompiledHost::new(Arc::clone(&runtime));

        // SAFETY: `keel_user_toplevel` is emitted by keel-codegen for every
        // compiled program; the object this crate is statically linked into
        // always defines it.
        unsafe { keel_user_toplevel() }
    })
}
