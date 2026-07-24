//! Function emission.
//!
//! Every `KirFunction`, including the synthetic `toplevel` one, compiles to
//! a real, separate LLVM function — `emit_function` for ordinary functions,
//! `emit_toplevel_function` for `toplevel` (named `keel_user_toplevel`, the
//! symbol `keel-rt-ffi`'s `keel_rt_start` calls back into). `toplevel`'s KIR
//! return type is always `Unit` (real top-level Keel statements have no
//! `return <value>` to give one), so `keel_user_toplevel` keeps the M1
//! "bare `int` expression -> process exit code" convention from the
//! walking-skeleton (issue #132) as its *own* return value instead of
//! `main`'s: a bare top-level `int` expression becomes the exit code, a bare
//! `return` (with no value — the only form `keel_user_toplevel`'s KIR can
//! contain) exits with code `0`, and falling off the end without either
//! defaults to `0` too. `main` itself (`emit_main`) is now just a thin
//! wrapper that calls `keel_rt_start` and returns its result — see
//! `designs/llvm-compilation.md` §2.6 and issue #134.
//!
//! # Basic-block wiring
//!
//! `stmt.rs` builds real LLVM basic blocks for `if`/`while`/`for` (branches,
//! loop header/body/exit) and relies on LLVM's `mem2reg` to turn the
//! `alloca`-based locals back into SSA registers later, per
//! `designs/llvm-compilation.md` §2.3's structured-IR rationale — this crate
//! does no manual CFG/SSA construction of its own.

use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicType;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue, ValueKind};

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
    pub(crate) module: &'a Module<'ctx>,
    pub(crate) builder: &'a Builder<'ctx>,
    /// The whole program — needed by `layout::llvm_type`/`struct_layout_type`
    /// to resolve a `KirType::Struct`'s field layout.
    pub(crate) program: &'a KirProgram,
    /// The LLVM function currently being emitted into — needed to attach
    /// new basic blocks (`if`/`while`/`for` wiring).
    pub(crate) function: FunctionValue<'ctx>,
    /// Every non-`toplevel` `KirFunction`'s compiled `FunctionValue`,
    /// indexed by `FuncId`, resolved up front (before any body is emitted)
    /// so forward/mutual/recursive `CallTarget::Fn` calls resolve. The
    /// `toplevel` slot is `None` — nothing ever calls it by `FuncId`
    /// (`keel-rt-ffi` calls it by symbol name, `keel_user_toplevel`).
    pub(crate) functions: &'a [Option<FunctionValue<'ctx>>],
    /// `true` while emitting `toplevel`'s body as `keel_user_toplevel`. A
    /// bare `return` there (the only form `toplevel`'s KIR can contain —
    /// see the module doc) means "exit 0 now" instead of `ret void`.
    pub(crate) is_toplevel: bool,
    pub(crate) locals: HashMap<LocalId, PointerValue<'ctx>>,
    /// This function's own logical return type (`KirFunction::ret`) — needed
    /// by `raise.rs`'s `emit_ok_return` to box a `return`/fallthrough value
    /// into the result-ABI's payload slot.
    pub(crate) ret_ty: KirType,
    /// Whether this function returns the result-ABI wrapper instead of a
    /// plain `ret_ty`-typed value (`KirFunction::can_raise` — see its doc
    /// and `designs/llvm-compilation.md` §2.5). Always `false` for
    /// `toplevel` (`lower_program` rejects a `can_raise` toplevel).
    pub(crate) can_raise: bool,
    /// Stack of currently active `try` scopes in this function, innermost
    /// last: `(handler basic block, catch binder's already-allocated
    /// `alloca`)`. A `can_raise` call's `is_err` branch
    /// (`expr.rs`'s `emit_call`) jumps to the top entry's handler block
    /// (storing the error payload into its binder first) if non-empty, or
    /// propagates via this function's own error return otherwise — see
    /// `stmt.rs`'s `emit_try_catch`.
    pub(crate) catch_stack: Vec<(BasicBlock<'ctx>, PointerValue<'ctx>)>,
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
            .map(|p| crate::layout::llvm_type(context, program, p.ty).map(Into::into))
            .collect::<Result<Vec<_>, _>>()?;
        let fn_type = if func.can_raise {
            crate::raise::result_abi_type(context).fn_type(&param_types, false)
        } else {
            match func.ret {
                KirType::Unit => context.void_type().fn_type(&param_types, false),
                other => {
                    crate::layout::llvm_type(context, program, other)?.fn_type(&param_types, false)
                }
            }
        };
        functions[id] = Some(module.add_function(&func.name, fn_type, None));
    }
    Ok(functions)
}

