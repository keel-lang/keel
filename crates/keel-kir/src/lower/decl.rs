//! Task declaration lowering: signature extraction (pass 1) and body
//! lowering (pass 2). See module docs on `lower/mod.rs` for the two-pass
//! rationale.

use std::collections::HashMap;

use keel_syntax::ast::TaskDecl;

use super::{FnCtx, FuncSig, LowerCtx, LowerError, binding_ident, ty_expr_to_kir};
use crate::ir::{EnumId, KirFunction, Param, StructId};
use crate::span_table::SpanTable;
use crate::types::KirType;

/// Extracts `(param_types, return_type)` from a task's AST signature.
/// Rejects generics, variadics, and default params — none are in the M0
/// scalar subset (generics need `mono.rs`; variadics/defaults need the
/// container ABI and desugaring `sugar.rs` doesn't do yet).
pub(crate) fn signature_of(
    task: &TaskDecl,
    structs_by_name: &HashMap<String, StructId>,
    enums_by_name: &HashMap<String, EnumId>,
) -> Result<(Vec<KirType>, KirType), LowerError> {
    if !task.type_params.is_empty() {
        return Err(LowerError::unsupported(
            "generic task",
            task.name_span.clone(),
        ));
    }
    let mut params = Vec::with_capacity(task.params.len());
    for param in &task.params {
        if param.variadic {
            return Err(LowerError::unsupported(
                "variadic parameter",
                param.name_span.clone(),
            ));
        }
        if param.default.is_some() {
            return Err(LowerError::unsupported(
                "default parameter value",
                param.name_span.clone(),
            ));
        }
        binding_ident(&param.name, &param.name_span)?; // rejects destructuring params
        params.push(ty_expr_to_kir(&param.ty, structs_by_name, enums_by_name)?);
    }
    let ret = match &task.return_type {
        Some(ty) => ty_expr_to_kir(ty, structs_by_name, enums_by_name)?,
        None => KirType::Unit,
    };
    Ok((params, ret))
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
        locals: ctx.locals,
        body,
    })
}
