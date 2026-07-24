//! Task declaration lowering: signature extraction (pass 1) and body
//! lowering (pass 2). See module docs on `lower/mod.rs` for the two-pass
//! rationale.

use std::collections::HashMap;

use keel_syntax::ast::TaskDecl;

use super::{FnCtx, FuncSig, LowerCtx, LowerError, binding_ident, ty_expr_to_kir};
use crate::ir::{EnumId, Expr, KirFunction, Param, StructId};
use crate::span_table::SpanTable;
use crate::types::KirType;

/// Extracts `(param_types, return_type)` from a task's AST signature.
/// Rejects generics and variadics — neither is in the M0 scalar subset
/// (generics need `mono.rs`; variadics need the container ABI). Default
/// parameter values are allowed here (just their *type*, matching the
/// declared param) — see [`lower_param_defaults`] for lowering the default
/// *expressions* themselves.
pub(crate) fn signature_of(
    task: &TaskDecl,
    structs_by_name: &HashMap<String, StructId>,
    enums_by_name: &HashMap<String, EnumId>,
    lists: &std::cell::RefCell<Vec<KirType>>,
    nullables: &std::cell::RefCell<Vec<KirType>>,
) -> Result<(Vec<KirType>, KirType), LowerError> {
    if !task.type_params.is_empty() {
        return Err(LowerError::unsupported(
            "generic task",
            task.name_span.clone(),
        ));
    }
    let mut seen_default = false;
    let mut params = Vec::with_capacity(task.params.len());
    for param in &task.params {
        if param.variadic {
            return Err(LowerError::unsupported(
                "variadic parameter",
                param.name_span.clone(),
            ));
        }
        // Call-site omitted-arg filling (`lower_call`) only fills a
        // *trailing* run of defaulted params — the checker doesn't enforce
        // this ordering itself, so KIR does, rather than silently mis-
        // filling a non-trailing default.
        if param.default.is_some() {
            seen_default = true;
        } else if seen_default {
            return Err(LowerError::unsupported(
                "a non-default parameter after a defaulted one (defaults must be trailing)",
                param.name_span.clone(),
            ));
        }
        binding_ident(&param.name, &param.name_span)?; // rejects destructuring params
        params.push(ty_expr_to_kir(
            &param.ty,
            structs_by_name,
            enums_by_name,
            lists,
            nullables,
        )?);
    }
    let ret = match &task.return_type {
        Some(ty) => ty_expr_to_kir(ty, structs_by_name, enums_by_name, lists, nullables)?,
        None => KirType::Unit,
    };
    Ok((params, ret))
}

/// Lowers each parameter's default-value expression (if any) against the
/// parameter's own declared type, once per declaration — not per call site,
/// see `lower/mod.rs`'s `param_defaults` doc. Uses a fresh, param-free
/// `FnCtx`: a default expression may not reference the task's own other
/// parameters (none of this codebase's examples need that, and it would
/// otherwise need per-call-site re-lowering instead of a lower-once cache).
pub(crate) fn lower_param_defaults(
    task: &TaskDecl,
    sig_params: &[KirType],
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Vec<Option<Expr>>, LowerError> {
    let mut ctx = FnCtx::new();
    task.params
        .iter()
        .zip(sig_params)
        .map(|(param, ty)| match &param.default {
            Some(expr) => Ok(Some(super::expr::lower_expr_expecting(
                expr, *ty, &mut ctx, lcx, table,
            )?)),
            None => Ok(None),
        })
        .collect()
}

/// Lowers a task's body given its already-computed signature.
pub(crate) fn lower_task_body(
    task: &TaskDecl,
    sig: &FuncSig,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<KirFunction, LowerError> {
    let mut ctx = FnCtx::new();
    let mut params = Vec::with_capacity(task.params.len());
    for (param, ty) in task.params.iter().zip(&sig.params) {
        let name = binding_ident(&param.name, &param.name_span)?;
        let local = ctx.declare(name, *ty);
        params.push(Param { local, ty: *ty });
    }

    let body = super::stmt::lower_block(&task.body, &mut ctx, lcx, table, sig.ret)?;

    Ok(KirFunction {
        id: sig.func_id,
        name: task.name.clone(),
        params,
        ret: sig.ret,
        // Placeholder — `lower_program`'s `compute_can_raise` fills in the
        // real value once every function's body is lowered.
        can_raise: false,
        locals: ctx.locals,
        body,
    })
}
