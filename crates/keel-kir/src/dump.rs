//! Textual KIR dump — `keel build --emit=kir` and the golden tests in
//! `crates/keel-kir/tests/`.
//!
//! The format is deliberately simple (S-expression-flavored infix, one
//! statement per line) and is test surface, not a stable API — see
//! `designs/llvm-compilation.md` §2.3.

use std::fmt::Write as _;

use crate::ir::{BinOp, Block, CallTarget, Expr, FuncId, KirFunction, KirProgram, Stmt, UnOp};
use crate::types::KirType;

/// Renders every struct declaration, then every function in `program`, in
/// declaration order.
#[must_use]
pub fn dump(program: &KirProgram) -> String {
    let mut out = String::new();
    for s in &program.structs {
        let fields = s
            .fields
            .iter()
            .map(|(name, ty)| format!("{name}: {}", fmt_ty(program, *ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "struct {} {{ {fields} }}", s.name);
    }
    for e in &program.enums {
        let _ = writeln!(out, "enum {} {{ {} }}", e.name, e.variants.join(", "));
    }
    if !program.structs.is_empty() || !program.enums.is_empty() {
        out.push('\n');
    }
    for (i, func) in program.functions.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        dump_function(&mut out, program, func);
    }
    out
}

/// Renders `ty` for the dump — same as `KirType`'s own `Display` for
/// scalars, but resolves a struct id to its declared name (`KirType` alone
/// can't, see `types.rs`'s `name()` doc; this function has `program` in hand).
fn fmt_ty(program: &KirProgram, ty: KirType) -> String {
    match ty {
        KirType::Struct(id) => program.structs[id].name.clone(),
        KirType::Enum(id) => program.enums[id].name.clone(),
        KirType::List(id) => format!("list[{}]", fmt_ty(program, program.lists[id])),
        KirType::Nullable(id) => format!("{}?", fmt_ty(program, program.nullables[id])),
        other => other.to_string(),
    }
}

fn dump_function(out: &mut String, program: &KirProgram, func: &KirFunction) {
    let params = func
        .params
        .iter()
        .map(|p| {
            let name = &func.locals[p.local].name;
            format!("{name}: {}", fmt_ty(program, p.ty))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "fn {}({params}) -> {} {{",
        func.name,
        fmt_ty(program, func.ret)
    );
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
                fmt_ty(program, l.ty),
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
        Stmt::ForEach {
            var, list, body, ..
        } => {
            let v = &func.locals[*var];
            let _ = writeln!(
                out,
                "for {} in {} {{",
                v.name,
                fmt_expr(program, func, list)
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
            let name = call_target_name(program, *target);
            let args = args
                .iter()
                .map(|a| fmt_expr(program, func, a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}({args})")
        }
        Expr::MakeStruct { struct_id, fields } => {
            let layout = &program.structs[*struct_id];
            let fields = layout
                .fields
                .iter()
                .zip(fields)
                .map(|((name, _), value)| format!("{name}: {}", fmt_expr(program, func, value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{ {fields} }}", layout.name)
        }
        Expr::FieldGet {
            base, field_index, ..
        } => {
            let Expr::Local { ty, .. } = base.as_ref() else {
                return format!("{}.#{field_index}", fmt_expr(program, func, base));
            };
            let KirType::Struct(struct_id) = ty else {
                return format!("{}.#{field_index}", fmt_expr(program, func, base));
            };
            let field_name = &program.structs[*struct_id].fields[*field_index].0;
            format!("{}.{field_name}", fmt_expr(program, func, base))
        }
        Expr::MakeEnum {
            enum_id,
            variant_index,
        } => {
            let layout = &program.enums[*enum_id];
            format!("{}.{}", layout.name, layout.variants[*variant_index])
        }
        Expr::Index { list, index, .. } => {
            format!(
                "{}[{}]",
                fmt_expr(program, func, list),
                fmt_expr(program, func, index)
            )
        }
        Expr::NullLit { .. } => "none".to_string(),
        Expr::NullSome { value, .. } => fmt_expr(program, func, value),
        Expr::NullCoalesce {
            nullable, fallback, ..
        } => format!(
            "({} ?? {})",
            fmt_expr(program, func, nullable),
            fmt_expr(program, func, fallback)
        ),
        Expr::NullFieldGet {
            base, field_index, ..
        } => {
            let Expr::Local { ty, .. } = base.as_ref() else {
                return format!("{}?.#{field_index}", fmt_expr(program, func, base));
            };
            let KirType::Nullable(nullable_id) = ty else {
                return format!("{}?.#{field_index}", fmt_expr(program, func, base));
            };
            let KirType::Struct(struct_id) = program.nullables[*nullable_id] else {
                return format!("{}?.#{field_index}", fmt_expr(program, func, base));
            };
            let field_name = &program.structs[struct_id].fields[*field_index].0;
            format!("{}?.{field_name}", fmt_expr(program, func, base))
        }
    }
}

fn fn_name(program: &KirProgram, id: FuncId) -> &str {
    &program.functions[id].name
}

/// `foo` for a direct call, `namespace.method` for a namespace call —
/// resolved back from `(ns_id, method_id)` via the catalog purely for dump
/// readability (KIR itself only stores the numeric ids; see `ir::CallTarget`).
fn call_target_name(program: &KirProgram, target: CallTarget) -> String {
    match target {
        CallTarget::Fn(id) => fn_name(program, id).to_string(),
        CallTarget::Ns { ns_id, method_id } => keel_catalog::catalog()
            .find(|m| {
                keel_catalog::namespace_id(m.namespace) == Some(ns_id) && m.method_id == method_id
            })
            .map(|m| format!("{}.{}", m.namespace, m.name))
            .unwrap_or_else(|| format!("ns#{ns_id}.method#{method_id}")),
        CallTarget::Rt(rt_fn) => format!("rt.{}", rt_fn_name(rt_fn)),
    }
}

fn rt_fn_name(rt_fn: crate::ir::RtFn) -> &'static str {
    match rt_fn {
        crate::ir::RtFn::ListNew => "list_new",
        crate::ir::RtFn::ListPush => "list_push",
        crate::ir::RtFn::ListLen => "list_len",
        crate::ir::RtFn::IntToStr => "int_to_str",
        crate::ir::RtFn::FloatToStr => "float_to_str",
        crate::ir::RtFn::BoolToStr => "bool_to_str",
    }
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
