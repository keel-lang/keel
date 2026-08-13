//! Call-site argument checking for the type checker.
//!
//! Validates that inferred argument types match declared parameter types,
//! handling positional, named, variadic, and spread arguments.

use crate::ast::CallArg;
use crate::types::ty::{Ty, describe_ty};

use super::Checker;

impl Checker<'_, '_> {
    /// Check inferred argument types against declared parameter types.
    ///
    /// Positional args fill params in order; named args match by param name
    /// (mirroring the interpreter's Python-style keyword-argument convention).
    /// When `variadic` is true the last param is a rest-parameter (`...name: T`):
    ///   - plain positional args beyond the fixed params are each checked as `T`
    ///   - spread args (`...expr`) must be `list[T]` or `set[T]`
    ///
    /// `required` is parallel to `params`: a `true` entry with no matching
    /// named or positional arg is a missing-argument error (issue #235) —
    /// previously this silently `continue`d, so the interpreter's own
    /// param-binding fallback (`Value::None` for a param with no default)
    /// was the only thing that ran, undetected by `keel check`. `span` is
    /// the call site's own span, used for that diagnostic since a missing
    /// argument has no argument-level span of its own to point at.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_call_args(
        &mut self,
        params: &[(String, Ty)],
        required: &[bool],
        variadic: bool,
        args: &[CallArg],
        arg_tys: &[Ty],
        callee: &str,
        span: crate::lexer::Span,
    ) {
        if !variadic && args.iter().any(|a| a.spread) {
            self.err(format!(
                "{callee}: spread args (`...`) require a variadic callee"
            ));
            return;
        }
        let named: std::collections::HashMap<&str, (&Ty, &CallArg)> = args
            .iter()
            .zip(arg_tys.iter())
            .filter_map(|(a, ty)| a.name.as_deref().map(|n| (n, (ty, a))))
            .collect();
        // Plain positional args — not named, not spread.
        let positional: Vec<(&Ty, &CallArg)> = args
            .iter()
            .zip(arg_tys.iter())
            .filter(|(a, _)| a.name.is_none() && !a.spread)
            .map(|(arg, ty)| (ty, arg))
            .collect();

        let fixed_params = if variadic && !params.is_empty() {
            &params[..params.len() - 1]
        } else {
            params
        };
        let fixed_required = &required[..fixed_params.len()];

        let mut pos_idx = 0;
        for ((param_name, param_ty), is_required) in fixed_params.iter().zip(fixed_required) {
            let (arg_ty, arg) = if let Some((ty, arg)) = named.get(param_name.as_str()) {
                (*ty, *arg)
            } else if let Some((ty, arg)) = positional.get(pos_idx) {
                pos_idx += 1;
                (*ty, *arg)
            } else {
                if *is_required {
                    self.err_at(
                        format!("{callee}: missing required argument `{param_name}`"),
                        span.clone(),
                    );
                }
                continue;
            };
            self.expect_at(
                arg_ty,
                param_ty,
                &format!("{callee} arg `{param_name}`"),
                arg.value.span.clone(),
            );
        }

        if variadic && let Some((var_name, elem_ty)) = params.last() {
            // Check each remaining plain positional arg against the element type.
            for (arg_ty, arg) in positional.iter().skip(pos_idx) {
                self.expect_at(
                    arg_ty,
                    elem_ty,
                    &format!("{callee} variadic arg `{var_name}`"),
                    arg.value.span.clone(),
                );
            }
            // Check spread args: each must be list[T] or set[T].
            // Use err_at with the argument's own span so the diagnostic points
            // to the specific spread expression, not to the enclosing statement.
            for (a, arg_ty) in args.iter().zip(arg_tys.iter()).filter(|(a, _)| a.spread) {
                let expected_list = Ty::List(Box::new(elem_ty.clone()));
                let expected_set = Ty::Set(Box::new(elem_ty.clone()));
                let ok = match arg_ty {
                    Ty::List(inner) | Ty::Set(inner) => self.types_match(inner.as_ref(), elem_ty),
                    _ => false,
                };
                if !ok {
                    self.err_at(
                        format!(
                            "{callee}: spread arg `...` must be `{}` or `{}`, got `{}`",
                            describe_ty(&expected_list),
                            describe_ty(&expected_set),
                            describe_ty(arg_ty),
                        ),
                        a.value.span.clone(),
                    );
                }
            }
        }
    }
}
