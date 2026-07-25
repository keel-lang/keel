//! KIR well-formedness verifier — runs at the end of the fixed pass order
//! (`designs/llvm-compilation.md` §2.3). Checks structural invariants that
//! every later stage (codegen) is entitled to assume without re-checking:
//! every `LocalId`/`FuncId` referenced actually exists, call arity/types
//! match the callee's signature, and condition/return expressions have the
//! type their position requires.
//!
//! This intentionally does not re-derive full type inference (that already
//! happened once in `lower/expr.rs`) — it re-checks the *results* structurally,
//! which is enough to catch a lowering bug that produced an internally
//! inconsistent tree.

use std::fmt;

use crate::ir::{
    Block, CallTarget, EnumId, Expr, FuncId, KirFunction, KirProgram, LocalId, Stmt, StructId,
};
use crate::types::KirType;

#[derive(Debug, Clone)]
pub struct VerifyError {
    pub function: String,
    pub message: String,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KIR verify error in `{}`: {}",
            self.function, self.message
        )
    }
}

impl std::error::Error for VerifyError {}

/// # Errors
///
/// Returns the first structural inconsistency found.
pub fn verify(program: &KirProgram) -> Result<(), VerifyError> {
    if program.toplevel >= program.functions.len() {
        return Err(VerifyError {
            function: "<program>".to_string(),
            message: format!(
                "toplevel FuncId {} is out of range (only {} functions)",
                program.toplevel,
                program.functions.len()
            ),
        });
    }
    for (i, func) in program.functions.iter().enumerate() {
        if func.id != i {
            return Err(VerifyError {
                function: func.name.clone(),
                message: format!(
                    "function stored at index {i} but carries FuncId {}",
                    func.id
                ),
            });
        }
        verify_function(program, func)?;
    }
    Ok(())
}

fn verify_function(program: &KirProgram, func: &KirFunction) -> Result<(), VerifyError> {
    let err = |message: String| VerifyError {
        function: func.name.clone(),
        message,
    };

    for param in &func.params {
        check_local(func, param.local).map_err(err)?;
        if func.locals[param.local].ty != param.ty {
            return Err(err(format!(
                "param local {} has type {} but the signature says {}",
                param.local, func.locals[param.local].ty, param.ty
            )));
        }
    }

    verify_block(program, func, &func.body).map_err(err)?;
    Ok(())
}

fn check_local(func: &KirFunction, id: LocalId) -> Result<(), String> {
    if id >= func.locals.len() {
        return Err(format!(
            "LocalId {id} out of range ({} locals)",
            func.locals.len()
        ));
    }
    Ok(())
}

fn check_func(program: &KirProgram, id: FuncId) -> Result<(), String> {
    if id >= program.functions.len() {
        return Err(format!(
            "FuncId {id} out of range ({} functions)",
            program.functions.len()
        ));
    }
    Ok(())
}

fn check_struct(program: &KirProgram, id: StructId) -> Result<(), String> {
    if id >= program.structs.len() {
        return Err(format!(
            "StructId {id} out of range ({} structs)",
            program.structs.len()
        ));
    }
    Ok(())
}

fn check_enum(program: &KirProgram, id: EnumId) -> Result<(), String> {
    if id >= program.enums.len() {
        return Err(format!(
            "EnumId {id} out of range ({} enums)",
            program.enums.len()
        ));
    }
    Ok(())
}

fn check_list(program: &KirProgram, id: crate::ir::ListId) -> Result<(), String> {
    if id >= program.lists.len() {
        return Err(format!(
            "ListId {id} out of range ({} lists)",
            program.lists.len()
        ));
    }
    Ok(())
}

fn check_map(program: &KirProgram, id: crate::ir::MapId) -> Result<(), String> {
    if id >= program.maps.len() {
        return Err(format!(
            "MapId {id} out of range ({} maps)",
            program.maps.len()
        ));
    }
    Ok(())
}

