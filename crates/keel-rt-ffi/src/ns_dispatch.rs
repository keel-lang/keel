//! `keel_rt_call_ns` — the one generic namespace-method dispatch entry point
//! `keel-codegen` compiles every `CallTarget::Ns` call site into
//! (`designs/llvm-compilation.md` §2.7). `ns_id`/`method_id` are resolved
//! back to names via `keel-catalog` (the same table KIR lowering assigned
//! them from), the boxed args are unwrapped to owned `Value`s, and the call
//! runs through `CompiledHost::call_namespace_method` — the exact same
//! `Namespace.methods` closure the interpreter would run. All 23 namespaces
//! work this way with zero per-namespace codegen.

use std::ffi::c_char;

use keel_runtime::interpreter::CallArgValue;
use keel_runtime::interpreter::value::Value;

use crate::abi;
use crate::scheduler::block_on_namespace_call;

/// The compiled analogue of `Result<Value>`: `is_err == 0` means `payload`
/// is a `KeelBox` holding the call's return value; nonzero means `payload`
/// is a `KeelBox` holding a `Value::String` error message.
///
/// M1 does not lower `try`/`catch`/`raise`, so no generated code branches on
/// `is_err` yet — every namespace method this milestone wires
/// (`io.show`, `log.*`) never actually returns an error in practice, so this
/// is "a real return convention to build on" (per issue #135's scope) rather
/// than a fully consumed one. A future milestone that lowers `try`/`catch`
/// is what starts reading `is_err`.
#[repr(C)]
pub struct KeelRes {
    pub is_err: u8,
    pub payload: *const Value,
}

/// # Safety
///
/// `args` and `arg_names` must each be valid for `nargs` reads; every
/// `args[i]` must be a live `KeelBox` (see `abi::keel_box_int`'s ownership
/// convention) that this call borrows, not consumes. Each `arg_names[i]` is
/// either null (positional) or a valid, NUL-terminated C string — M1 never
/// emits named-argument calls, so `keel-codegen` always passes all-null, but
/// the shape matches Keel's named-argument semantics for when it does.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_rt_call_ns(
    ns_id: u16,
    method_id: u16,
    args: *const *const Value,
    arg_names: *const *const c_char,
    nargs: u32,
    _span_id: u32,
) -> KeelRes {
    // `ns_id`/`method_id` came from keel-kir's lowering, which only ever
    // assigns pairs it already resolved against this exact catalog — a
    // lookup miss here is a codegen/lowering bug, not a user-facing error.
    let ns = keel_catalog::namespace_by_id(ns_id)
        .unwrap_or_else(|| panic!("keel_rt_call_ns: no namespace registered for ns_id {ns_id}"));
    let method = keel_catalog::method_by_id(ns, method_id).unwrap_or_else(|| {
        panic!("keel_rt_call_ns: no method registered for {ns}'s method_id {method_id}")
    });

    let mut call_args = Vec::with_capacity(nargs as usize);
    for i in 0..nargs as usize {
        // SAFETY: caller's contract (see this fn's doc).
        let value = unsafe { abi::borrow(*args.add(i)) };
        let name_ptr = unsafe { *arg_names.add(i) };
        let name = (!name_ptr.is_null()).then(|| {
            unsafe { std::ffi::CStr::from_ptr(name_ptr) }
                .to_string_lossy()
                .into_owned()
        });
        call_args.push(CallArgValue { name, value });
    }

    match block_on_namespace_call(ns, method, call_args) {
        Ok(value) => KeelRes {
            is_err: 0,
            payload: abi::boxed(value),
        },
        Err(report) => KeelRes {
            is_err: 1,
            payload: abi::boxed(Value::String(report.to_string())),
        },
    }
}
