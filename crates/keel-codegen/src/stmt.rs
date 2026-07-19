//! Statement codegen — M1 walking-skeleton scope only: `let`/assign and bare
//! expression statements. `if`/`while`/`for`/`return` land in issue #133
//! (control flow needs basic-block wiring this crate doesn't have yet).

use inkwell::values::BasicValueEnum;

use keel_kir::ir::{Block, Stmt};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::expr;
use crate::func::FuncCtx;
use crate::layout;

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

/// Emits every statement in `block` in order. Returns the last statement's
/// value *only* if that last statement was a bare `Stmt::Expr` — see
/// `func.rs`'s module doc for why `emit_main` needs this.
pub(crate) fn emit_block<'ctx>(
    fcx: &mut FuncCtx<'ctx, '_>,
    block: &Block,
) -> Result<Option<(BasicValueEnum<'ctx>, KirType)>, CodegenError> {
    let mut last = None;
    for s in block {
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
        Stmt::Return(_) => Err(CodegenError::Unsupported(
            "return (function-level codegen lands in #133/#134)".to_string(),
        )),
        Stmt::If { .. } => Err(CodegenError::Unsupported(
            "if/else (issue #133)".to_string(),
        )),
        Stmt::While { .. } => Err(CodegenError::Unsupported("while (issue #133)".to_string())),
        Stmt::ForIndex { .. } => Err(CodegenError::Unsupported("for (issue #133)".to_string())),
    }
}
