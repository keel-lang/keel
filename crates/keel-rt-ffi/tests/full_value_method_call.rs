//! Exercises the exact path `keel-codegen` will generate for a value-method
//! call site (issue #214): `keel_rt_start` boots the runtime, the (mock)
//! compiled toplevel boxes a string receiver and an argument via
//! `keel_box_str`, and calls `keel_rt_call_value_method` with `"upper"` and
//! `"contains"`, exactly as `keel-kir`'s lowering would. Mirrors
//! `full_ns_call.rs`'s shape — proves the runtime side accepts this call
//! convention end to end with no LLVM involved.

use std::ffi::CString;

use keel_rt::value_dispatch::keel_rt_call_value_method;
use keel_runtime::interpreter::value::Value;

#[unsafe(no_mangle)]
pub extern "C" fn keel_user_toplevel() -> i32 {
    let receiver = "keel";
    let receiver_boxed = unsafe { keel_rt::abi::keel_box_str(receiver.as_ptr(), receiver.len()) };
    let method = CString::new("upper").unwrap();

    let res = unsafe {
        keel_rt_call_value_method(
            receiver_boxed,
            method.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            0,
        )
    };
    if res.is_err != 0 {
        return 1;
    }
    // SAFETY: `payload` is a live `KeelBox` `keel_rt_call_value_method` just
    // returned ownership of.
    let value = unsafe { &*res.payload };
    if *value != Value::String("KEEL".to_string()) {
        return 2;
    }

    let arg = unsafe { keel_rt::abi::keel_box_str("ee".as_ptr(), 2) };
    let args = [arg];
    let contains = CString::new("contains").unwrap();
    let res = unsafe {
        keel_rt_call_value_method(
            receiver_boxed,
            contains.as_ptr(),
            args.as_ptr(),
            [std::ptr::null()].as_ptr(),
            1,
            0,
        )
    };
    if res.is_err != 0 {
        return 3;
    }
    let value = unsafe { &*res.payload };
    if *value != Value::Bool(true) {
        return 4;
    }

    0
}

#[test]
fn keel_rt_call_value_method_dispatches_str_upper_and_contains_end_to_end() {
    assert_eq!(
        keel_rt::keel_rt_start(),
        0,
        "s.upper()/s.contains() must not error"
    );
}
