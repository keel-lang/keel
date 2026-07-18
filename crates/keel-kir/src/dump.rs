//! Textual KIR dump — `keel build --emit=kir` and the golden tests in
//! `crates/keel-kir/tests/`.
//!
//! The format is deliberately simple (S-expression-flavored infix, one
//! statement per line) and is test surface, not a stable API — see
//! `designs/llvm-compilation.md` §2.3.

use std::fmt::Write as _;

use crate::ir::{BinOp, Block, Expr, FuncId, KirFunction, KirProgram, Stmt, UnOp};

/// Renders every function in `program`, in declaration order.
#[must_use]
pub fn dump(program: &KirProgram) -> String {
    let mut out = String::new();
    for (i, func) in program.functions.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        dump_function(&mut out, program, func);
    }
    out
}

fn dump_function(out: &mut String, program: &KirProgram, func: &KirFunction) {
    let params = func
        .params
        .iter()
        .map(|p| {
            let name = &func.locals[p.local].name;
            format!("{name}: {}", p.ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "fn {}({params}) -> {} {{", func.name, func.ret);
    dump_block(out, program, func, &func.body, 1);
    out.push_str("}\n");
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn dump_block(
    out: &mut String,
    program: &KirProgram,
    func: &KirFunction,
    block: &Block,
    depth: usize,
) {
    for stmt in block {
        dump_stmt(out, program, func, stmt, depth);
    }
}

fn dump_stmt(
    out: &mut String,
    program: &KirProgram,
    func: &KirFunction,
    stmt: &Stmt,
    depth: usize,
) {
    indent(out, depth);
    match stmt {
        Stmt::Let { local, init } => {
            let l = &func.locals[*local];
            let _ = writeln!(
                out,
                "let {}: {} = {}",
                l.name,
                l.ty,
                fmt_expr(program, func, init)
            );
        }
        Stmt::Assign { local, value } => {
            let l = &func.locals[*local];
            let _ = writeln!(out, "{} = {}", l.name, fmt_expr(program, func, value));
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let _ = writeln!(out, "if {} {{", fmt_expr(program, func, cond));
            dump_block(out, program, func, then_branch, depth + 1);
            indent(out, depth);
            if else_branch.is_empty() {
                out.push_str("}\n");
            } else {
                out.push_str("} else {\n");
                dump_block(out, program, func, else_branch, depth + 1);
                indent(out, depth);
                out.push_str("}\n");
            }
        }
        Stmt::While { cond, body } => {
            let _ = writeln!(out, "while {} {{", fmt_expr(program, func, cond));
            dump_block(out, program, func, body, depth + 1);
            indent(out, depth);
            out.push_str("}\n");
        }
        Stmt::ForIndex {
            var,
            low,
            high,
            body,
        } => {
            let v = &func.locals[*var];
            let _ = writeln!(
                out,
                "for {} in {}..{} {{",
                v.name,
                fmt_expr(program, func, low),
                fmt_expr(program, func, high)
            );
            dump_block(out, program, func, body, depth + 1);
            indent(out, depth);
            out.push_str("}\n");
        }
        Stmt::Return(None) => {
            out.push_str("return\n");
        }
        Stmt::Return(Some(expr)) => {
            let _ = writeln!(out, "return {}", fmt_expr(program, func, expr));
        }
        Stmt::Expr(expr) => {
            let _ = writeln!(out, "{}", fmt_expr(program, func, expr));
        }
    }
}

fn fmt_expr(program: &KirProgram, func: &KirFunction, expr: &Expr) -> String {
    match expr {
        Expr::ConstInt(v) => v.to_string(),
        Expr::ConstFloat(v) => {
            if v.fract() == 0.0 && v.is_finite() {
                format!("{v:.1}")
            } else {
                v.to_string()
            }
        }
        Expr::ConstBool(v) => v.to_string(),
        Expr::ConstStr(v) => format!("{v:?}"),
        Expr::Local { id, .. } => func.locals[*id].name.clone(),
        Expr::BinOp {
            op, left, right, ..
        } => format!(
            "({} {} {})",
            fmt_expr(program, func, left),
            binop_symbol(*op),
            fmt_expr(program, func, right)
        ),
        Expr::UnOp { op, operand, .. } => {
            format!("({}{})", unop_symbol(*op), fmt_expr(program, func, operand))
        }
        Expr::Call { target, args, .. } => {
            let name = fn_name(program, *target);
            let args = args
                .iter()
                .map(|a| fmt_expr(program, func, a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}({args})")
        }
    }
}

fn fn_name(program: &KirProgram, id: FuncId) -> &str {
    &program.functions[id].name
}

fn binop_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

fn unop_symbol(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "not ",
    }
}
