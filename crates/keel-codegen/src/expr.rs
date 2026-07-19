//! Expression codegen: literals, locals, arithmetic/comparison/logical
//! binary ops, unary `-`/`not`, and direct `CallTarget::Fn` calls between
//! compiled Keel functions. `str` literals and `CallTarget::Ns` (namespace
//! dispatch) are rejected — see `layout.rs` and issue #135.

use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, ValueKind};
use inkwell::{FloatPredicate, IntPredicate};

use keel_kir::ir::{BinOp, CallTarget, Expr, UnOp};
use keel_kir::types::KirType;

use crate::CodegenError;
use crate::func::FuncCtx;
use crate::layout;

fn llvm_err(e: impl std::fmt::Display) -> CodegenError {
    CodegenError::Llvm(e.to_string())
}

/// A KIR invariant this crate relies on was violated — `keel_kir::passes::
/// verify` already checks the shapes codegen assumes, so reaching one of
/// these means a codegen bug, not a bad program.
fn unreachable_combo(op: impl std::fmt::Debug, ty: KirType) -> CodegenError {
    CodegenError::Llvm(format!(
        "{op:?} on {ty} operand(s) should be unreachable — keel-kir's type inference rejects it"
    ))
}

pub(crate) fn emit_expr<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    expr: &Expr,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    match expr {
        Expr::ConstInt(v) => Ok(fcx.context.i64_type().const_int(*v as u64, true).into()),
        Expr::ConstFloat(v) => Ok(fcx.context.f64_type().const_float(*v).into()),
        Expr::ConstBool(v) => Ok(fcx
            .context
            .bool_type()
            .const_int(u64::from(*v), false)
            .into()),
        Expr::ConstStr(_) => Err(CodegenError::Unsupported(
            "str literal (issue #135)".to_string(),
        )),
        Expr::Local { id, .. } => {
            let ptr = *fcx
                .locals
                .get(id)
                .expect("verified KIR: local declared before use (passes::verify)");
            let ty = layout::llvm_type(fcx.context, expr.ty())?;
            fcx.builder.build_load(ty, ptr, "load").map_err(llvm_err)
        }
        Expr::BinOp {
            op, left, right, ..
        } => emit_binop(fcx, *op, left, right),
        Expr::UnOp { op, operand, .. } => emit_unop(fcx, *op, operand),
        Expr::Call {
            target,
            args,
            ty,
            span,
        } => emit_call(fcx, *target, args, *ty, *span),
    }
}

fn emit_call<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    target: CallTarget,
    args: &[Expr],
    ty: KirType,
    span: keel_kir::span_table::SpanId,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let func_id = match target {
        CallTarget::Fn(id) => id,
        CallTarget::Ns { ns_id, method_id } => {
            return crate::ns_call::emit_ns_call(fcx, ns_id, method_id, args, span);
        }
    };
    let callee = fcx.functions[func_id]
        .expect("declare_functions declares every non-toplevel FuncId before any body is emitted");

    let mut arg_values: Vec<BasicMetadataValueEnum> = Vec::with_capacity(args.len());
    for arg in args {
        arg_values.push(emit_expr(fcx, arg)?.into());
    }

    let call = fcx
        .builder
        .build_call(callee, &arg_values, "call")
        .map_err(llvm_err)?;

    match call.try_as_basic_value() {
        ValueKind::Basic(v) => Ok(v),
        // A Unit-returning (void) call. `ty` is Unit too (verified KIR), so
        // no caller ever inspects this value — the exit-code convention in
        // `func.rs` and every other consumer branch on `ty` first.
        ValueKind::Instruction(_) => {
            debug_assert_eq!(ty, KirType::Unit);
            Ok(fcx.context.bool_type().const_zero().into())
        }
    }
}

