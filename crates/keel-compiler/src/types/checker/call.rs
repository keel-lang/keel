//! Call-site argument checking for the type checker.
//!
//! Validates that inferred argument types match declared parameter types,
//! handling positional, named, variadic, and spread arguments.

use crate::ast::CallArg;
use crate::builtins::{BuiltinMethod, BuiltinParam, ParamBinding, TySpec};
use crate::types::prelude::ty_from_spec;
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

    /// Check a call whose callee resolved to a lambda literal's own
    /// `Ty::Func` — a binding `Scope::is_exact_lambda` confirmed is exact,
    /// or an immediately-invoked lambda literal (see both call sites).
    /// Unlike a task, `LambdaParam` has no default, named, or
    /// variadic/spread concept, so every parameter is required and matched
    /// strictly by position — a call omitting one previously passed `keel
    /// check` silently and let the interpreter's own param-binding
    /// fallback bind it to `Value::None` (issue #238).
    pub(crate) fn check_closure_call_arity(
        &mut self,
        expected: usize,
        args: &[CallArg],
        span: crate::lexer::Span,
    ) {
        // A spread arg's expanded length isn't known statically — e.g.
        // `add(...pair)` is a valid call today when `pair` has exactly
        // `expected` elements, since argument evaluation flattens it into
        // positional runtime args before the closure ever binds them.
        // Skip the check rather than reject an already-working call shape.
        if args.iter().any(|a| a.spread) {
            return;
        }
        if args.len() != expected {
            self.err_at(
                format!(
                    "closure call: expected {expected} argument(s), got {}",
                    args.len()
                ),
                span,
            );
        }
    }

    /// Validate a `std/*` namespace call's arguments against its catalog
    /// entry's declared `params` (issue #222).
    ///
    /// Each param's [`ParamBinding`] records how the runtime actually looks
    /// it up: `PositionalOnly` reads it via `positional(args, idx)`, where
    /// `idx` is the param's rank among `PositionalOnly`/`Either` params in
    /// declared order (named args are filtered out before that index is
    /// taken, so it does not shift no matter what else is passed by name);
    /// `NamedOnly` reads it via `find_arg(args, name)` with no positional
    /// fallback; `Either` accepts both. So positional args and named args
    /// are validated as two independent passes here, exactly mirroring how
    /// the runtime's two lookup helpers never consult each other.
    ///
    /// A method whose catalog entry declares zero params is skipped
    /// entirely — that means either a true zero-arg method or one not
    /// statically validated yet (`ai.embed`'s free-form input, `db.query`'s
    /// driver-specific args). A `TySpec::Callback` param (trailing
    /// task/closure argument) needs no special-casing: `ty_from_spec` maps
    /// it to an opaque `Ty::Unknown`, which `expect_at` already skips.
    ///
    /// A method whose params are *all* `NamedOnly` (`http.request`) can
    /// additionally be called with a single positional map/struct literal
    /// as sugar for all of its named args at once
    /// (`http.request({method: "GET", url: "..."})`,
    /// `crates/keel-runtime/src/runtime/namespaces/http.rs`'s `"request"`
    /// handler) — a calling convention `ParamBinding` has no way to
    /// represent, since it is not any one param's binding but the whole
    /// call's shape. Recognized structurally (one struct-typed positional
    /// arg, no named args, every declared param `NamedOnly`) rather than a
    /// hardcoded namespace/method list, so it is not tied to `http.request`
    /// specifically. The struct literal's individual keys are left for the
    /// runtime to validate, same as any other value of statically-unknown
    /// shape.
    pub(crate) fn check_builtin_args(
        &mut self,
        entry: &'static BuiltinMethod,
        args: &[CallArg],
        arg_tys: &[Ty],
        span: crate::lexer::Span,
    ) {
        let params = entry.params;
        if params.is_empty() {
            return;
        }
        if let [arg] = args
            && arg.name.is_none()
            && matches!(arg_tys.first(), Some(Ty::Struct { .. }))
            && params.iter().all(|p| p.binding == ParamBinding::NamedOnly)
        {
            return;
        }
        let callee = format!("`{}.{}`", entry.namespace, entry.name);

        if args.iter().any(|a| a.spread) {
            self.err_at(
                format!("{callee}: spread args (`...`) are not supported here"),
                span,
            );
            return;
        }

        let mut covered: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut misplaced: std::collections::HashSet<&str> = std::collections::HashSet::new();

        // Positional pass: zip the call's unnamed args against the params
        // eligible for positional binding, in declared order.
        let positional_params: Vec<_> = params
            .iter()
            .filter(|p| p.binding != ParamBinding::NamedOnly)
            .collect();
        let positional_args: Vec<(&Ty, &CallArg)> = args
            .iter()
            .zip(arg_tys.iter())
            .filter(|(a, _)| a.name.is_none())
            .map(|(a, ty)| (ty, a))
            .collect();

        if positional_args.len() > positional_params.len() {
            self.err_at(
                format!(
                    "{callee} takes {} positional argument(s), got {}",
                    positional_params.len(),
                    positional_args.len()
                ),
                span.clone(),
            );
        }

        for (param, (ty, arg)) in positional_params.iter().zip(positional_args.iter()) {
            self.expect_builtin_arg(
                ty,
                param,
                &format!("{callee} arg `{}`", param.name),
                arg.value.span.clone(),
            );
            covered.insert(param.name);
        }

        // Named pass: each name must match a declared param that accepts a
        // named form — a `PositionalOnly` param is never read by name.
        for (arg, ty) in args.iter().zip(arg_tys.iter()) {
            let Some(name) = arg.name.as_deref() else {
                continue;
            };
            let Some(param) = params.iter().find(|p| p.name == name) else {
                self.err_at(
                    format!("{callee}: unknown argument `{name}:`"),
                    arg.value.span.clone(),
                );
                continue;
            };
            if param.binding == ParamBinding::PositionalOnly {
                self.err_at(
                    format!("{callee}: `{name}` must be passed positionally, not as `{name}:`"),
                    arg.value.span.clone(),
                );
                misplaced.insert(name);
                continue;
            }
            self.expect_builtin_arg(
                ty,
                param,
                &format!("{callee} arg `{name}`"),
                arg.value.span.clone(),
            );
            covered.insert(name);
        }

        for param in params {
            if !param.optional && !covered.contains(param.name) && !misplaced.contains(param.name) {
                self.err_at(
                    format!("{callee}: missing required argument `{}`", param.name),
                    span.clone(),
                );
            }
        }
    }

    /// Type-check one builtin-call argument against its declared param,
    /// widening `int` to a `float`-typed param first.
    ///
    /// SPEC.md §14: "All [Math] functions accept `int` or `float`
    /// arguments" — the runtime's `num_arg` helper
    /// (`crates/keel-runtime/src/runtime/namespaces/math.rs`) accepts both
    /// and always returns `f64`. Every current `TySpec::Float` *parameter*
    /// (as opposed to a `TySpec::Float` *result*) belongs to `math`, so
    /// this widening is scoped by the spec's own meaning, not a namespace
    /// hardcode — a future non-math `TySpec::Float` param would get the
    /// same leniency, which is the correct reading of what `TySpec::Float`
    /// as a parameter type means today.
    fn expect_builtin_arg(
        &mut self,
        actual: &Ty,
        param: &BuiltinParam,
        context: &str,
        span: crate::lexer::Span,
    ) {
        if param.ty == TySpec::Float && matches!(actual, Ty::Int) {
            return;
        }
        self.expect_at(actual, &ty_from_spec(param.ty), context, span);
    }
}
