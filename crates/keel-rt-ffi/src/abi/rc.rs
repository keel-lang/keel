//! Retain/release for [`super`]'s `KeelBox` (`Arc<Value>` behind an opaque
//! pointer). Generated code calls these at ordinary RC discipline points —
//! `keel_retain` when a box is stored somewhere that outlives the call that
//! produced it, `keel_release` once done with it.
//!
//! M1 does not yet emit `keel_retain`/`keel_release` around `CallTarget::Ns`
//! call sites (`keel-codegen`'s `emit_call`): every argument box is
//! constructed immediately before the call and never stored, and the
//! returned payload is discarded (M1 only wires `Unit`-returning methods).
//! Both leak for the process's lifetime, which is harmless for M1's
//! short-lived compiled fixtures — emitting the matching release calls is
//! left to the milestone that gives compiled programs long-lived `dynamic`
//! values worth freeing.

use keel_runtime::interpreter::value::Value;
use std::sync::Arc;

/// Increments a `KeelBox`'s strong count without taking ownership.
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` (see [`super::keel_box_int`]'s ownership
/// convention).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_retain(ptr: *const Value) {
    unsafe { Arc::increment_strong_count(ptr) }
}

/// Drops one strong reference to a `KeelBox`, freeing it if this was the
/// last one.
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` the caller has not already released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_release(ptr: *const Value) {
    drop(unsafe { Arc::from_raw(ptr) });
}
