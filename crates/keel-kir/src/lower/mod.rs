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
use keel_syntax::ast::{Binding, Decl, Program, TypeDef, UseDecl, UseKind, UseSource};
use keel_syntax::lexer::Span;

use crate::ir::{
    EnumId, EnumLayout, FuncId, KirFunction, KirProgram, ListId, LocalId, NullableId, StructId,
    StructLayout,
};
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

/// Shared, read-only lowering state threaded through every lowering
/// function (bundled into one struct rather than growing the parameter
/// list further — M2's per-feature issues each add another whole-program
/// lookup table; structs here, more to come). `table: &mut SpanTable`
/// (mutable) and `ctx: &mut FnCtx` (per-function, mutable) stay separate
/// parameters — only immutable, whole-program state lives here.
pub(crate) struct LowerCtx<'a> {
    pub(crate) funcs: &'a HashMap<String, FuncSig>,
    pub(crate) ns_bindings: &'a HashMap<String, String>,
    pub(crate) structs_by_name: &'a HashMap<String, StructId>,
    pub(crate) struct_layouts: &'a [StructLayout],
    pub(crate) enums_by_name: &'a HashMap<String, EnumId>,
    pub(crate) enum_layouts: &'a [EnumLayout],
    /// See `lower_program`'s `lists` local for why this needs interior
    /// mutability (structurally discovered, not pre-declared).
    pub(crate) lists: &'a std::cell::RefCell<Vec<KirType>>,
    /// Same interior-mutability rationale as `lists`, for `T?` shapes.
    pub(crate) nullables: &'a std::cell::RefCell<Vec<KirType>>,
    /// Each task's per-parameter default-value expression (`None` for a
    /// parameter with no default), indexed by `FuncId`, parallel to that
    /// task's own param list. Lowered once per declaration in a separate,
    /// param-free scope — see `lower_program`'s pass 2c — not per call site;
    /// [`crate::lower::expr::lower_call`] clones the stored `Expr` into each
    /// call that omits a trailing arg.
    pub(crate) param_defaults: &'a HashMap<FuncId, Vec<Option<crate::ir::Expr>>>,
    /// Not consumed yet — #145 (named structs) resolves everything through
    /// context-threaded expected types instead (see `expr::lower_expr_expecting`).
    /// Becomes load-bearing for a construct the checker must resolve and
    /// lowering can't (an anonymous struct literal, a nullable's inner type,
    /// …) — see the module doc's `CheckArtifacts` section.
    #[allow(dead_code)]
    pub(crate) artifacts: &'a CheckArtifacts,
}

