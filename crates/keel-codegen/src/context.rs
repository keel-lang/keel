//! Host `TargetMachine` creation. `inkwell::context::Context`/`Module`/
//! `Builder` construction itself is trivial enough (three constructor calls)
//! that `lib.rs::compile` does it inline; this module holds the one part
//! with real failure modes — talking to LLVM's target registry.

use inkwell::OptimizationLevel;
use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};

use crate::CodegenError;

/// Creates a `TargetMachine` for the host triple, initializing the native
/// LLVM target backend first. No cross-compilation / `--target` support yet
/// (M1 scope) — always the machine this process is running on.
pub(crate) fn host_target_machine() -> Result<TargetMachine, CodegenError> {
    Target::initialize_native(&InitializationConfig::default()).map_err(CodegenError::Target)?;

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| CodegenError::Target(e.to_string()))?;

    let cpu = TargetMachine::get_host_cpu_name();
    let features = TargetMachine::get_host_cpu_features();
    let cpu = cpu
        .to_str()
        .map_err(|e| CodegenError::Target(e.to_string()))?;
    let features = features
        .to_str()
        .map_err(|e| CodegenError::Target(e.to_string()))?;

    target
        .create_target_machine(
            &triple,
            cpu,
            features,
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| {
            CodegenError::Target(format!(
                "failed to create a TargetMachine for host triple {}",
                triple.as_str().to_string_lossy()
            ))
        })
}
