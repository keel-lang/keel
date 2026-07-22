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
//! # `CheckArtifacts`
//!
//! `designs/llvm-compilation.md` §2.2 specifies
//! `lib.rs: lower(ModuleGraph, CheckArtifacts) -> KirProgram` — consuming
//! the type checker's per-expression `Ty` table (`CheckArtifacts::expr_types`,
//! added by issue #109's `check_program_with_artifacts`/PR #122). `lower_program`
//! takes `&CheckArtifacts` and threads it through every lowering function, but
//! the M0/M1 scalar subset still gets its `KirType`s from structural bottom-up
//! inference (literal -> obvious type; binary op -> `expr::infer_binop_ty`;
//! identifier -> the type its declaring `let`/param recorded) rather than
//! artifact lookups — that inference is provably correct and conformance-
//! tested for everything it currently covers, so replacing it would be
//! churn with no behavior change. `artifacts` becomes load-bearing starting
//! with constructs that need the checker's own resolution (an anonymous
//! struct literal's target type, a nullable's inner type, …) — see
//! `designs/llvm-compilation.md` §4 M2's per-feature issues.
//!
//! Multi-module lowering is still out of scope: `lower_program` takes one
//! already-`keel check`ed [`Program`] (plus the `CheckArtifacts` from
//! checking that same program), not a `ModuleGraph`. The CLI
//! (`src/pipeline.rs`) passes the entry module only.
pub mod decl;
pub mod expr;
pub mod stmt;
pub mod sugar;

use std::collections::HashMap;
use std::fmt;

use keel_compiler::types::artifacts::CheckArtifacts;
use keel_syntax::ast::{Binding, Decl, Program, UseDecl, UseKind, UseSource};
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
pub fn lower_program(
    program: &Program,
    file_name: &str,
    artifacts: &CheckArtifacts,
) -> Result<KirProgram, LowerError> {
    let mut span_table = SpanTable::new(file_name);
    let mut funcs: HashMap<String, FuncSig> = HashMap::new();
    let mut ns_bindings: HashMap<String, String> = HashMap::new();
    let mut task_order: Vec<&keel_syntax::ast::TaskDecl> = Vec::new();

    // Pass 1: collect every task signature (so calls resolve regardless of
    // declaration order — forward references, mutual/self recursion) and
    // every `use std/<name>` namespace binding (so namespace calls resolve
    // regardless of whether the `use` appears before or after they're used).
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
            Decl::Use(use_decl) => {
                lower_use(use_decl, &decl.span, &mut ns_bindings)?;
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

    // Pass 2: lower each task body now that `funcs`/`ns_bindings` are complete.
    let mut functions: Vec<KirFunction> = Vec::with_capacity(task_order.len() + 1);
    for task in &task_order {
        let sig = &funcs[&task.name];
        functions.push(decl::lower_task_body(
            task,
            sig,
            &funcs,
            &ns_bindings,
            &mut span_table,
            artifacts,
        )?);
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
                &ns_bindings,
                &mut span_table,
                KirType::Unit,
                artifacts,
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

/// Lowers a `use` declaration into a `ns_bindings` entry (bound identifier
/// -> stdlib namespace name), or rejects it. Only `use std/<name>` (flat
/// stdlib module imports, no symbol lists, no relative-file imports) is in
/// scope: M1's namespace-call lowering only needs to know which identifier
/// a namespace is bound under, and multi-module/local-file lowering isn't
/// wired up yet (`lower_program` still takes one file, not a `ModuleGraph`).
fn lower_use(
    use_decl: &UseDecl,
    span: &Span,
    ns_bindings: &mut HashMap<String, String>,
) -> Result<(), LowerError> {
    let UseKind::Module { source, alias } = &use_decl.kind else {
        return Err(LowerError::unsupported(
            "symbol-list `use ... from ...` import",
            span.clone(),
        ));
    };
    let UseSource::Module(segments) = source else {
        return Err(LowerError::unsupported(
            "file-path `use` import (multi-module lowering isn't wired up yet)",
            span.clone(),
        ));
    };
    if segments.len() != 2 || segments[0] != "std" {
        return Err(LowerError::unsupported(
            "a `use` path other than `std/<name>`",
            span.clone(),
        ));
    }
    let namespace = &segments[1];
    if keel_catalog::namespace_id(namespace).is_none() {
        return Err(LowerError::new(
            format!("unknown std module `std/{namespace}`"),
            span.clone(),
        ));
    }
    let bound_name = alias.clone().unwrap_or_else(|| namespace.clone());
    ns_bindings.insert(bound_name, namespace.clone());
    Ok(())
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