fn check_set(program: &KirProgram, id: crate::ir::SetId) -> Result<(), String> {
    if id >= program.sets.len() {
        return Err(format!(
            "SetId {id} out of range ({} sets)",
            program.sets.len()
        ));
    }
    Ok(())
}

fn check_nullable(program: &KirProgram, id: crate::ir::NullableId) -> Result<(), String> {
    if id >= program.nullables.len() {
        return Err(format!(
            "NullableId {id} out of range ({} nullables)",
            program.nullables.len()
        ));
    }
    Ok(())
}

fn verify_block(program: &KirProgram, func: &KirFunction, block: &Block) -> Result<(), String> {
    for stmt in block {
        verify_stmt(program, func, stmt)?;
    }
    Ok(())
}

fn verify_stmt(program: &KirProgram, func: &KirFunction, stmt: &Stmt) -> Result<(), String> {
    match stmt {
        Stmt::Let { local, init } => {
            check_local(func, *local)?;
            let Some(init) = init else {
                // `init: None` — a declare-only `when`-as-expression temp
                // (issue #160); the result local's type is fixed at declare
                // time from the `when`'s first arm (or annotation), and
                // every arm's `Stmt::Assign` into it is checked below.
                return Ok(());
            };
            verify_expr(program, func, init)?;
            let declared = func.locals[*local].ty;
            if declared != init.ty() {
                return Err(format!(
                    "let-bound local {local} declared {declared} but initializer is {}",
                    init.ty()
                ));
            }
            Ok(())
        }
        Stmt::Assign { local, value } => {
            check_local(func, *local)?;
            verify_expr(program, func, value)?;
            let declared = func.locals[*local].ty;
            if declared != value.ty() {
                return Err(format!(
                    "assignment to local {local} ({declared}) has value of type {}",
                    value.ty()
                ));
            }
            Ok(())
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            verify_expr(program, func, cond)?;
            if cond.ty() != KirType::Bool {
                return Err(format!("`if` condition is {}, expected bool", cond.ty()));
            }
            verify_block(program, func, then_branch)?;
            verify_block(program, func, else_branch)
        }
        Stmt::While { cond, body } => {
            verify_expr(program, func, cond)?;
            if cond.ty() != KirType::Bool {
                return Err(format!("`while` condition is {}, expected bool", cond.ty()));
            }
            verify_block(program, func, body)
        }
        Stmt::ForIndex {
            var,
            low,
            high,
            body,
        } => {
            check_local(func, *var)?;
            verify_expr(program, func, low)?;
            verify_expr(program, func, high)?;
            let var_ty = func.locals[*var].ty;
            if var_ty != KirType::I64 {
                return Err(format!("for-loop variable {var} is {var_ty}, expected int"));
            }
            if low.ty() != KirType::I64 {
                return Err(format!(
                    "for-loop range start is {}, expected int",
                    low.ty()
                ));
            }
            if high.ty() != KirType::I64 {
                return Err(format!("for-loop range end is {}, expected int", high.ty()));
            }
            verify_block(program, func, body)
        }
        Stmt::ForEach {
            var,
            elem_ty,
            list,
            body,
        } => {
            check_local(func, *var)?;
            verify_expr(program, func, list)?;
            let KirType::List(list_id) = list.ty() else {
                return Err(format!("for-each list is {}, expected a list", list.ty()));
            };
            check_list(program, list_id)?;
            let declared_elem = program.lists[list_id];
            if declared_elem != *elem_ty {
                return Err(format!(
                    "for-each elem_ty claims {elem_ty} but the list holds {declared_elem}"
                ));
            }
            let var_ty = func.locals[*var].ty;
            if var_ty != *elem_ty {
                return Err(format!(
                    "for-each loop variable {var} is {var_ty}, expected {elem_ty}"
                ));
            }
            verify_block(program, func, body)
        }
        Stmt::Return(None) => {
            if func.ret != KirType::Unit {
                return Err(format!(
                    "bare return in a function declared to return {}",
                    func.ret
                ));
            }
            Ok(())
        }
        Stmt::Return(Some(expr)) => {
            verify_expr(program, func, expr)?;
            if expr.ty() != func.ret {
                return Err(format!(
                    "return value is {} but function returns {}",
                    expr.ty(),
                    func.ret
                ));
            }
            Ok(())
        }
        Stmt::Expr(expr) => verify_expr(program, func, expr),
        Stmt::Raise { error, .. } => {
            verify_expr(program, func, error)?;
            let KirType::Struct(struct_id) = error.ty() else {
                return Err(format!("raise error is {}, expected a struct", error.ty()));
            };
            check_struct(program, struct_id)?;
            verify_user_raised_shape(program, struct_id)
        }
        Stmt::TryCatch {
            body,
            binder,
            binder_ty,
            handler,
        } => {
            check_local(func, *binder)?;
            verify_block(program, func, body)?;
            let declared = func.locals[*binder].ty;
            if declared != *binder_ty {
                return Err(format!(
                    "catch binder {binder} is declared {declared} but binder_ty claims {binder_ty}"
                ));
            }
            let KirType::Struct(struct_id) = *binder_ty else {
                return Err(format!("catch binder_ty is {binder_ty}, expected a struct"));
            };
            check_struct(program, struct_id)?;
            verify_user_raised_shape(program, struct_id)?;
            verify_block(program, func, handler)
        }
    }
}

