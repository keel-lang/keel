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

/// Boxes `none` — the `none` sentinel for a pointer-typed nullable (`str?`/
/// `list[T]?`), whose non-null values are already boxed `*const Value`
/// pointers (`Value::String`/`Value::List`) with no null-pointer bit to
/// spare. A nullable *struct* doesn't need this: a native struct record is
/// never `Value`-boxed, so a plain null pointer already means `none` there
/// (`designs/llvm-compilation.md` §1.1) — see [`keel_is_none`] for the
/// matching check. See [`keel_box_int`] for ownership.
#[unsafe(no_mangle)]
pub extern "C" fn keel_box_none() -> *const Value {
    Arc::into_raw(Arc::new(Value::None))
}

/// Reports whether a boxed pointer-typed nullable (`str?`/`list[T]?`) is
/// `none` — the counterpart check to [`keel_box_none`]. Never called on a
/// nullable *struct* (a null pointer there is checked directly, no runtime
/// call needed).
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` (see [`borrow`]) — any variant, since
/// checking is exactly "is this variant `Value::None`".
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_is_none(ptr: *const Value) -> u8 {
    u8::from(matches!(unsafe { &*ptr }, Value::None))
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

/// Concatenates two boxed strings without consuming either caller's
/// reference — backs `keel-codegen`'s `str + str` (plain concatenation and
/// desugared string interpolation both lower to this same operator; see
/// `keel-kir`'s `lower_string_lit`). Returns a **fresh** boxed string; `a`
/// and `b` are untouched, matching `keel_list_push`'s always-clone
/// convention. See [`keel_box_int`] for the returned reference's ownership.
///
/// # Safety
///
/// `a` and `b` must each be a live `KeelBox` whose underlying `Value` is
/// `Value::String` — always true for a `str`-typed KIR expression, which is
/// the only thing `keel-codegen` ever passes here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_str_concat(a: *const Value, b: *const Value) -> *const Value {
    let (Value::String(a), Value::String(b)) = (unsafe { &*a }, unsafe { &*b }) else {
        unreachable!(
            "keel-codegen only calls keel_str_concat on str-typed (Value::String) operands"
        );
    };
    boxed(Value::String(format!("{a}{b}")))
}

/// Converts an `int` to its boxed `str` representation, matching the
/// interpreter's `Value::Integer` `Display` impl exactly (`{n}`) — backs a
/// scalar string-interpolation slot's to-string conversion. See
/// [`keel_box_int`] for the returned reference's ownership.
#[unsafe(no_mangle)]
pub extern "C" fn keel_int_to_str(v: i64) -> *const Value {
    boxed(Value::String(v.to_string()))
}

/// Converts a `float` to its boxed `str` representation, matching the
/// interpreter's `Value::Float` `Display` impl exactly (`{n}`, Rust's
/// shortest round-tripping representation — same formatting the
/// interpreter's own `write!(f, "{n}")` produces). See [`keel_int_to_str`].
#[unsafe(no_mangle)]
pub extern "C" fn keel_float_to_str(v: f64) -> *const Value {
    boxed(Value::String(v.to_string()))
}

/// Converts a `bool` (`0`/nonzero, matching [`keel_box_bool`]'s convention)
/// to its boxed `str` representation (`"true"`/`"false"`), matching the
/// interpreter's `Value::Bool` `Display` impl exactly. See
/// [`keel_int_to_str`].
#[unsafe(no_mangle)]
pub extern "C" fn keel_bool_to_str(v: u8) -> *const Value {
    boxed(Value::String((v != 0).to_string()))
}

/// Unboxes an `int` — `keel-codegen` only ever calls this (and its
/// `keel_unbox_float`/`keel_unbox_bool` siblings) on a box it knows (from
/// the expression's `KirType`) holds that scalar kind.
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` (see [`borrow`]) whose `Value` is
/// `Value::Integer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_unbox_int(ptr: *const Value) -> i64 {
    let Value::Integer(v) = (unsafe { &*ptr }) else {
        unreachable!("keel-codegen only calls keel_unbox_int on int-typed (Value::Integer) boxes");
    };
    *v
}

