//! `keel_rt_call_value_method` — the generic value-method dispatch entry
//! point `keel-codegen` compiles every value-method call site into
//! (`designs/llvm-compilation.md` §2.7, issue #214). Structurally identical
//! to [`crate::ns_dispatch::keel_rt_call_ns`] except the receiver is passed
//! as an extra boxed arg and dispatch goes by method *name* rather than a
//! numeric `ns_id`/`method_id` pair — value methods (`xs.map`, `s.upper`, …)
//! aren't in the `keel-catalog` namespace registry, they're a hardcoded
//! match in `keel_runtime::interpreter::call_method_on_value` (issue #213).
//! Runs through `CompiledHost::call_method_on_value` — the exact same
//! dispatch the interpreter uses.

use std::ffi::{CStr, c_char};

use keel_runtime::interpreter::CallArgValue;
use keel_runtime::interpreter::value::Value;

use crate::abi;
use crate::ns_dispatch::KeelRes;
use crate::scheduler::block_on_value_method_call;

/// # Safety
///
/// `receiver` must be a live `KeelBox` this call borrows, not consumes.
/// `method` must be a valid, NUL-terminated C string. `args`/`arg_names`
/// follow [`crate::ns_dispatch::keel_rt_call_ns`]'s contract exactly.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_rt_call_value_method(
    receiver: *const Value,
    method: *const c_char,
    args: *const *const Value,
    arg_names: *const *const c_char,
    nargs: u32,
    _span_id: u32,
) -> KeelRes {
    // SAFETY: caller's contract (see this fn's doc).
    let receiver = unsafe { abi::borrow(receiver) };
    // SAFETY: caller's contract (see this fn's doc).
    let method = unsafe { CStr::from_ptr(method) }.to_string_lossy();

    let mut call_args = Vec::with_capacity(nargs as usize);
    for i in 0..nargs as usize {
        // SAFETY: caller's contract (see this fn's doc).
        let value = unsafe { abi::borrow(*args.add(i)) };
        let name_ptr = unsafe { *arg_names.add(i) };
        let name = (!name_ptr.is_null()).then(|| {
            unsafe { CStr::from_ptr(name_ptr) }
                .to_string_lossy()
                .into_owned()
        });
        call_args.push(CallArgValue { name, value });
    }

    match block_on_value_method_call(receiver, &method, call_args) {
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
