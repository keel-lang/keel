//! Call-site argument checking for the type checker.
//!
//! Validates that inferred argument types match declared parameter types,
//! handling positional, named, variadic, and spread arguments.

use std::collections::HashMap;

use crate::ast::CallArg;
use crate::types::ty::{describe_ty, Ty};

use super::Checker;

impl Checker {
    /// Check inferred argument types against declared parameter types.
    ///
    /// Positional args fill params in order; named args match by param name
    /// (mirroring the interpreter's Python-style keyword-argument convention).
    /// When `variadic` is true the last param is a rest-parameter (`...name: T`):
    ///   - plain positional args beyond the fixed params are each checked as `T`
    ///   - spread args (`...expr`) must be `list[T]` or `set[T]`
    pub(crate) fn check_call_args(
        &mut self,
        params: &[(String, Ty)],
        variadic: bool,
        args: &[CallArg],
        arg_tys: &[Ty],
        callee: &str,
    ) {
        if !variadic && args.iter().any(|a| a.spread) {
            self.err(format!(
                "{callee}: spread args (`...`) require a variadic callee"
            ));
            return;
        }
        let named: HashMap<&str, &Ty> = args
            .iter()
            .zip(arg_tys.iter())
            .filter_map(|(a, ty)| a.name.as_deref().map(|n| (n, ty)))
            .collect();
        // Plain positional args — not named, not spread.
        let positional: Vec<&Ty> = args
            .iter()
            .zip(arg_tys.iter())
            .filter(|(a, _)| a.name.is_none() && !a.spread)
            .map(|(_, ty)| ty)
            .collect();

        let fixed_params = if variadic && !params.is_empty() {
            &params[..params.len() - 1]
        } else {
            params
        };

        let mut pos_idx = 0;
        for (param_name, param_ty) in fixed_params {
            let arg_ty = if let Some(ty) = named.get(param_name.as_str()) {
                *ty
            } else if let Some(ty) = positional.get(pos_idx) {
                pos_idx += 1;
                *ty
            } else {
                continue;
            };
            self.expect(arg_ty, param_ty, &format!("{callee} arg `{param_name}`"));
        }

        if variadic && let Some((var_name, elem_ty)) = params.last() {
            // Check each remaining plain positional arg against the element type.
            for arg_ty in positional.iter().skip(pos_idx) {
                self.expect(
                    arg_ty,
                    elem_ty,
                    &format!("{callee} variadic arg `{var_name}`"),
                );
            }
            // Check spread args: each must be list[T] or set[T].
            for (_a, arg_ty) in args.iter().zip(arg_tys.iter()).filter(|(a, _)| a.spread) {
                let expected_list = Ty::List(Box::new(elem_ty.clone()));
                let expected_set = Ty::Set(Box::new(elem_ty.clone()));
                let ok = match arg_ty {
                    Ty::List(inner) | Ty::Set(inner) => self.types_match(inner.as_ref(), elem_ty),
                    _ => false,
                };
                if !ok {
                    self.err(format!(
                        "{callee}: spread arg `...` must be `{}` or `{}`, got `{}`",
                        describe_ty(&expected_list),
                        describe_ty(&expected_set),
                        describe_ty(arg_ty),
                    ));
                }
            }
        }
    }
}
