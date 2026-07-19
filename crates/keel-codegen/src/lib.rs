//! `keel-codegen` — KIR -> LLVM IR -> native object -> linked binary
//! (`designs/llvm-compilation.md` §2.2). The only crate in the workspace
//! that links LLVM; gated behind the root crate's `build-backend` feature.
//!
//! # M1 scope
//!
//! Scalar arithmetic on `int`/`float`/`bool`, `if`/`else`, `while`, `for`
//! over a range, and direct calls between compiled tasks (`CallTarget::Fn`)
//! — still no I/O (`CallTarget::Ns` namespace dispatch lands in #135). The
//! program's `toplevel` KIR function compiles to a real `keel_user_toplevel`
//! function (see the module doc on [`func`] for the bare-`int`-expression ->
//! exit-code convention it still follows), and the emitted `main` just calls
//! into `keel_rt_start` (from the separate `keel-rt-ffi` crate) to boot the
//! runtime and invoke it — see [`BuildOptions::runtime_link_args`].
//!
//! Entry point: [`compile`].

mod context;
mod expr;
mod func;
mod layout;
mod link;
mod ns_call;
mod stmt;

use std::path::PathBuf;

use inkwell::context::Context;
use keel_kir::ir::KirProgram;

/// Where to write the intermediate object file and the linked binary.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub out_dir: PathBuf,
    /// Extra `cc` arguments needed to link the emitted `main` (which calls
    /// `keel_rt_start`) against `libkeel_rt.a` — the archive's path followed
    /// by its `native-static-libs` (platform-specific system libs a Rust
    /// staticlib needs; see `rustc --print native-static-libs`). `keel-codegen`
    /// treats this as opaque link input and never depends on `keel-rt-ffi`
    /// itself, per the dependency rule in `designs/llvm-compilation.md` §2.2.
    pub runtime_link_args: Vec<String>,
}

/// Everything that can go wrong turning a [`KirProgram`] into a running
/// native binary.
#[derive(Debug)]
pub enum CodegenError {
    /// A KIR construct this milestone's codegen does not lower yet (e.g.
    /// `str`, cross-function calls, control flow — see the relevant later
    /// issue).
    Unsupported(String),
    /// The native LLVM target backend could not be initialized, or no
    /// `TargetMachine` could be created for the host triple — almost always
    /// a broken/missing local LLVM install (see this crate's README).
    Target(String),
    /// An `inkwell` builder call failed, or the LLVM module verifier
    /// rejected the emitted IR — an internal codegen bug, not a KIR/user
    /// error (the KIR that reached this crate already passed
    /// `keel_kir::passes::verify`).
    Llvm(String),
    /// Emitting the native object file failed.
    ObjectEmission(String),
    /// The system `cc` link step failed to spawn or exited non-zero.
    Link(String),
    Io(std::io::Error),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::Unsupported(what) => {
                write!(f, "`{what}` is not supported by keel-codegen yet")
            }
            CodegenError::Target(msg) => write!(f, "LLVM target error: {msg}"),
            CodegenError::Llvm(msg) => write!(f, "LLVM codegen error: {msg}"),
            CodegenError::ObjectEmission(msg) => write!(f, "object emission failed: {msg}"),
            CodegenError::Link(msg) => write!(f, "link step failed: {msg}"),
            CodegenError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for CodegenError {}

impl From<std::io::Error> for CodegenError {
    fn from(e: std::io::Error) -> Self {
        CodegenError::Io(e)
    }
}

/// Compiles `program` to a native binary under `opts.out_dir`, returning the
/// binary's path.
///
/// # Errors
///
/// See [`CodegenError`].
pub fn compile(program: &KirProgram, opts: &BuildOptions) -> Result<PathBuf, CodegenError> {
    std::fs::create_dir_all(&opts.out_dir)?;

    let llvm_context = Context::create();
    let module = llvm_context.create_module("keel_program");
    let builder = llvm_context.create_builder();

    // Declare every non-`toplevel` function up front (so forward/mutual/
    // recursive calls resolve), then emit bodies, then `toplevel` itself
    // (as `keel_user_toplevel`), then the trivial `main` that hands off to
    // `keel_rt_start`.
    let functions = func::declare_functions(&llvm_context, &module, program)?;
    for (id, kir_func) in program.functions.iter().enumerate() {
        if id == program.toplevel {
            continue;
        }
        let function_value = functions[id].expect("declared above");
        func::emit_function(
            &llvm_context,
            &module,
            &builder,
            &functions,
            kir_func,
            function_value,
        )?;
    }
    func::emit_toplevel_function(&llvm_context, &module, &builder, &functions, program)?;
    func::emit_main(&llvm_context, &module, &builder)?;

    module
        .verify()
        .map_err(|e| CodegenError::Llvm(e.to_string()))?;

    let machine = context::host_target_machine()?;

    let obj_path = opts.out_dir.join("keel_program.o");
    let bin_path = opts.out_dir.join("keel_program");
    link::emit_object(&machine, &module, &obj_path)?;
    link::link_binary(&obj_path, &bin_path, &opts.runtime_link_args)?;

    Ok(bin_path)
}