/// Describes `ty` for a diagnostic message. Same as `KirType`'s own
/// `Display` for scalars, but resolves a struct id to its declared name —
/// `KirType` alone can't do this (no `KirProgram` access, see `types.rs`'s
/// `name()` doc), but lowering always has `lcx.struct_layouts` in hand.
pub(crate) fn describe_ty(ty: KirType, lcx: &LowerCtx<'_>) -> String {
    match ty {
        KirType::Struct(id) => format!("struct {}", lcx.struct_layouts[id].name),
        KirType::Enum(id) => format!("enum {}", lcx.enum_layouts[id].name),
        other => other.to_string(),
    }
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
    let mut structs_by_name: HashMap<String, StructId> = HashMap::new();
    let mut struct_decls: Vec<&keel_syntax::ast::TypeDecl> = Vec::new();
    let mut enums_by_name: HashMap<String, EnumId> = HashMap::new();
    let mut enum_layouts: Vec<EnumLayout> = Vec::new();
    // `list[T]` shapes are structurally interned (see `ir.rs`'s `ListId`
    // doc) as they're *discovered* while lowering type annotations and list
    // literals throughout the whole program — unlike `structs_by_name`/
    // `enums_by_name` (built once, up front, from declarations), there's no
    // separate "declaration" pass to collect these from. `RefCell` keeps
    // `LowerCtx` otherwise fully immutable/shared (see its doc) while still
    // letting every lowering function grow this table via `intern_list`.
    let lists: std::cell::RefCell<Vec<KirType>> = std::cell::RefCell::new(Vec::new());
    // Same structural-interning rationale as `lists`, for `T?` shapes.
    let nullables: std::cell::RefCell<Vec<KirType>> = std::cell::RefCell::new(Vec::new());

    // Pass 1a: reserve a `StructId` for every named struct declaration
    // before resolving any field types, so a field can reference another
    // struct regardless of declaration order (forward references) — same
    // rationale as task signatures resolving before bodies. A simple enum
    // has no forward-reference problem (variants are bare names, not
    // types), so it's fully built here in one step rather than needing a
    // reserve-then-resolve split like structs. `RichEnum`/`Alias` aren't
    // scoped yet — rich (payload-carrying) variants are a follow-up issue
    // (see `ir.rs`'s `KirProgram::enums` doc).
    for decl in &program.declarations {
        if let Decl::Type(type_decl) = &decl.kind {
            match &type_decl.def {
                TypeDef::Struct(_) => {
                    if !type_decl.type_params.is_empty() {
                        return Err(LowerError::unsupported(
                            "generic struct type",
                            type_decl.name_span.clone(),
                        ));
                    }
                    let id = struct_decls.len();
                    structs_by_name.insert(type_decl.name.clone(), id);
                    struct_decls.push(type_decl);
                }
                TypeDef::SimpleEnum(variants) => {
                    if !type_decl.type_params.is_empty() {
                        return Err(LowerError::unsupported(
                            "generic enum type",
                            type_decl.name_span.clone(),
                        ));
                    }
                    let id = enum_layouts.len();
                    enums_by_name.insert(type_decl.name.clone(), id);
                    enum_layouts.push(EnumLayout {
                        id,
                        name: type_decl.name.clone(),
                        variants: variants.clone(),
                    });
                }
                TypeDef::RichEnum(_) | TypeDef::Alias(_) => {
                    return Err(LowerError::unsupported(
                        "rich enum or type-alias declaration (rich/payload-carrying variants \
                         land in a later M2/M3 issue; aliases aren't scoped yet)",
                        decl.span.clone(),
                    ));
                }
            }
        }
    }

    // Pass 1b: resolve each struct's field types now that every struct and
    // enum name in the file is known.
    let mut struct_layouts: Vec<StructLayout> = Vec::with_capacity(struct_decls.len());
    for (id, type_decl) in struct_decls.iter().enumerate() {
        let TypeDef::Struct(ast_fields) = &type_decl.def else {
            unreachable!("struct_decls only ever holds TypeDef::Struct entries, filtered above")
        };
        let mut fields = Vec::with_capacity(ast_fields.len());
        for field in ast_fields {
            fields.push((
                field.name.clone(),
                ty_expr_to_kir(
                    &field.ty,
                    &structs_by_name,
                    &enums_by_name,
                    &lists,
                    &nullables,
                )?,
            ));
        }
        struct_layouts.push(StructLayout {
            id,
            name: type_decl.name.clone(),
            fields,
        });
    }

    // Pass 2: collect every task signature (so calls resolve regardless of
    // declaration order — forward references, mutual/self recursion) and
    // every `use std/<name>` namespace binding (so namespace calls resolve
    // regardless of whether the `use` appears before or after they're used).
    for decl in &program.declarations {
        match &decl.kind {
            Decl::Task(task) => {
                let (params, ret) =
                    decl::signature_of(task, &structs_by_name, &enums_by_name, &lists, &nullables)?;
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
            Decl::Type(_) => {} // already handled in pass 1a/1b
            Decl::Stmt(_) => {} // handled in pass 3 (toplevel)
            other => {
                return Err(LowerError::unsupported(
                    decl_kind_name(other),
                    decl.span.clone(),
                ));
            }
        }
    }

    // Pass 2c: lower each task's parameter default-value expressions, now
    // that every task signature and namespace binding is known (in case a
    // default references either). Done via a bootstrap `LowerCtx` with an
    // empty `param_defaults` — a default expression may not itself omit a
    // defaulted argument of another call (an obscure case none of this
    // codebase's examples need); everything else about default expressions
    // (calls, namespace methods, literals) resolves normally. Each default
    // is lowered once, in a fresh param-free `FnCtx`, not per call site.
    let empty_param_defaults: HashMap<FuncId, Vec<Option<crate::ir::Expr>>> = HashMap::new();
    let mut param_defaults: HashMap<FuncId, Vec<Option<crate::ir::Expr>>> = HashMap::new();
    {
        let bootstrap_lcx = LowerCtx {
            funcs: &funcs,
            ns_bindings: &ns_bindings,
            structs_by_name: &structs_by_name,
            struct_layouts: &struct_layouts,
            enums_by_name: &enums_by_name,
            enum_layouts: &enum_layouts,
            lists: &lists,
            nullables: &nullables,
            param_defaults: &empty_param_defaults,
            artifacts,
        };
        for task in &task_order {
            let sig = &funcs[&task.name];
            let defaults =
                decl::lower_param_defaults(task, &sig.params, &bootstrap_lcx, &mut span_table)?;
            param_defaults.insert(sig.func_id, defaults);
        }
    }

    let lcx = LowerCtx {
        funcs: &funcs,
        ns_bindings: &ns_bindings,
        structs_by_name: &structs_by_name,
        struct_layouts: &struct_layouts,
        enums_by_name: &enums_by_name,
        enum_layouts: &enum_layouts,
        lists: &lists,
        nullables: &nullables,
        param_defaults: &param_defaults,
        artifacts,
    };

    // Pass 3: lower each task body now that `lcx` is complete.
    let mut functions: Vec<KirFunction> = Vec::with_capacity(task_order.len() + 1);
    for task in &task_order {
        let sig = &funcs[&task.name];
        functions.push(decl::lower_task_body(task, sig, &lcx, &mut span_table)?);
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
                &lcx,
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
        structs: struct_layouts,
        enums: enum_layouts,
        lists: lists.into_inner(),
        nullables: nullables.into_inner(),
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
/// variant outside the M0/M1/M2-so-far subset. `structs_by_name`/
/// `enums_by_name` resolve a bare `Named` type to a declared struct or enum
/// — checked after the built-in scalar names, so neither can shadow a
/// reserved type name.
pub(crate) fn ty_expr_to_kir(
    ty: &keel_syntax::ast::Node<keel_syntax::ast::TypeExpr>,
    structs_by_name: &HashMap<String, StructId>,
    enums_by_name: &HashMap<String, EnumId>,
    lists: &std::cell::RefCell<Vec<KirType>>,
    nullables: &std::cell::RefCell<Vec<KirType>>,
) -> Result<KirType, LowerError> {
    use keel_syntax::ast::TypeExpr;
    match &ty.kind {
        TypeExpr::Named(name) => match name.as_str() {
            "int" => Ok(KirType::I64),
            "float" => Ok(KirType::F64),
            "bool" => Ok(KirType::Bool),
            "str" => Ok(KirType::Str),
            "none" => Ok(KirType::Unit),
            other => {
                if let Some(id) = structs_by_name.get(other) {
                    Ok(KirType::Struct(*id))
                } else if let Some(id) = enums_by_name.get(other) {
                    Ok(KirType::Enum(*id))
                } else {
                    Err(LowerError::unsupported(
                        &format!("named type `{other}`"),
                        ty.span.clone(),
                    ))
                }
            }
        },
        TypeExpr::Nullable(inner) => {
            // Same no-own-span situation as `TypeExpr::List` below.
            let inner_node = keel_syntax::ast::Node::new((**inner).clone(), ty.span.clone());
            let inner_ty = ty_expr_to_kir(
                &inner_node,
                structs_by_name,
                enums_by_name,
                lists,
                nullables,
            )?;
            if !is_nullable_inner_ty(inner_ty) {
                return Err(LowerError::unsupported(
                    "nullable inner type other than int/float/bool/str/list/struct (enum and \
                     nested-nullable inner types are a later M2/M3 concern)",
                    ty.span.clone(),
                ));
            }
            Ok(KirType::Nullable(intern_nullable(nullables, inner_ty)))
        }
        TypeExpr::List(inner) => {
            // `TypeExpr::List` boxes a bare `TypeExpr`, not a `Node<TypeExpr>`
            // (no span of its own — see `keel-syntax`'s `ast::ty::TypeExpr`),
            // so diagnostics about the element type fall back to the whole
            // `list[...]` annotation's span.
            let inner_node = keel_syntax::ast::Node::new((**inner).clone(), ty.span.clone());
            let elem_ty = ty_expr_to_kir(
                &inner_node,
                structs_by_name,
                enums_by_name,
                lists,
                nullables,
            )?;
            if !is_list_element_ty(elem_ty) {
                return Err(LowerError::unsupported(
                    "list element type other than int/float/bool/str (struct/enum elements \
                     need Value marshaling, a later M2/M3 concern)",
                    ty.span.clone(),
                ));
            }
            Ok(KirType::List(intern_list(lists, elem_ty)))
        }
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

/// `true` for the element types a `list[T]` can hold today — int/float/
/// bool/str, the same set `emit_box_arg`/`rt_call::unbox_value` in
/// `keel-codegen` can marshal to/from a boxed `Value` without needing
/// struct/enum `Value` conversion (a later M2/M3 concern).
pub(crate) fn is_list_element_ty(ty: KirType) -> bool {
    matches!(
        ty,
        KirType::I64 | KirType::F64 | KirType::Bool | KirType::Str
    )
}

/// Interns `elem` into `lists`, returning its `ListId` — reuses an existing
/// entry for a structurally-identical element type rather than minting a
/// fresh one (`list[int]` written twice in a program is one `ListId`, not
/// two; see `ir.rs`'s `ListId` doc on why this differs from `StructId`/
/// `EnumId`'s nominal, declaration-order interning).
pub(crate) fn intern_list(lists: &std::cell::RefCell<Vec<KirType>>, elem: KirType) -> ListId {
    let mut lists = lists.borrow_mut();
    if let Some(id) = lists.iter().position(|t| *t == elem) {
        return id;
    }
    lists.push(elem);
    lists.len() - 1
}

/// `true` for the inner types a nullable (`T?`) can wrap today —
/// int/float/bool/str/list/struct, per §1.1's representation split (see
/// `KirType::Nullable`'s doc). `enum`/`none`/nested-nullable inner types are
/// a later M2/M3 concern, rejected with a clear message rather than
/// silently building a bad representation.
pub(crate) fn is_nullable_inner_ty(ty: KirType) -> bool {
    matches!(
        ty,
        KirType::I64
            | KirType::F64
            | KirType::Bool
            | KirType::Str
            | KirType::List(_)
            | KirType::Struct(_)
    )
}

/// Interns `inner` into `nullables`, returning its `NullableId` — same
/// structural (not declaration-order) interning as [`intern_list`].
pub(crate) fn intern_nullable(
    nullables: &std::cell::RefCell<Vec<KirType>>,
    inner: KirType,
) -> NullableId {
    let mut nullables = nullables.borrow_mut();
    if let Some(id) = nullables.iter().position(|t| *t == inner) {
        return id;
    }
    nullables.push(inner);
    nullables.len() - 1
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