/// Both `Stmt::Raise`'s constructed error value and `Stmt::TryCatch`'s
/// binder must be the synthetic `UserRaised { message: str }` shape (see
/// `ir.rs`'s `Stmt::TryCatch` doc) — checked structurally rather than by a
/// hardcoded id, since `verify` has no `LowerCtx` to compare against.
fn verify_user_raised_shape(program: &KirProgram, struct_id: StructId) -> Result<(), String> {
    let layout = &program.structs[struct_id];
    match layout.fields.as_slice() {
        [(name, KirType::Str)] if name == "message" => Ok(()),
        _ => Err(format!(
            "struct `{}` used as a raise/catch error value doesn't have the expected \
             `{{ message: str }}` shape",
            layout.name
        )),
    }
}

fn verify_expr(program: &KirProgram, func: &KirFunction, expr: &Expr) -> Result<(), String> {
    match expr {
        Expr::ConstInt(_) | Expr::ConstFloat(_) | Expr::ConstBool(_) | Expr::ConstStr(_) => Ok(()),
        Expr::Local { id, ty } => {
            check_local(func, *id)?;
            if func.locals[*id].ty != *ty {
                return Err(format!(
                    "local reference {id} claims type {ty} but local is {}",
                    func.locals[*id].ty
                ));
            }
            Ok(())
        }
        Expr::BinOp { left, right, .. } => {
            verify_expr(program, func, left)?;
            verify_expr(program, func, right)
        }
        Expr::UnOp { operand, .. } => verify_expr(program, func, operand),
        Expr::Call {
            target, args, ty, ..
        } => {
            for arg in args {
                verify_expr(program, func, arg)?;
            }
            match target {
                CallTarget::Fn(id) => verify_fn_call(program, *id, args, *ty),
                CallTarget::Ns { ns_id, method_id } => {
                    verify_ns_call(*ns_id, *method_id, args, *ty)
                }
                CallTarget::Rt(rt_fn) => verify_rt_call(program, *rt_fn, args, *ty),
            }
        }
        Expr::MakeStruct { struct_id, fields } => {
            check_struct(program, *struct_id)?;
            let layout = &program.structs[*struct_id];
            if layout.fields.len() != fields.len() {
                return Err(format!(
                    "`{}` literal has {} field(s), struct declares {}",
                    layout.name,
                    fields.len(),
                    layout.fields.len()
                ));
            }
            for (field, (name, declared_ty)) in fields.iter().zip(&layout.fields) {
                verify_expr(program, func, field)?;
                if field.ty() != *declared_ty {
                    return Err(format!(
                        "`{}.{name}` is declared {declared_ty} but the literal supplies {}",
                        layout.name,
                        field.ty()
                    ));
                }
            }
            Ok(())
        }
        Expr::FieldGet {
            base,
            field_index,
            ty,
        } => {
            verify_expr(program, func, base)?;
            let KirType::Struct(struct_id) = base.ty() else {
                return Err(format!(
                    "field-get base is {}, expected a struct",
                    base.ty()
                ));
            };
            check_struct(program, struct_id)?;
            let layout = &program.structs[struct_id];
            if *field_index >= layout.fields.len() {
                return Err(format!(
                    "field-get index {field_index} out of range for `{}` ({} fields)",
                    layout.name,
                    layout.fields.len()
                ));
            }
            let declared_ty = layout.fields[*field_index].1;
            if declared_ty != *ty {
                return Err(format!(
                    "field-get on `{}.{}` claims type {ty} but the field is {declared_ty}",
                    layout.name, layout.fields[*field_index].0
                ));
            }
            Ok(())
        }
        Expr::MakeEnum {
            enum_id,
            variant_index,
        } => {
            check_enum(program, *enum_id)?;
            let layout = &program.enums[*enum_id];
            if *variant_index >= layout.variants.len() {
                return Err(format!(
                    "MakeEnum variant index {variant_index} out of range for `{}` ({} variants)",
                    layout.name,
                    layout.variants.len()
                ));
            }
            Ok(())
        }
        Expr::Index { list, index, ty } => {
            verify_expr(program, func, list)?;
            verify_expr(program, func, index)?;
            if index.ty() != KirType::I64 {
                return Err(format!("index is {}, expected int", index.ty()));
            }
            let KirType::List(list_id) = list.ty() else {
                return Err(format!("index base is {}, expected a list", list.ty()));
            };
            check_list(program, list_id)?;
            let declared_elem = program.lists[list_id];
            if declared_elem != *ty {
                return Err(format!(
                    "index claims type {ty} but the list holds {declared_elem}"
                ));
            }
            Ok(())
        }
        Expr::NullLit { ty } => {
            let KirType::Nullable(id) = ty else {
                return Err(format!("NullLit claims non-nullable type {ty}"));
            };
            check_nullable(program, *id)
        }
        Expr::NullSome { value, ty } => {
            verify_expr(program, func, value)?;
            let KirType::Nullable(id) = ty else {
                return Err(format!("NullSome claims non-nullable type {ty}"));
            };
            check_nullable(program, *id)?;
            let inner = program.nullables[*id];
            if value.ty() != inner {
                return Err(format!(
                    "NullSome wraps a {} value but claims inner type {inner}",
                    value.ty()
                ));
            }
            Ok(())
        }
        Expr::NullCoalesce {
            nullable,
            fallback,
            ty,
        } => {
            verify_expr(program, func, nullable)?;
            verify_expr(program, func, fallback)?;
            let KirType::Nullable(id) = nullable.ty() else {
                return Err(format!(
                    "`??` left-hand side is {}, expected a nullable",
                    nullable.ty()
                ));
            };
            check_nullable(program, id)?;
            let inner = program.nullables[id];
            if inner != *ty {
                return Err(format!(
                    "`??` claims type {ty} but the nullable's inner type is {inner}"
                ));
            }
            if fallback.ty() != *ty {
                return Err(format!("`??` fallback is {}, expected {ty}", fallback.ty()));
            }
            Ok(())
        }
        Expr::NullFieldGet {
            base,
            field_index,
            ty,
        } => {
            verify_expr(program, func, base)?;
            let KirType::Nullable(base_nullable_id) = base.ty() else {
                return Err(format!("`?.` base is {}, expected a nullable", base.ty()));
            };
            check_nullable(program, base_nullable_id)?;
            let KirType::Struct(struct_id) = program.nullables[base_nullable_id] else {
                return Err(format!(
                    "`?.` base's nullable inner type is {}, expected a struct",
                    program.nullables[base_nullable_id]
                ));
            };
            check_struct(program, struct_id)?;
            let layout = &program.structs[struct_id];
            if *field_index >= layout.fields.len() {
                return Err(format!(
                    "`?.` field index {field_index} out of range for `{}` ({} fields)",
                    layout.name,
                    layout.fields.len()
                ));
            }
            let field_ty = layout.fields[*field_index].1;
            let KirType::Nullable(result_nullable_id) = ty else {
                return Err(format!("`?.` claims non-nullable type {ty}"));
            };
            check_nullable(program, *result_nullable_id)?;
            let declared = program.nullables[*result_nullable_id];
            if declared != field_ty {
                return Err(format!(
                    "`?.` on `{}.{}` claims inner type {declared} but the field is {field_ty}",
                    layout.name, layout.fields[*field_index].0
                ));
            }
            Ok(())
        }
    }
}

