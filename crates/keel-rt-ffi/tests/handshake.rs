//! Exit criterion for issue #134, isolated from `keel-codegen`/LLVM entirely:
//! `keel_rt_start` boots tokio, constructs `RuntimeContext`/`CompiledHost`,
//! and calls back into `keel_user_toplevel`, propagating its return value.
//! In a real compiled binary that symbol is emitted by `keel-codegen`; here
//! it's a plain mock linked into this test binary, so this test only proves
//! the runtime handshake — `keel-codegen`'s own tests prove the link step.

#[unsafe(no_mangle)]
pub extern "C" fn keel_user_toplevel() -> i32 {
    42
}

#[test]
fn keel_rt_start_boots_and_runs_the_compiled_toplevel() {
    assert_eq!(keel_rt::keel_rt_start(), 42);
}
