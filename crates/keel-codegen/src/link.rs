//! Object emission + link — the loop proven in `spikes/llvm-poc`
//! (`designs/llvm-toolchain-spike.md`), made `Result`-returning instead of
//! `expect`-panicking now that it's production code, not a throwaway spike.

use std::path::Path;
use std::process::Command;

use inkwell::module::Module;
use inkwell::targets::{FileType, TargetMachine};

use crate::CodegenError;

/// Emits a native object file for `module` at `obj_path`.
pub(crate) fn emit_object(
    machine: &TargetMachine,
    module: &Module,
    obj_path: &Path,
) -> Result<(), CodegenError> {
    machine
        .write_to_file(module, FileType::Object, obj_path)
        .map_err(|e| CodegenError::ObjectEmission(e.to_string()))
}

/// Links `obj` into an executable at `bin` via the system `cc` driver.
///
/// `extra_args` carries whatever the caller needs beyond the object file
/// itself — e.g. `libkeel_rt.a`'s path plus its `native-static-libs` (see
/// `BuildOptions::runtime_link_args`). `keel-codegen` never hardcodes that
/// list itself: it's platform-specific (macOS system frameworks vs. glibc's
/// `-ldl -lpthread`, …) and the caller is expected to derive it (e.g. via
/// `rustc --print native-static-libs`), not guess it.
pub(crate) fn link_binary(
    obj: &Path,
    bin: &Path,
    extra_args: &[String],
) -> Result<(), CodegenError> {
    let output = Command::new("cc")
        .arg(obj)
        .args(extra_args)
        .arg("-o")
        .arg(bin)
        .output()
        .map_err(|e| CodegenError::Link(format!("spawn `cc` failed: {e}")))?;

    if !output.status.success() {
        return Err(CodegenError::Link(format!(
            "`cc` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}
