//! Shared test helper: the `cc` arguments needed to link a compiled binary
//! (whose `main` calls `keel_rt_start`) against `libkeel_rt.a`.
//!
//! `keel-codegen` itself never depends on `keel-rt-ffi` (see `BuildOptions::
//! runtime_link_args`'s doc) — it's a sibling crate whose only connection to
//! codegen is the `keel_rt_start` symbol resolved at link time. This helper
//! builds it via `cargo build -p keel-rt-ffi` (no Cargo dependency edge
//! needed) and derives the platform-specific native libs a Rust staticlib
//! needs via `rustc --print native-static-libs`, rather than hardcoding a
//! list that would silently be wrong on the `build-backend` CI job's Linux
//! runner.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub fn runtime_link_args() -> &'static Vec<String> {
    static ARGS: OnceLock<Vec<String>> = OnceLock::new();
    ARGS.get_or_init(|| {
        let mut args = vec![build_keel_rt_ffi_archive().to_string_lossy().into_owned()];
        args.extend(native_static_libs());
        eprintln!("keel-codegen tests: runtime link args: {args:?}");
        args
    })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/keel-codegen has a parent (crates/)")
        .parent()
        .expect("crates/ has a parent (the workspace root)")
        .to_path_buf()
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
        // `CARGO_TERM_COLOR=always` (as CI sets globally) makes rustc wrap
        // the last token of this diagnostic in an ANSI reset code (e.g.
        // `-lc\x1b[0m`), which `cc`/`ld` then can't resolve as a library
        // name — force color off regardless of the inherited environment.
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
    // Read the rest of `stderr` as one blob rather than line-by-line: rustc's
    // diagnostic renderer can wrap a long native-static-libs list across
    // multiple physical lines (this list is much longer on Linux's glibc
    // than on macOS), and splitting on `.lines()` would silently drop
    // whatever landed on the continuation line. `split_whitespace` treats
    // the wrap's newline like any other separator, so this is safe either
    // way — it's the last thing `--print` emits, nothing structured follows.
    if let Some((_, rest)) = stderr.split_once("native-static-libs:") {
        return rest.split_whitespace().map(str::to_string).collect();
    }
    panic!("`cargo rustc -p keel-rt-ffi -- --print native-static-libs` gave no list:\n{stderr}");
}