fn verify_fn_call(
    program: &KirProgram,
    id: FuncId,
    args: &[Expr],
    ty: KirType,
) -> Result<(), String> {
    check_func(program, id)?;
    let callee = &program.functions[id];
    if callee.ret != ty {
        return Err(format!(
            "call to `{}` has result type {ty} but the callee returns {}",
            callee.name, callee.ret
        ));
    }
    if callee.params.len() != args.len() {
        return Err(format!(
            "call to `{}` passes {} arg(s), callee takes {}",
            callee.name,
            args.len(),
            callee.params.len()
        ));
    }
    for (arg, param) in args.iter().zip(&callee.params) {
        if arg.ty() != param.ty {
            return Err(format!(
                "call to `{}` passes {} where {} is expected",
                callee.name,
                arg.ty(),
                param.ty
            ));
        }
    }
    Ok(())
}

/// Verifies a `CallTarget::Ns` call shape: `(ns_id, method_id)` must
/// resolve to a real catalog method, arity must fit its param count, and
/// the call's `ty` must match what the catalog declares (via
/// `KirType::from_tyspec` — anything that maps to `None` there was already
/// rejected at lowering time, so should never reach a verified program).
fn verify_ns_call(ns_id: u16, method_id: u16, args: &[Expr], ty: KirType) -> Result<(), String> {
    let builtin = keel_catalog::catalog()
        .find(|m| {
            keel_catalog::namespace_id(m.namespace) == Some(ns_id) && m.method_id == method_id
        })
        .ok_or_else(|| {
            format!(
                "CallTarget::Ns references an unknown method (ns_id={ns_id}, method_id={method_id})"
            )
        })?;

    let required = builtin.params.iter().filter(|p| !p.optional).count();
    if args.len() < required || args.len() > builtin.params.len() {
        return Err(format!(
            "call to `{}.{}` passes {} arg(s), method takes {required}-{}",
            builtin.namespace,
            builtin.name,
            args.len(),
            builtin.params.len()
        ));
    }

    let keel_catalog::builtins::BuiltinResult::Fixed(spec) = builtin.result else {
        return Err(format!(
            "call to `{}.{}` has a runtime-context-dependent result type, not representable \
             as a KirType yet",
            builtin.namespace, builtin.name
        ));
    };
    if KirType::from_tyspec(spec) != Some(ty) {
        return Err(format!(
            "call to `{}.{}` has result type {ty} but the catalog says {spec:?}",
            builtin.namespace, builtin.name
        ));
    }
    Ok(())
}

