//! `evaluate` DAP request — parses the debug console's expression text and
//! runs it against the live, paused frame via `Interpreter::eval_in_frame`.
//!
//! Deliberately expression-only (not full statements): a bare expression
//! skips the top-level `exec_stmt` call and its hook invocation. But if the
//! expression itself calls a task or closure (e.g. `helper(9)`), that call's
//! body still runs through `exec_stmt` one level down, which re-enters
//! `DebugHook::on_statement` while the outer pause is still servicing this
//! very `evaluate` request. `DapHook::on_statement` guards against that by
//! refusing to pause again while already paused (see its reentrancy check),
//! so the nested call runs straight through instead of corrupting the
//! outer pause's state.

use keel_runtime::interpreter::Interpreter;
use keel_runtime::interpreter::environment::Environment;
use keel_syntax::ast::{Node, Stmt};

/// Parse and evaluate `expression` in `env`, returning its display string and
/// type name for the DAP `evaluate` response, or the error message on
/// failure (surfaced to the debug console rather than propagated — a typo in
/// a watch expression must not crash the paused program).
pub async fn evaluate(
    interp: &mut Interpreter,
    env: &mut Environment,
    expression: &str,
) -> Result<(String, String), String> {
    let named = miette::NamedSource::new("<evaluate>", expression.to_string());
    let tokens = keel_syntax::lexer::lex(expression, &named).map_err(|err| format!("{err:?}"))?;
    let mut stmts = keel_syntax::parser::parse_stmts(tokens, expression.len(), &named)
        .map_err(|err| format!("{err:?}"))?;
    let Some(Node {
        kind: Stmt::Expr(expr_node),
        ..
    }) = stmts.pop()
    else {
        return Err("only expressions can be evaluated here".to_string());
    };
    match interp.eval_in_frame(&expr_node, env).await {
        Ok(value) => Ok((value.to_string(), value.type_name().to_string())),
        Err(err) => Err(format!("{err:?}")),
    }
}
