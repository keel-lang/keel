//! Exercises the exact path `keel-codegen` will generate for a
//! `CallTarget::Ns` call site: `keel_rt_start` boots the runtime, the
//! (mock) compiled toplevel boxes a string constant via `keel_box_str` and
//! calls `keel_rt_call_ns` with `io.show`'s real `(ns_id, method_id)`
//! resolved from `keel-catalog`, exactly as `keel-kir`'s lowering would.
//! `keel-codegen`'s own tests prove the LLVM side actually emits this same
//! call shape; this proves the runtime side accepts it end to end with no
//! LLVM involved.

use keel_rt::ns_dispatch::keel_rt_call_ns;

#[unsafe(no_mangle)]
pub extern "C" fn keel_user_toplevel() -> i32 {
    let ns_id = keel_catalog::namespace_id("io").expect("io is a registered namespace");
    let method_id = keel_catalog::catalog_method("io", "show")
        .expect("io.show is a registered method")
        .method_id;

    let msg = "hello from full_ns_call";
    let boxed = unsafe { keel_rt::abi::keel_box_str(msg.as_ptr(), msg.len()) };
    let args = [boxed];
    let arg_names: [*const std::ffi::c_char; 1] = [std::ptr::null()];

    let res = unsafe { keel_rt_call_ns(ns_id, method_id, args.as_ptr(), arg_names.as_ptr(), 1, 0) };
    i32::from(res.is_err)
}

#[test]
fn keel_rt_call_ns_dispatches_io_show_end_to_end() {
    assert_eq!(keel_rt::keel_rt_start(), 0, "io.show must not error");
}
