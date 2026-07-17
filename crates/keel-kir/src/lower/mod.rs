//! AST -> KIR lowering driver — the only KIR stage that sees the AST
//! (`designs/llvm-compilation.md` §2.3: "All desugaring happens here so
//! later passes see one form").
//!
//! # Scope (M0)
//!
//! Scalar subset only: `int`/`float`/`bool`/`str` literals, arithmetic and
//! comparison binary ops, `if`/`else`, `while`, `let`/assign, task
//! declarations with scalar params, direct calls, `return`. Everything else
//! — namespaces, agents, structs, enums, nullable, lambdas, generics,
//! string interpolation, `for`, `when`, `try`/`catch` — is rejected with a
//! [`LowerError`] naming the unsupported construct and its source span. This
//! is intentional (AGENTS.md: "no silent fallbacks") rather than a partial
//! best-effort lowering.
//!
//! Multi-module lowering is also out of scope for M0: `lower_program` takes
//! one already-`keel check`ed [`Program`], not a `ModuleGraph`. The CLI
//! (`src/pipeline.rs`) passes the entry module only.
//!
//! # The `#109` seam
//!
//! `designs/llvm-compilation.md` §2.2 specifies
//! `lib.rs: lower(ModuleGraph, CheckArtifacts) -> KirProgram` — consuming
//! the type checker's per-expression `Ty` table (`CheckArtifacts::expr_types`,
//! added by issue #109's `check_program_with_artifacts`). That plumbing does
//! not exist yet in this tree, so `lower_program` below infers scalar types
//! itself: task signatures come from AST type annotations
//! (`decl::signature_of`), and expression types are propagated structurally
//! (literal -> obvious type; binary op -> `expr::infer_binop_ty`; identifier
//! -> the type its declaring `let`/param recorded).
//!
// TODO(#109): once `CheckArtifacts` lands, replace the local inference in
// `expr.rs`/`stmt.rs`/`decl.rs` with lookups into `artifacts.expr_types`
// (keyed by `Span`), and change this function's signature to take
// `&ModuleGraph` + `&CheckArtifacts` instead of `&Program`. The two-pass
// signature-collection structure below (collect all task signatures, then
// lower bodies) stays either way — it's what makes forward/mutual calls
// resolve.
pub mod decl;
pub mod expr;
pub mod stmt;
pub mod sugar;

use std::collections::HashMap;
use std::fmt;

use keel_syntax::ast::{Binding, Decl, Program};
use keel_syntax::lexer::Span;

use crate::ir::{FuncId, KirFunction, KirProgram, LocalId};
use crate::span_table::SpanTable;
use crate::types::KirType;

/// A lowering failure: an AST construct M0's KIR does not (yet) support, or
/// a local scalar-inference mismatch. Points at the offending source span.
#[derive(Debug, Clone)]
pub struct LowerError {
    pub message: String,
    pub span: Span,
}

