//! `keel_rt_start` — the handshake between a `keel-codegen`-linked binary's
//! `main` and the Keel runtime. Boots a tokio runtime, constructs the same
//! `RuntimeContext`/`CompiledHost` the interpreter's namespace closures run
//! against, and calls back into the compiled program's top-level function
//! (`keel_user_toplevel`, emitted by `keel-codegen`).
//!
//! `keel_user_toplevel` is ordinary synchronous compiled code (LLVM has no
//! async), so it's called directly here — *not* wrapped in `block_on` — so
//! that a `CallTarget::Ns` site inside it can reach back into the runtime
//! via [`block_on_namespace_call`] with a plain, non-nested `Handle::block_on`
//! rather than needing `block_in_place`. The tokio runtime and `CompiledHost`
//! live in a thread-local for the duration of that call chain; M1 has no
//! agents/spawned tasks yet, so everything runs on this one thread.

use std::cell::RefCell;

use keel_runtime::interpreter::CallArgValue;
use keel_runtime::interpreter::host::Host;
use keel_runtime::interpreter::value::Value;
use keel_runtime::runtime::context::RuntimeContext;
use tokio::runtime::{Handle, Runtime};

use crate::host::CompiledHost;

unsafe extern "C" {
    /// Emitted by `keel-codegen` for every compiled program (`func.rs`'s
    /// `emit_toplevel_function`). Returns the M1 exit-code-convention value.
    fn keel_user_toplevel() -> i32;
}

thread_local! {
    static CONTEXT: RefCell<Option<(CompiledHost, Handle)>> = const { RefCell::new(None) };
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
    let tokio_rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("keel: failed to start async runtime: {e}");
            return 1;
        }
    };

    let host = CompiledHost::new(RuntimeContext::native());
    let handle = tokio_rt.handle().clone();
    CONTEXT.with(|c| *c.borrow_mut() = Some((host, handle)));

    // SAFETY: `keel_user_toplevel` is emitted by keel-codegen for every
    // compiled program; the object this crate is statically linked into
    // always defines it.
    let code = unsafe { keel_user_toplevel() };

    CONTEXT.with(|c| c.borrow_mut().take());
    code
}

/// Dispatches a namespace-method call through the current thread's
/// `CompiledHost`, blocking this (synchronous, LLVM-emitted) call site until
/// the async namespace closure completes. Used by [`crate::ns_dispatch::
/// keel_rt_call_ns`].
///
/// # Panics
///
/// If called from a thread `keel_rt_start` never ran on.
pub(crate) fn block_on_namespace_call(
    ns: &str,
    method: &str,
    args: Vec<CallArgValue>,
) -> miette::Result<Value> {
    CONTEXT.with(|c| {
        let mut binding = c.borrow_mut();
        let (host, handle) = binding
            .as_mut()
            .expect("keel_rt_call_ns invoked on a thread keel_rt_start never ran on");
        let handle = handle.clone();
        handle.block_on(host.call_namespace_method(ns, method, args))
    })
}

/// Dispatches a value-method call (`xs.map`, `s.upper`, …) through the
/// current thread's `CompiledHost`, blocking this (synchronous, LLVM-emitted)
/// call site until the async dispatch completes. Used by
/// [`crate::value_dispatch::keel_rt_call_value_method`].
///
/// # Panics
///
/// If called from a thread `keel_rt_start` never ran on.
pub(crate) fn block_on_value_method_call(
    obj: Value,
    method: &str,
    args: Vec<CallArgValue>,
) -> miette::Result<Value> {
    CONTEXT.with(|c| {
        let mut binding = c.borrow_mut();
        let (host, handle) = binding
            .as_mut()
            .expect("keel_rt_call_value_method invoked on a thread keel_rt_start never ran on");
        let handle = handle.clone();
        handle.block_on(host.call_method_on_value(obj, method, args))
    })
}
