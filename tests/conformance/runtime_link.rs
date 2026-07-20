//! Locates `libkeel_rt.a` and its platform-specific `native-static-libs`,
//! the same way `crates/keel-codegen/tests/support/mod.rs` does — the two
//! don't share a crate (this file compiles into the root package's
//! conformance test binary, that one into `keel-codegen`'s own), so the
//! ~30 lines are duplicated rather than factored into a new shared crate
//! just for this. See that file's doc comment for the full rationale
//! (dependency-purity: `keel-codegen` never depends on `keel-rt-ffi` by
//! name, so the caller supplies the link args) and the `CARGO_TERM_COLOR`
//! pitfall (CI sets it to `always`, which corrupts the last token of
//! `native-static-libs` with an ANSI reset code unless disabled here).

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub fn runtime_link_args() -> &'static Vec<String> {
    static ARGS: OnceLock<Vec<String>> = OnceLock::new();
    ARGS.get_or_init(|| {
        let mut args = vec![build_keel_rt_ffi_archive().to_string_lossy().into_owned()];
        args.extend(native_static_libs());
        args
    })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_keel_rt_ffi_archive() -> PathBuf {
    let output = Command::new("cargo")
        .current_dir(workspace_root())
        .env("CARGO_TERM_COLOR", "never")
        .args([
            "build",
            "-p",
            "keel-rt-ffi",
            "--lib",
            "--message-format=json",
        ])
        .output()
        .expect("spawn `cargo build -p keel-rt-ffi`");
    assert!(
        output.status.success(),
        "building keel-rt-ffi failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-artifact") {
            continue;
        }
        let Some(filenames) = msg.get("filenames").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for name in filenames {
            if let Some(s) = name.as_str()
                && s.ends_with(".a")
            {
                return PathBuf::from(s);
            }
        }
    }
    panic!("`cargo build -p keel-rt-ffi` did not report a `.a` artifact");
}

fn native_static_libs() -> Vec<String> {
    let output = Command::new("cargo")
        .current_dir(workspace_root())
        .env("CARGO_TERM_COLOR", "never")
        .args([
            "rustc",
            "-p",
            "keel-rt-ffi",
            "--lib",
            "-q",
            "--color=never",
            "--",
            "--print",
            "native-static-libs",
        ])
        .output()
        .expect("spawn `cargo rustc -p keel-rt-ffi`");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some((_, rest)) = stderr.split_once("native-static-libs:") {
        return rest.split_whitespace().map(str::to_string).collect();
    }
    panic!("`cargo rustc -p keel-rt-ffi -- --print native-static-libs` gave no list:\n{stderr}");
}