/// Unboxes a `float`. See [`keel_unbox_int`].
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` whose `Value` is `Value::Float`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_unbox_float(ptr: *const Value) -> f64 {
    let Value::Float(v) = (unsafe { &*ptr }) else {
        unreachable!(
            "keel-codegen only calls keel_unbox_float on float-typed (Value::Float) boxes"
        );
    };
    *v
}

/// Unboxes a `bool` (`0`/`1`, matching [`keel_box_bool`]'s convention). See
/// [`keel_unbox_int`].
///
/// # Safety
///
/// `ptr` must be a live `KeelBox` whose `Value` is `Value::Bool`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_unbox_bool(ptr: *const Value) -> u8 {
    let Value::Bool(v) = (unsafe { &*ptr }) else {
        unreachable!("keel-codegen only calls keel_unbox_bool on bool-typed (Value::Bool) boxes");
    };
    u8::from(*v)
}

/// Builds an empty list. See [`keel_box_int`] for ownership.
#[unsafe(no_mangle)]
pub extern "C" fn keel_list_new() -> *const Value {
    Arc::into_raw(Arc::new(Value::List(Vec::new())))
}

/// Returns a **fresh** list with `elem` appended, leaving `list` and `elem`
/// untouched (a deliberate always-clone, not a real copy-on-write: `Arc::
/// make_mut`-style in-place mutation would only be sound with accurate
/// strong counts, which needs the retain-on-alias/release-on-scope-exit RC
/// pass `designs/llvm-compilation.md` §2.3 describes and this codebase does
/// not implement yet — see `keel-codegen`'s module docs. Always-cloning
/// matches the interpreter's own `Value::List` "push" method exactly
/// (`interpreter/methods.rs`: `let mut result = items.clone(); result.push(..)`),
/// so this is semantically identical, just not the eventual perf
/// optimization). See [`keel_box_int`] for the returned reference's
/// ownership; `list`/`elem` are borrowed, not consumed.
///
/// # Safety
///
/// `list` must be a live `KeelBox` whose `Value` is `Value::List`; `elem`
/// must be a live `KeelBox` (any variant — it becomes one of the list's
/// elements as-is).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_list_push(list: *const Value, elem: *const Value) -> *const Value {
    let Value::List(items) = (unsafe { &*list }) else {
        unreachable!("keel-codegen only calls keel_list_push on a KirType::List value");
    };
    let mut items = items.clone();
    items.push(unsafe { borrow(elem) });
    boxed(Value::List(items))
}

/// Returns a list's element count.
///
/// # Safety
///
/// `list` must be a live `KeelBox` whose `Value` is `Value::List`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_list_len(list: *const Value) -> i64 {
    let Value::List(items) = (unsafe { &*list }) else {
        unreachable!("keel-codegen only calls keel_list_len on a KirType::List value");
    };
    items.len() as i64
}

/// Returns the element at `index` as a **borrowed-then-retained** `KeelBox`
/// (the caller owns the returned reference, same convention as every other
/// `keel_box_*`/`keel_list_*` return). Exits the process with a clear
/// message on out-of-bounds — the interpreter *raises* (`NullError`-style,
/// catchable), but the compiled result/raise calling convention (§2.5)
/// isn't wired up yet (#150); this is an explicit placeholder, not a
/// silent divergence into undefined behavior.
///
/// # Safety
///
/// `list` must be a live `KeelBox` whose `Value` is `Value::List`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_list_get(list: *const Value, index: i64) -> *const Value {
    let Value::List(items) = (unsafe { &*list }) else {
        unreachable!("keel-codegen only calls keel_list_get on a KirType::List value");
    };
    let Ok(idx) = usize::try_from(index) else {
        eprintln!("index {index} out of bounds (length {})", items.len());
        std::process::exit(1);
    };
    let Some(item) = items.get(idx) else {
        eprintln!("index {index} out of bounds (length {})", items.len());
        std::process::exit(1);
    };
    boxed(item.clone())
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
