//! `KeelBox` — the dynamic-value ABI type generated code and `keel_rt_call_ns`
//! exchange (`designs/llvm-compilation.md` §2.4).
//!
//! **`KeelBox` is literally the interpreter's `Value`, behind an opaque,
//! reference-counted pointer** (`Arc<Value>::into_raw`) — not a separate
//! reimplementation. This is deliberate: `dynamic` semantics stay identical
//! between engines by construction, and marshalling at the namespace
//! boundary is a cheap wrap/unwrap instead of a structural conversion.
//!
//! M1 only needs to *construct* a `KeelBox` from a compile-time scalar
//! constant (to pass as a namespace-call argument) and read one back (the
//! call's return value) — never a mutable, first-class `KeelStr` with its
//! own concat/slice operations, since KIR has no `str` locals or operations
//! yet (every `str` value in M1 is a literal flowing straight into a
//! `dynamic`-typed namespace-call parameter). A standalone `KeelStr` type
//! lands with M2's string operations; `keel_box_str` covers this milestone.
//!
//! Retain/release live in [`rc`].

pub mod rc;

use std::slice;
use std::sync::Arc;

use keel_runtime::interpreter::value::Value;

/// Boxes an `int` constant. Returns a strong (+1) reference the caller owns
/// and must eventually pass to [`rc::keel_box_release`].
#[unsafe(no_mangle)]
pub extern "C" fn keel_box_int(v: i64) -> *const Value {
    Arc::into_raw(Arc::new(Value::Integer(v)))
}

/// Boxes a `float` constant. See [`keel_box_int`] for ownership.
#[unsafe(no_mangle)]
pub extern "C" fn keel_box_float(v: f64) -> *const Value {
    Arc::into_raw(Arc::new(Value::Float(v)))
}

/// Boxes a `bool` constant (`0` = false, nonzero = true). See [`keel_box_int`]
/// for ownership.
#[unsafe(no_mangle)]
pub extern "C" fn keel_box_bool(v: u8) -> *const Value {
    Arc::into_raw(Arc::new(Value::Bool(v != 0)))
}

/// Boxes a UTF-8 string constant, copying `len` bytes starting at `ptr` into
/// an owned `Value::String`. See [`keel_box_int`] for ownership.
///
/// # Safety
///
/// `ptr` must be valid for reads of `len` bytes, and those bytes must be
/// valid UTF-8 (always true for a `keel-codegen`-emitted string-literal
/// global — this is not a user-facing conversion boundary).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_box_str(ptr: *const u8, len: usize) -> *const Value {
    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    let s = std::str::from_utf8(bytes)
        .expect("keel-codegen only emits valid-UTF-8 string-literal globals")
        .to_string();
    Arc::into_raw(Arc::new(Value::String(s)))
}

/// Compares two boxed strings for equality without consuming either
/// caller's reference — backs `keel-codegen`'s `str == str` / `str != str`
/// (`when` over a `str` scrutinee, or any other string comparison).
/// Returns `1` for equal, `0` otherwise.
///
/// # Safety
///
/// `a` and `b` must each be a live `KeelBox` (see [`borrow`]) whose
/// underlying `Value` is `Value::String` — always true for a `str`-typed
/// KIR expression, which is the only thing `keel-codegen` ever passes here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_str_eq(a: *const Value, b: *const Value) -> u8 {
    let (Value::String(a), Value::String(b)) = (unsafe { &*a }, unsafe { &*b }) else {
        unreachable!("keel-codegen only calls keel_str_eq on str-typed (Value::String) operands");
    };
    u8::from(a == b)
}

/// Reads a `KeelBox`'s value without consuming the caller's reference —
/// used to marshal namespace-call arguments (`const KeelBox**`, i.e.
/// borrowed) into owned `Value`s.
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` (an `Arc<Value>::into_raw` pointer whose
/// strong count has not yet dropped to zero).
pub(crate) unsafe fn borrow(ptr: *const Value) -> Value {
    unsafe { &*ptr }.clone()
}

/// Boxes an owned `Value` (e.g. a namespace method's return value) as a
/// fresh `KeelBox`, mirroring [`keel_box_int`]'s ownership convention.
pub(crate) fn boxed(v: Value) -> *const Value {
    Arc::into_raw(Arc::new(v))
}