fn emit_binop<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    op: BinOp,
    left: &Expr,
    right: &Expr,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    // Both operands share a type by construction (`infer_binop_ty`), so the
    // left operand's KIR type picks which LLVM instructions to emit.
    let operand_ty = left.ty();
    let lv = emit_expr(fcx, left)?;
    let rv = emit_expr(fcx, right)?;
    let b = fcx.builder;

    match operand_ty {
        KirType::I64 => {
            let l = lv.into_int_value();
            let r = rv.into_int_value();
            let result: BasicValueEnum = match op {
                BinOp::Add => b.build_int_add(l, r, "add").map_err(llvm_err)?.into(),
                BinOp::Sub => b.build_int_sub(l, r, "sub").map_err(llvm_err)?.into(),
                BinOp::Mul => b.build_int_mul(l, r, "mul").map_err(llvm_err)?.into(),
                BinOp::Div => b
                    .build_int_signed_div(l, r, "sdiv")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Mod => b
                    .build_int_signed_rem(l, r, "srem")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Eq => b
                    .build_int_compare(IntPredicate::EQ, l, r, "eq")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Neq => b
                    .build_int_compare(IntPredicate::NE, l, r, "ne")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Lt => b
                    .build_int_compare(IntPredicate::SLT, l, r, "lt")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Gt => b
                    .build_int_compare(IntPredicate::SGT, l, r, "gt")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Lte => b
                    .build_int_compare(IntPredicate::SLE, l, r, "le")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Gte => b
                    .build_int_compare(IntPredicate::SGE, l, r, "ge")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::And | BinOp::Or => return Err(unreachable_combo(op, operand_ty)),
            };
            Ok(result)
        }
        KirType::F64 => {
            let l = lv.into_float_value();
            let r = rv.into_float_value();
            let result: BasicValueEnum = match op {
                BinOp::Add => b.build_float_add(l, r, "fadd").map_err(llvm_err)?.into(),
                BinOp::Sub => b.build_float_sub(l, r, "fsub").map_err(llvm_err)?.into(),
                BinOp::Mul => b.build_float_mul(l, r, "fmul").map_err(llvm_err)?.into(),
                BinOp::Div => b.build_float_div(l, r, "fdiv").map_err(llvm_err)?.into(),
                BinOp::Mod => {
                    return Err(CodegenError::Unsupported(
                        "float `%` codegen (not exercised by M1's scalar-arithmetic fixtures yet)"
                            .to_string(),
                    ));
                }
                BinOp::Eq => b
                    .build_float_compare(FloatPredicate::OEQ, l, r, "feq")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Neq => b
                    .build_float_compare(FloatPredicate::ONE, l, r, "fne")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Lt => b
                    .build_float_compare(FloatPredicate::OLT, l, r, "flt")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Gt => b
                    .build_float_compare(FloatPredicate::OGT, l, r, "fgt")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Lte => b
                    .build_float_compare(FloatPredicate::OLE, l, r, "fle")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Gte => b
                    .build_float_compare(FloatPredicate::OGE, l, r, "fge")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::And | BinOp::Or => return Err(unreachable_combo(op, operand_ty)),
            };
            Ok(result)
        }
        KirType::Bool => {
            let l = lv.into_int_value();
            let r = rv.into_int_value();
            let result: BasicValueEnum = match op {
                BinOp::Eq => b
                    .build_int_compare(IntPredicate::EQ, l, r, "beq")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::Neq => b
                    .build_int_compare(IntPredicate::NE, l, r, "bne")
                    .map_err(llvm_err)?
                    .into(),
                BinOp::And => b.build_and(l, r, "and").map_err(llvm_err)?.into(),
                BinOp::Or => b.build_or(l, r, "or").map_err(llvm_err)?.into(),
                _ => return Err(unreachable_combo(op, operand_ty)),
            };
            Ok(result)
        }
        KirType::Str => Err(CodegenError::Unsupported(
            "str concatenation/comparison (issue #135)".to_string(),
        )),
        KirType::Unit => Err(unreachable_combo(op, operand_ty)),
    }
}

fn emit_unop<'ctx>(
    fcx: &FuncCtx<'ctx, '_>,
    op: UnOp,
    operand: &Expr,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let operand_ty = operand.ty();
    let v = emit_expr(fcx, operand)?;
    match (op, operand_ty) {
        (UnOp::Neg, KirType::I64) => Ok(fcx
            .builder
            .build_int_neg(v.into_int_value(), "neg")
            .map_err(llvm_err)?
            .into()),
        (UnOp::Neg, KirType::F64) => Ok(fcx
            .builder
            .build_float_neg(v.into_float_value(), "fneg")
            .map_err(llvm_err)?
            .into()),
        (UnOp::Not, KirType::Bool) => Ok(fcx
            .builder
            .build_not(v.into_int_value(), "not")
            .map_err(llvm_err)?
            .into()),
        _ => Err(unreachable_combo(op, operand_ty)),
    }
}