/// Verifies a `CallTarget::Rt` call shape against the arity/type each
/// `RtFn` variant declares (`keel-codegen`'s `rt_call.rs` implements the
/// matching codegen side).
fn verify_rt_call(
    program: &KirProgram,
    rt_fn: crate::ir::RtFn,
    args: &[Expr],
    ty: KirType,
) -> Result<(), String> {
    use crate::ir::RtFn;
    match rt_fn {
        RtFn::ListNew => {
            if !args.is_empty() {
                return Err(format!("rt.list_new takes 0 args, got {}", args.len()));
            }
            let KirType::List(list_id) = ty else {
                return Err(format!("rt.list_new result is {ty}, expected a list"));
            };
            check_list(program, list_id)
        }
        RtFn::ListPush => {
            let [list, elem] = args else {
                return Err(format!("rt.list_push takes 2 args, got {}", args.len()));
            };
            let KirType::List(list_id) = list.ty() else {
                return Err(format!(
                    "rt.list_push base is {}, expected a list",
                    list.ty()
                ));
            };
            check_list(program, list_id)?;
            let elem_ty = program.lists[list_id];
            if elem.ty() != elem_ty {
                return Err(format!(
                    "rt.list_push element is {} but the list holds {elem_ty}",
                    elem.ty()
                ));
            }
            if ty != list.ty() {
                return Err(format!(
                    "rt.list_push result is {ty} but the base list is {}",
                    list.ty()
                ));
            }
            Ok(())
        }
        RtFn::ListLen => {
            let [list] = args else {
                return Err(format!("rt.list_len takes 1 arg, got {}", args.len()));
            };
            let KirType::List(list_id) = list.ty() else {
                return Err(format!(
                    "rt.list_len base is {}, expected a list",
                    list.ty()
                ));
            };
            check_list(program, list_id)?;
            if ty != KirType::I64 {
                return Err(format!("rt.list_len result is {ty}, expected int"));
            }
            Ok(())
        }
        RtFn::MapNew => {
            if !args.is_empty() {
                return Err(format!("rt.map_new takes 0 args, got {}", args.len()));
            }
            let KirType::Map(map_id) = ty else {
                return Err(format!("rt.map_new result is {ty}, expected a map"));
            };
            check_map(program, map_id)
        }
        RtFn::MapInsert => {
            let [map, key, val] = args else {
                return Err(format!("rt.map_insert takes 3 args, got {}", args.len()));
            };
            let KirType::Map(map_id) = map.ty() else {
                return Err(format!(
                    "rt.map_insert base is {}, expected a map",
                    map.ty()
                ));
            };
            check_map(program, map_id)?;
            if key.ty() != KirType::Str {
                return Err(format!(
                    "rt.map_insert key is {}, expected str (map[str, V] only)",
                    key.ty()
                ));
            }
            let value_ty = program.maps[map_id];
            if val.ty() != value_ty {
                return Err(format!(
                    "rt.map_insert value is {} but the map holds {value_ty}",
                    val.ty()
                ));
            }
            if ty != map.ty() {
                return Err(format!(
                    "rt.map_insert result is {ty} but the base map is {}",
                    map.ty()
                ));
            }
            Ok(())
        }
        RtFn::MapGet => {
            let [map, key] = args else {
                return Err(format!("rt.map_get takes 2 args, got {}", args.len()));
            };
            let KirType::Map(map_id) = map.ty() else {
                return Err(format!("rt.map_get base is {}, expected a map", map.ty()));
            };
            check_map(program, map_id)?;
            if key.ty() != KirType::Str {
                return Err(format!(
                    "rt.map_get key is {}, expected str (map[str, V] only)",
                    key.ty()
                ));
            }
            let value_ty = program.maps[map_id];
            let KirType::Nullable(nullable_id) = ty else {
                return Err(format!("rt.map_get result is {ty}, expected a nullable"));
            };
            check_nullable(program, nullable_id)?;
            if program.nullables[nullable_id] != value_ty {
                return Err(format!(
                    "rt.map_get result is {}? but the map holds {value_ty}",
                    program.nullables[nullable_id]
                ));
            }
            Ok(())
        }
        RtFn::MapLen => {
            let [map] = args else {
                return Err(format!("rt.map_len takes 1 arg, got {}", args.len()));
            };
            let KirType::Map(map_id) = map.ty() else {
                return Err(format!("rt.map_len base is {}, expected a map", map.ty()));
            };
            check_map(program, map_id)?;
            if ty != KirType::I64 {
                return Err(format!("rt.map_len result is {ty}, expected int"));
            }
            Ok(())
        }
        RtFn::MapContains => {
            let [map, key] = args else {
                return Err(format!("rt.map_contains takes 2 args, got {}", args.len()));
            };
            let KirType::Map(map_id) = map.ty() else {
                return Err(format!(
                    "rt.map_contains base is {}, expected a map",
                    map.ty()
                ));
            };
            check_map(program, map_id)?;
            if key.ty() != KirType::Str {
                return Err(format!(
                    "rt.map_contains key is {}, expected str (map[str, V] only)",
                    key.ty()
                ));
            }
            if ty != KirType::Bool {
                return Err(format!("rt.map_contains result is {ty}, expected bool"));
            }
            Ok(())
        }
        RtFn::MapKeys => {
            let [map] = args else {
                return Err(format!("rt.map_keys takes 1 arg, got {}", args.len()));
            };
            let KirType::Map(map_id) = map.ty() else {
                return Err(format!("rt.map_keys base is {}, expected a map", map.ty()));
            };
            check_map(program, map_id)?;
            let KirType::List(list_id) = ty else {
                return Err(format!("rt.map_keys result is {ty}, expected a list"));
            };
            check_list(program, list_id)?;
            if program.lists[list_id] != KirType::Str {
                return Err(format!(
                    "rt.map_keys result is list[{}], expected list[str] (map[str, V] only)",
                    program.lists[list_id]
                ));
            }
            Ok(())
        }
        RtFn::MapValues => {
            let [map] = args else {
                return Err(format!("rt.map_values takes 1 arg, got {}", args.len()));
            };
            let KirType::Map(map_id) = map.ty() else {
                return Err(format!(
                    "rt.map_values base is {}, expected a map",
                    map.ty()
                ));
            };
            check_map(program, map_id)?;
            let value_ty = program.maps[map_id];
            let KirType::List(list_id) = ty else {
                return Err(format!("rt.map_values result is {ty}, expected a list"));
            };
            check_list(program, list_id)?;
            if program.lists[list_id] != value_ty {
                return Err(format!(
                    "rt.map_values result is list[{}] but the map holds {value_ty}",
                    program.lists[list_id]
                ));
            }
            Ok(())
        }
        RtFn::SetNew => {
            if !args.is_empty() {
                return Err(format!("rt.set_new takes 0 args, got {}", args.len()));
            }
            let KirType::Set(set_id) = ty else {
                return Err(format!("rt.set_new result is {ty}, expected a set"));
            };
            check_set(program, set_id)
        }
        RtFn::SetInsert => {
            let [set, elem] = args else {
                return Err(format!("rt.set_insert takes 2 args, got {}", args.len()));
            };
            let KirType::Set(set_id) = set.ty() else {
                return Err(format!(
                    "rt.set_insert base is {}, expected a set",
                    set.ty()
                ));
            };
            check_set(program, set_id)?;
            let elem_ty = program.sets[set_id];
            if elem.ty() != elem_ty {
                return Err(format!(
                    "rt.set_insert element is {} but the set holds {elem_ty}",
                    elem.ty()
                ));
            }
            if ty != set.ty() {
                return Err(format!(
                    "rt.set_insert result is {ty} but the base set is {}",
                    set.ty()
                ));
            }
            Ok(())
        }
        RtFn::IntToStr | RtFn::FloatToStr | RtFn::BoolToStr => {
            let [value] = args else {
                return Err(format!(
                    "rt.{{int,float,bool}}_to_str takes 1 arg, got {}",
                    args.len()
                ));
            };
            let expected_arg_ty = match rt_fn {
                RtFn::IntToStr => KirType::I64,
                RtFn::FloatToStr => KirType::F64,
                RtFn::BoolToStr => KirType::Bool,
                _ => unreachable!("outer match already narrowed to the *ToStr arms"),
            };
            if value.ty() != expected_arg_ty {
                return Err(format!(
                    "rt.*_to_str argument is {} but this variant expects {expected_arg_ty}",
                    value.ty()
                ));
            }
            if ty != KirType::Str {
                return Err(format!("rt.*_to_str result is {ty}, expected str"));
            }
            Ok(())
        }
    }
}
