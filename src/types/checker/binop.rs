//! Binary-operator type rules.
//!
//! Provides two `pub(crate)` helpers used by the checker's statement and
//! expression passes:
//!
//! - [`check_binop`] — validates that an operator can be applied to two types
//!   and returns an error string if not.
//! - [`infer_binary`] — infers the result type of a binary expression.

use crate::ast::BinOp;
use crate::types::ty::{describe_ty, Ty};

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

// ---------------------------------------------------------------------------
// Public(crate) API
// ---------------------------------------------------------------------------

/// Validate that `op` can be applied to operands of type `l` and `r`.
///
/// Returns `Some(message)` when the combination is invalid, `None` when it is
/// valid.  `Ty::Unknown` and `Ty::Dynamic` operands are always accepted so
/// that an earlier type error does not produce a cascade of binary-op errors.
pub(crate) fn check_binop(op: BinOp, l: &Ty, r: &Ty) -> Option<String> {
    let lb = l.strip_nullable();
    let rb = r.strip_nullable();

    if matches!(lb, Ty::Unknown | Ty::Dynamic) || matches!(rb, Ty::Unknown | Ty::Dynamic) {
        return None;
    }

    let ok = match op {
        BinOp::Add => {
            matches!(
                (lb, rb),
                (Ty::Int, Ty::Int)
                    | (Ty::Float, Ty::Float)
                    | (Ty::Int, Ty::Float)
                    | (Ty::Float, Ty::Int)
                    | (Ty::Str, Ty::Str)
                    | (Ty::Datetime, Ty::Duration)
                    | (Ty::Duration, Ty::Datetime)
                    | (Ty::Duration, Ty::Duration)
            ) || matches!((lb, rb), (Ty::List(_), Ty::List(_)))
        }

        BinOp::Sub => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
                | (Ty::Datetime, Ty::Duration)
                | (Ty::Datetime, Ty::Datetime)
                | (Ty::Duration, Ty::Duration)
        ),

        BinOp::Mul | BinOp::Div | BinOp::Mod => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
        ),

        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
                | (Ty::Str, Ty::Str)
                | (Ty::Datetime, Ty::Datetime)
                | (Ty::Duration, Ty::Duration)
        ),

        BinOp::Eq | BinOp::Neq | BinOp::And | BinOp::Or => true,
    };

    if ok {
        None
    } else {
        Some(format!(
            "cannot apply `{}` to {} and {}",
            op_symbol(op),
            describe_ty(lb),
            describe_ty(rb)
        ))
    }
}

/// Infer the result type of applying `op` to operands of type `l` and `r`.
///
/// Nullable wrappers are stripped before matching; the result is the base
/// type of the operation (callers may re-wrap in `Ty::Nullable` if needed).
pub(crate) fn infer_binary(op: BinOp, l: &Ty, r: &Ty) -> Ty {
    use BinOp::*;
    let lb = l.strip_nullable();
    let rb = r.strip_nullable();
    match op {
        Add | Sub | Mul | Div | Mod => match (lb, rb) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) => Ty::Float,
            (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Str, Ty::Str) if matches!(op, Add) => Ty::Str,
            (Ty::List(le), Ty::List(_)) if matches!(op, Add) => Ty::List(le.clone()),
            _ => Ty::Unknown,
        },
        Eq | Neq | Lt | Gt | Lte | Gte => Ty::Bool,
        And | Or => Ty::Bool,
    }
}
