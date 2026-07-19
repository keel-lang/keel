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

use crate::ir::{Block, CallTarget, Expr, FuncId, KirFunction, KirProgram, LocalId, Stmt};
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
            }
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
