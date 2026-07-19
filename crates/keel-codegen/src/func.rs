//! Function emission.
//!
//! Every `KirFunction` except the synthetic `toplevel` one compiles to a
//! real, separate LLVM function — `emit_function` — so `CallTarget::Fn`
//! calls between them are ordinary LLVM `call` instructions. `toplevel`'s
//! statements are still inlined directly into `main` (`emit_main`), exactly
//! as in the M1 walking-skeleton (issue #132): real top-level Keel
//! statements always lower to a `Unit`-returning function with no way to
//! produce a computed exit code, so `main`'s "bare `int` expression ->
//! process exit code" convention documented there is unchanged — it's
//! replaced wholesale once `main` calls `keel_rt_start` (issue #134).
//!
//! # Basic-block wiring
//!
//! `stmt.rs` builds real LLVM basic blocks for `if`/`while`/`for` (branches,
//! loop header/body/exit) and relies on LLVM's `mem2reg` to turn the
//! `alloca`-based locals back into SSA registers later, per
//! `designs/llvm-compilation.md` §2.3's structured-IR rationale — this crate
//! does no manual CFG/SSA construction of its own.

use std::collections::HashMap;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicType;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use keel_kir::ir::{KirFunction, KirProgram, LocalId};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::stmt;

pub(crate) fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

/// Per-function codegen state, shared by every statement/expression walker.
pub(crate) struct FuncCtx<'ctx, 'a> {
    pub(crate) context: &'ctx Context,
    pub(crate) builder: &'a Builder<'ctx>,
    /// The LLVM function currently being emitted into — needed to attach
    /// new basic blocks (`if`/`while`/`for` wiring).
    pub(crate) function: FunctionValue<'ctx>,
    /// Every non-`toplevel` `KirFunction`'s compiled `FunctionValue`,
    /// indexed by `FuncId`, resolved up front (before any body is emitted)
    /// so forward/mutual/recursive `CallTarget::Fn` calls resolve. The
    /// `toplevel` slot is `None` — nothing ever calls it by `FuncId`.
    pub(crate) functions: &'a [Option<FunctionValue<'ctx>>],
    /// `true` while emitting `toplevel`'s statements inline into `main`.
    /// `return` is rejected in that context (see the module doc) rather
    /// than given ad hoc exit-code semantics.
    pub(crate) is_toplevel_in_main: bool,
    pub(crate) locals: HashMap<LocalId, PointerValue<'ctx>>,
}

/// Declares (signature only, no body) every `KirFunction` except
/// `program.toplevel`, so calls between them resolve regardless of
/// declaration order.
pub(crate) fn declare_functions<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    program: &KirProgram,
) -> Result<Vec<Option<FunctionValue<'ctx>>>, CodegenError> {
    let mut functions = vec![None; program.functions.len()];
    for (id, func) in program.functions.iter().enumerate() {
        if id == program.toplevel {
            continue;
        }
        let param_types = func
            .params
            .iter()
            .map(|p| crate::layout::llvm_type(context, p.ty).map(Into::into))
            .collect::<Result<Vec<_>, _>>()?;
        let fn_type = match func.ret {
            KirType::Unit => context.void_type().fn_type(&param_types, false),
            other => crate::layout::llvm_type(context, other)?.fn_type(&param_types, false),
        };
        functions[id] = Some(module.add_function(&func.name, fn_type, None));
    }
    Ok(functions)
}

/// Emits `func`'s body into its already-declared `function_value`.
pub(crate) fn emit_function<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    functions: &[Option<FunctionValue<'ctx>>],
    func: &KirFunction,
    function_value: FunctionValue<'ctx>,
) -> Result<(), CodegenError> {
    let entry = context.append_basic_block(function_value, "entry");
    builder.position_at_end(entry);

    let mut fcx = FuncCtx {
        context,
        builder,
        function: function_value,
        functions,
        is_toplevel_in_main: false,
        locals: HashMap::new(),
    };

    for (i, param) in func.params.iter().enumerate() {
        let ty = crate::layout::llvm_type(context, param.ty)?;
        let ptr = builder
            .build_alloca(ty, &format!("local.{}", param.local))
            .map_err(llvm_err)?;
        let incoming = function_value
            .get_nth_param(u32::try_from(i).expect("param index fits u32"))
            .expect("verified KIR: param count matches the declared signature");
        builder.build_store(ptr, incoming).map_err(llvm_err)?;
        fcx.locals.insert(param.local, ptr);
    }

    stmt::emit_block(&mut fcx, &func.body)?;
    finish_block(&fcx, func.ret)
}

/// Emits `main`, running `program.toplevel`'s statements directly in its
/// body. See the module doc for the exit-code convention.
pub(crate) fn emit_main<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    functions: &[Option<FunctionValue<'ctx>>],
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
        function: main_fn,
        functions,
        is_toplevel_in_main: true,
        locals: HashMap::new(),
    };
    let last = stmt::emit_block(&mut fcx, &toplevel.body)?;

    let exit_code = match last {
        Some((BasicValueEnum::IntValue(v), KirType::I64)) => builder
            .build_int_truncate(v, i32_type, "exit_code")
            .map_err(llvm_err)?,
        _ => i32_type.const_zero(),
    };
    if !block_is_terminated(builder) {
        builder.build_return(Some(&exit_code)).map_err(llvm_err)?;
    }
    Ok(())
}

/// `true` if the builder's current insertion block already ends in a
/// terminator (a `return`, or both arms of an enclosing `if` already
/// returned) — emitting another one after it would be invalid LLVM IR.
pub(crate) fn block_is_terminated(builder: &Builder) -> bool {
    builder
        .get_insert_block()
        .is_some_and(|bb| bb.get_terminator().is_some())
}

/// Adds a fallback terminator if the function's last block fell off the end
/// without one. `keel check` requires every path through a non-`none`-typed
/// task to return (this crate does not re-derive that guarantee — see
/// AGENTS.md "trust internal invariants"), so `unreachable` is the correct
/// terminator for a scalar-returning function's fallthrough: a well-typed
/// program never actually reaches it.
fn finish_block(fcx: &FuncCtx, ret_ty: KirType) -> Result<(), CodegenError> {
    if block_is_terminated(fcx.builder) {
        return Ok(());
    }
    if ret_ty == KirType::Unit {
        fcx.builder.build_return(None).map_err(llvm_err)?;
    } else {
        fcx.builder.build_unreachable().map_err(llvm_err)?;
    }
    Ok(())
}