impl LowerError {
    pub(crate) fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub(crate) fn unsupported(what: &str, span: Span) -> Self {
        Self::new(
            format!("`{what}` is not supported by the scalar-subset KIR lowering (M0)"),
            span,
        )
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KIR lowering error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for LowerError {}

/// A task's lowered signature, known after the first pass so calls
/// (including forward and self-recursive calls) can resolve.
pub(crate) struct FuncSig {
    pub(crate) func_id: FuncId,
    pub(crate) params: Vec<KirType>,
    pub(crate) ret: KirType,
}

/// Per-function lowering state: the locals table under construction and a
/// stack of name -> `LocalId` scopes (innermost last), mirroring the
/// interpreter's `Environment` block scoping closely enough for M0's `if`/
/// `while` bodies.
pub(crate) struct FnCtx {
    pub(crate) locals: Vec<crate::ir::Local>,
    scopes: Vec<HashMap<String, LocalId>>,
}

impl FnCtx {
    fn new() -> Self {
        Self {
            locals: Vec::new(),
            scopes: vec![HashMap::new()],
        }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
        debug_assert!(!self.scopes.is_empty(), "popped the function's root scope");
    }

    /// Declares a fresh local in the *current* (innermost) scope — matches
    /// Keel's `x = expr` always-declares-in-current-scope rule.
    pub(crate) fn declare(&mut self, name: &str, ty: KirType) -> LocalId {
        let id = self.locals.len();
        self.locals.push(crate::ir::Local {
            id,
            name: name.to_string(),
            ty,
        });
        self.scopes
            .last_mut()
            .expect("root scope always present")
            .insert(name.to_string(), id);
        id
    }

    /// Resolves `name` against the nearest enclosing scope that declares it
    /// — used both for reads (`Ident`) and for `+=`-style updates, which
    /// mutate the existing binding rather than declaring a new one.
    pub(crate) fn resolve(&self, name: &str) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .copied()
    }
}

/// Lowers one already type-checked file to KIR.
///
/// # Errors
///
/// Returns a [`LowerError`] at the first AST construct outside the M0
/// scalar subset, or the first local scalar-inference mismatch.
pub fn lower_program(program: &Program, file_name: &str) -> Result<KirProgram, LowerError> {
    let mut span_table = SpanTable::new(file_name);
    let mut funcs: HashMap<String, FuncSig> = HashMap::new();
    let mut task_order: Vec<&keel_syntax::ast::TaskDecl> = Vec::new();

    // Pass 1: collect every task signature so calls resolve regardless of
    // declaration order (forward references, mutual/self recursion).
    for decl in &program.declarations {
        match &decl.kind {
            Decl::Task(task) => {
                let (params, ret) = decl::signature_of(task)?;
                let func_id = task_order.len();
                task_order.push(task);
                funcs.insert(
                    task.name.clone(),
                    FuncSig {
                        func_id,
                        params,
                        ret,
                    },
                );
            }
            Decl::Stmt(_) => {} // handled in pass 2 (toplevel)
            other => {
                return Err(LowerError::unsupported(
                    decl_kind_name(other),
                    decl.span.clone(),
                ));
            }
        }
    }

    // Pass 2: lower each task body now that `funcs` is complete.
    let mut functions: Vec<KirFunction> = Vec::with_capacity(task_order.len() + 1);
    for task in &task_order {
        let sig = &funcs[&task.name];
        functions.push(decl::lower_task_body(task, sig, &funcs, &mut span_table)?);
    }

    // Toplevel: every `Decl::Stmt` compiles into one synthetic function,
    // mirroring `Interpreter::execute`'s treatment of top-level statements.
    let toplevel_id = functions.len();
    let mut ctx = FnCtx::new();
    let mut body = Vec::new();
    for decl in &program.declarations {
        if let Decl::Stmt(stmt) = &decl.kind {
            body.push(stmt::lower_stmt(
                stmt,
                &mut ctx,
                &funcs,
                &mut span_table,
                KirType::Unit,
            )?);
        }
    }
    functions.push(KirFunction {
        id: toplevel_id,
        name: "<toplevel>".to_string(),
        params: Vec::new(),
        ret: KirType::Unit,
        locals: ctx.locals,
        body,
    });

    Ok(KirProgram {
        functions,
        toplevel: toplevel_id,
        span_table,
    })
}

fn decl_kind_name(decl: &Decl) -> &'static str {
    match decl {
        Decl::Type(_) => "type declaration",
        Decl::Interface(_) => "interface declaration",
        Decl::Impl(_) => "impl declaration",
        Decl::Task(_) => "task declaration",
        Decl::Test(_) => "test declaration",
        Decl::Extern(_) => "extern declaration",
        Decl::Agent(_) => "agent declaration",
        Decl::Use(_) => "use declaration",
        Decl::Stmt(_) => "statement",
    }
}

/// Converts a parsed type annotation to a `KirType`, rejecting every
/// variant outside the M0 scalar subset.
pub(crate) fn ty_expr_to_kir(
    ty: &keel_syntax::ast::Node<keel_syntax::ast::TypeExpr>,
) -> Result<KirType, LowerError> {
    use keel_syntax::ast::TypeExpr;
    match &ty.kind {
        TypeExpr::Named(name) => match name.as_str() {
            "int" => Ok(KirType::I64),
            "float" => Ok(KirType::F64),
            "bool" => Ok(KirType::Bool),
            "str" => Ok(KirType::Str),
            "none" => Ok(KirType::Unit),
            other => Err(LowerError::unsupported(
                &format!("named type `{other}`"),
                ty.span.clone(),
            )),
        },
        TypeExpr::Nullable(_) => Err(LowerError::unsupported("nullable type", ty.span.clone())),
        TypeExpr::List(_) => Err(LowerError::unsupported("list type", ty.span.clone())),
        TypeExpr::Map(_, _) => Err(LowerError::unsupported("map type", ty.span.clone())),
        TypeExpr::Set(_) => Err(LowerError::unsupported("set type", ty.span.clone())),
        TypeExpr::Struct(_) => Err(LowerError::unsupported(
            "inline struct type",
            ty.span.clone(),
        )),
        TypeExpr::Tuple(_) => Err(LowerError::unsupported("tuple type", ty.span.clone())),
        TypeExpr::Func(_, _) => Err(LowerError::unsupported("function type", ty.span.clone())),
        TypeExpr::Generic(_, _) => Err(LowerError::unsupported("generic type", ty.span.clone())),
        TypeExpr::Dynamic => Err(LowerError::unsupported("dynamic type", ty.span.clone())),
        TypeExpr::SelfType => Err(LowerError::unsupported("`self` type", ty.span.clone())),
    }
}

/// Extracts the plain identifier a `Binding` names, rejecting destructuring
/// patterns (`{a, b} = ...`, `(a, b) = ...`) — out of scope for M0.
pub(crate) fn binding_ident<'a>(binding: &'a Binding, span: &Span) -> Result<&'a str, LowerError> {
    match binding {
        Binding::Ident(name) => Ok(name),
        Binding::Destruct(_) => Err(LowerError::unsupported(
            "destructuring binding",
            span.clone(),
        )),
    }
}