/// Emits `func`'s body into its already-declared `function_value`.
pub(crate) fn emit_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    functions: &[Option<FunctionValue<'ctx>>],
    program: &KirProgram,
    func: &KirFunction,
    function_value: FunctionValue<'ctx>,
) -> Result<(), CodegenError> {
    let entry = context.append_basic_block(function_value, "entry");
    builder.position_at_end(entry);

    let mut fcx = FuncCtx {
        context,
        module,
        builder,
        program,
        function: function_value,
        functions,
        is_toplevel: false,
        locals: HashMap::new(),
        ret_ty: func.ret,
        can_raise: func.can_raise,
        catch_stack: Vec::new(),
    };

    for (i, param) in func.params.iter().enumerate() {
        let ty = crate::layout::llvm_type(context, program, param.ty)?;
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

/// Emits `program.toplevel`'s body as a real function named
/// `keel_user_toplevel` — the symbol `keel-rt-ffi`'s `keel_rt_start` calls
/// back into once it has booted the runtime. See the module doc for the
/// exit-code convention this function's `i32` return value follows.
pub(crate) fn emit_toplevel_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    functions: &[Option<FunctionValue<'ctx>>],
    program: &KirProgram,
) -> Result<(), CodegenError> {
    let i32_type = context.i32_type();
    let fn_type = i32_type.fn_type(&[], false);
    let toplevel_fn = module.add_function("keel_user_toplevel", fn_type, None);
    let entry = context.append_basic_block(toplevel_fn, "entry");
    builder.position_at_end(entry);

    let toplevel = &program.functions[program.toplevel];
    let mut fcx = FuncCtx {
        context,
        module,
        builder,
        program,
        function: toplevel_fn,
        functions,
        is_toplevel: true,
        locals: HashMap::new(),
        ret_ty: KirType::Unit,
        // `lower_program` rejects a `can_raise` toplevel (an uncaught raise
        // reaching the top level is a later M2/M3 concern — see its doc) —
        // this is always `false` in practice, never read.
        can_raise: false,
        catch_stack: Vec::new(),
    };
    let last = stmt::emit_block(&mut fcx, &toplevel.body)?;

    if !block_is_terminated(builder) {
        let exit_code = match last {
            Some((BasicValueEnum::IntValue(v), KirType::I64)) => builder
                .build_int_truncate(v, i32_type, "exit_code")
                .map_err(llvm_err)?,
            _ => i32_type.const_zero(),
        };
        builder.build_return(Some(&exit_code)).map_err(llvm_err)?;
    }
    Ok(())
}

/// Emits `main` as a thin wrapper around `keel_rt_start` (defined in
/// `keel-rt-ffi`, linked in via `BuildOptions::runtime_link_args`) — it boots
/// the runtime and calls back into `keel_user_toplevel`, and `main` just
/// propagates whatever it returns as the process exit code.
pub(crate) fn emit_main<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<(), CodegenError> {
    let i32_type = context.i32_type();
    let no_args_i32 = i32_type.fn_type(&[], false);

    let main_fn = module.add_function("main", no_args_i32, None);
    let entry = context.append_basic_block(main_fn, "entry");
    builder.position_at_end(entry);

    let rt_start_fn = module.add_function("keel_rt_start", no_args_i32, None);
    let call = builder
        .build_call(rt_start_fn, &[], "rt_start")
        .map_err(llvm_err)?;
    let result = match call.try_as_basic_value() {
        ValueKind::Basic(v) => v,
        ValueKind::Instruction(_) => unreachable!("keel_rt_start returns i32, never void"),
    };
    builder.build_return(Some(&result)).map_err(llvm_err)?;
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
/// program never actually reaches it. A `can_raise` function's `ret_ty ==
/// Unit` fallthrough still needs a real value, though — its LLVM return
/// type is the result-ABI struct, not `void` — so that case emits the
/// success branch instead of a bare `ret void`.
fn finish_block(fcx: &FuncCtx, ret_ty: KirType) -> Result<(), CodegenError> {
    if block_is_terminated(fcx.builder) {
        return Ok(());
    }
    if fcx.can_raise {
        if ret_ty == KirType::Unit {
            crate::raise::emit_ok_return(fcx, KirType::Unit, None)?;
        } else {
            fcx.builder.build_unreachable().map_err(llvm_err)?;
        }
    } else if ret_ty == KirType::Unit {
        fcx.builder.build_return(None).map_err(llvm_err)?;
    } else {
        fcx.builder.build_unreachable().map_err(llvm_err)?;
    }
    Ok(())
}
