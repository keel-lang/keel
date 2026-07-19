//! Statement codegen: `let`/assign, bare expressions, `if`/`else`, `while`,
//! `for` (indexed range loop), and `return`. Control flow builds real LLVM
//! basic blocks and relies on `mem2reg` to recover SSA form from the
//! `alloca`-based locals later — see `func.rs`'s module doc.

use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

use keel_kir::ir::{Block, Stmt};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::expr;
use crate::func::{FuncCtx, block_is_terminated, llvm_err};
use crate::layout;

/// Emits every statement in `block` in order. Returns the last statement's
/// value *only* if that last statement was a bare `Stmt::Expr` — see
/// `func.rs`'s module doc for why `emit_main` needs this. Stops emitting
/// (but keeps returning `Ok`) once the current block is terminated: any
/// statement after a `return` is unreachable and — since KIR is structured,
/// not CFG-based — dead code that would otherwise try to build IR after a
/// terminator, which LLVM rejects.
pub(crate) fn emit_block<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    block: &Block,
) -> Result<Option<(BasicValueEnum<'ctx>, KirType)>, CodegenError> {
    let mut last = None;
    for s in block {
        if block_is_terminated(fcx.builder) {
            break;
        }
        last = emit_stmt(fcx, s)?;
    }
    Ok(last)
}

fn emit_stmt<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    stmt: &Stmt,
) -> Result<Option<(BasicValueEnum<'ctx>, KirType)>, CodegenError> {
    match stmt {
        Stmt::Let { local, init } => {
            let value = expr::emit_expr(fcx, init)?;
            let ty = layout::llvm_type(fcx.context, init.ty())?;
            let ptr = fcx
                .builder
                .build_alloca(ty, &format!("local.{local}"))
                .map_err(llvm_err)?;
            fcx.builder.build_store(ptr, value).map_err(llvm_err)?;
            fcx.locals.insert(*local, ptr);
            Ok(None)
        }
        Stmt::Assign { local, value } => {
            let v = expr::emit_expr(fcx, value)?;
            let ptr = *fcx
                .locals
                .get(local)
                .expect("verified KIR: local exists before assignment (passes::verify)");
            fcx.builder.build_store(ptr, v).map_err(llvm_err)?;
            Ok(None)
        }
        Stmt::Expr(e) => Ok(Some((expr::emit_expr(fcx, e)?, e.ty()))),
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            emit_if(fcx, cond, then_branch, else_branch)?;
            Ok(None)
        }
        Stmt::While { cond, body } => {
            emit_while(fcx, cond, body)?;
            Ok(None)
        }
        Stmt::ForIndex {
            var,
            low,
            high,
            body,
        } => {
            emit_for_index(fcx, *var, low, high, body)?;
            Ok(None)
        }
        Stmt::Return(value) => {
            emit_return(fcx, value.as_ref())?;
            Ok(None)
        }
    }
}

