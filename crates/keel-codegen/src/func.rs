//! Emits the M1 walking-skeleton `main`: runs the program's `toplevel` KIR
//! function's statements directly inside `main`'s body (no separate LLVM
//! function for it yet — that starts once `keel_rt_start` needs something to
//! call it, see issue #134) and computes a process exit code from it.
//!
//! # The M1 entry/exit convention (temporary, replaced by #134)
//!
//! Real top-level Keel statements always lower to a `Unit`-returning
//! `toplevel` KIR function (`keel-kir` forces this — top-level `return
//! <value>` is a type error), so there is no way for real source to make
//! `main` exit with a *computed* value yet: there's no `keel_rt_start`/
//! `io.*`/process-exit surface wired up. So, for this issue's "prove object
//! emission + linking" exit criterion only: if the toplevel function's last
//! statement is a bare `int` expression (e.g. a file whose only content is
//! `2 + 2 * 10`), its value becomes the process exit code (truncated to
//! `i32`, mirroring how returning a value from a hosted C `main` becomes its
//! exit status); otherwise `main` exits `0`. This has nothing to do with
//! Keel's `return` statement, and this whole function is replaced wholesale
//! once #134 wires `main` to call `keel_rt_start` instead.

use std::collections::HashMap;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, PointerValue};

use keel_kir::ir::{KirProgram, LocalId};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::stmt;

/// Per-function codegen state: where each KIR local's `alloca` lives.
/// M1 only ever compiles one function (see the module doc), so this never
/// needs a call stack of these — `func.rs` in later issues will change that.
pub(crate) struct FuncCtx<'ctx, 'a> {
    pub(crate) context: &'ctx Context,
    pub(crate) builder: &'a Builder<'ctx>,
    pub(crate) locals: HashMap<LocalId, PointerValue<'ctx>>,
}

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

pub(crate) fn emit_main<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    program: &KirProgram,
) -> Result<(), CodegenError> {
    let i32_type = context.i32_type();
    let main_type = i32_type.fn_type(&[], false);
    let main_fn = module.add_function("main", main_type, None);
    let entry = context.append_basic_block(main_fn, "entry");
    builder.position_at_end(entry);

    let toplevel = &program.functions[program.toplevel];
    let mut fcx = FuncCtx {
        context,
        builder,
        locals: HashMap::new(),
    };
    let last = stmt::emit_block(&mut fcx, &toplevel.body)?;

    let exit_code = match last {
        Some((BasicValueEnum::IntValue(v), KirType::I64)) => builder
            .build_int_truncate(v, i32_type, "exit_code")
            .map_err(llvm_err)?,
        _ => i32_type.const_zero(),
    };
    builder.build_return(Some(&exit_code)).map_err(llvm_err)?;
    Ok(())
}