fn emit_if<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    cond: &keel_kir::ir::Expr,
    then_branch: &Block,
    else_branch: &Block,
) -> Result<(), CodegenError> {
    let cond_v = expr::emit_expr(fcx, cond)?.into_int_value();

    let then_bb = fcx.context.append_basic_block(fcx.function, "if.then");
    let else_bb = fcx.context.append_basic_block(fcx.function, "if.else");
    let merge_bb = fcx.context.append_basic_block(fcx.function, "if.merge");

    fcx.builder
        .build_conditional_branch(cond_v, then_bb, else_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(then_bb);
    emit_block(fcx, then_branch)?;
    if !block_is_terminated(fcx.builder) {
        fcx.builder
            .build_unconditional_branch(merge_bb)
            .map_err(llvm_err)?;
    }

    fcx.builder.position_at_end(else_bb);
    emit_block(fcx, else_branch)?;
    if !block_is_terminated(fcx.builder) {
        fcx.builder
            .build_unconditional_branch(merge_bb)
            .map_err(llvm_err)?;
    }

    fcx.builder.position_at_end(merge_bb);
    Ok(())
}

fn emit_while<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    cond: &keel_kir::ir::Expr,
    body: &Block,
) -> Result<(), CodegenError> {
    let cond_bb = fcx.context.append_basic_block(fcx.function, "while.cond");
    let body_bb = fcx.context.append_basic_block(fcx.function, "while.body");
    let end_bb = fcx.context.append_basic_block(fcx.function, "while.end");

    fcx.builder
        .build_unconditional_branch(cond_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(cond_bb);
    let cond_v = expr::emit_expr(fcx, cond)?.into_int_value();
    fcx.builder
        .build_conditional_branch(cond_v, body_bb, end_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(body_bb);
    emit_block(fcx, body)?;
    if !block_is_terminated(fcx.builder) {
        fcx.builder
            .build_unconditional_branch(cond_bb)
            .map_err(llvm_err)?;
    }

    fcx.builder.position_at_end(end_bb);
    Ok(())
}

/// `for var in low..high { body }` — both bounds inclusive (matches the
/// interpreter's `Value::Range`, see `keel-kir`'s `ForIndex` doc). `low`/
/// `high` are evaluated once, before the loop; `var`'s `alloca` is
/// (re-)initialized to `low` every time this statement is *reached*
/// (correct even when nested inside an outer loop — a fixed-size `alloca`
/// is one stack slot for the whole function call, not one per dynamic
/// execution, so re-running this statement just re-stores into the same
/// slot).
fn emit_for_index<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    var: keel_kir::ir::LocalId,
    low: &keel_kir::ir::Expr,
    high: &keel_kir::ir::Expr,
    body: &Block,
) -> Result<(), CodegenError> {
    let low_v = expr::emit_expr(fcx, low)?.into_int_value();
    let high_v = expr::emit_expr(fcx, high)?.into_int_value();

    let i64_type = fcx.context.i64_type();
    let var_ptr = fcx
        .builder
        .build_alloca(i64_type, &format!("local.{var}"))
        .map_err(llvm_err)?;
    fcx.builder.build_store(var_ptr, low_v).map_err(llvm_err)?;
    fcx.locals.insert(var, var_ptr);

    let cond_bb = fcx.context.append_basic_block(fcx.function, "for.cond");
    let body_bb = fcx.context.append_basic_block(fcx.function, "for.body");
    let end_bb = fcx.context.append_basic_block(fcx.function, "for.end");

    fcx.builder
        .build_unconditional_branch(cond_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(cond_bb);
    let cur = fcx
        .builder
        .build_load(i64_type, var_ptr, "for.cur")
        .map_err(llvm_err)?
        .into_int_value();
    let cond_v = fcx
        .builder
        .build_int_compare(IntPredicate::SLE, cur, high_v, "for.test")
        .map_err(llvm_err)?;
    fcx.builder
        .build_conditional_branch(cond_v, body_bb, end_bb)
        .map_err(llvm_err)?;

    fcx.builder.position_at_end(body_bb);
    emit_block(fcx, body)?;
    if !block_is_terminated(fcx.builder) {
        let cur = fcx
            .builder
            .build_load(i64_type, var_ptr, "for.cur")
            .map_err(llvm_err)?
            .into_int_value();
        let next = fcx
            .builder
            .build_int_add(cur, i64_type.const_int(1, false), "for.next")
            .map_err(llvm_err)?;
        fcx.builder.build_store(var_ptr, next).map_err(llvm_err)?;
        fcx.builder
            .build_unconditional_branch(cond_bb)
            .map_err(llvm_err)?;
    }

    fcx.builder.position_at_end(end_bb);
    Ok(())
}

fn emit_return<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    value: Option<&keel_kir::ir::Expr>,
) -> Result<(), CodegenError> {
    if fcx.is_toplevel_in_main {
        return Err(CodegenError::Unsupported(
            "`return` in top-level code (M1 walking-skeleton scope — put it in a task)".to_string(),
        ));
    }
    match value {
        Some(e) => {
            let v = expr::emit_expr(fcx, e)?;
            fcx.builder.build_return(Some(&v)).map_err(llvm_err)?;
        }
        None => {
            fcx.builder.build_return(None).map_err(llvm_err)?;
        }
    }
    Ok(())
}
