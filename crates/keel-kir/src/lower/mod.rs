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
    Block, EnumId, EnumLayout, FuncId, KirFunction, KirProgram, ListId, LocalId, MapId, NullableId,
    SetId, StructId, StructLayout, TupleId, TupleLayout,
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

/// One parameter's default state, as seen by a call site.
///
/// A call site needs two different things from a callee's defaults, and they
/// become available at different times: *whether* a parameter has one (to
/// check arity and decide what to fill) is known from the AST as soon as
/// signatures are collected, while the default's lowered `Expr` (to actually
/// fill an omitted argument) only exists after pass 2c has run. Splitting the
/// two lets a call inside a parameter default — lowered *during* pass 2c —
/// resolve normally instead of tripping over a half-built table.
pub(crate) enum ParamDefault {
    /// No default declared: an argument must be supplied at every call site.
    Required,
    /// A default is declared but not lowered yet — pass 2c is still running,
    /// and this call is itself inside some task's parameter default. Only
    /// blocks a call that *omits* this argument; supplying it is fine.
    NotLoweredYet,
    /// A lowered default, cloned into each call site that omits the argument.
    Lowered(crate::ir::Expr),
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
    /// Same interior-mutability rationale as `lists`, for `map[str, V]`
    /// value types.
    pub(crate) maps: &'a std::cell::RefCell<Vec<KirType>>,
    /// Same interior-mutability rationale as `lists`, for `set[T]` element
    /// types.
    pub(crate) sets: &'a std::cell::RefCell<Vec<KirType>>,
    /// Same interior-mutability rationale as `lists`, for `T?` shapes.
    pub(crate) nullables: &'a std::cell::RefCell<Vec<KirType>>,
    /// Same interior-mutability rationale as `lists`, for tuple shapes.
    pub(crate) tuples: &'a std::cell::RefCell<Vec<TupleLayout>>,
    /// Each task's per-parameter default state, indexed by `FuncId`,
    /// parallel to that task's own param list. Lowered once per declaration
    /// in a separate, param-free scope — see `lower_program`'s pass 2c — not
    /// per call site; [`crate::lower::expr::lower_call`] clones the stored
    /// `Expr` into each call that omits a trailing arg.
    pub(crate) param_defaults: &'a HashMap<FuncId, Vec<ParamDefault>>,
    /// Not consumed yet — #145 (named structs) resolves everything through
    /// context-threaded expected types instead (see `expr::lower_expr_expecting`).
    /// Becomes load-bearing for a construct the checker must resolve and
    /// lowering can't (an anonymous struct literal, a nullable's inner type,
    /// …) — see the module doc's `CheckArtifacts` section.
    #[allow(dead_code)]
    pub(crate) artifacts: &'a CheckArtifacts,
    /// The synthetic `UserRaised { message: str }` struct's id, if the
    /// program uses `raise`/`try`/`catch` anywhere (`lower_program`'s Pass
    /// 1c) — `None` otherwise. `Stmt::Raise`/`Stmt::TryCatch` lowering
    /// (`lower/stmt.rs`) only ever runs when this is `Some`, since their
    /// AST counterparts can't be reached in a program the scan found none
    /// in.
    pub(crate) user_raised_struct_id: Option<StructId>,
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

/// Per-function lowering state: the locals table under construction, a
/// stack of name -> `LocalId` scopes (innermost last), mirroring the
/// interpreter's `Environment` block scoping closely enough for M0's `if`/
/// `while` bodies, the function's own return type (needed wherever a `return`
/// or an implicit tail expression is lowered), and the *hoist buffer* — the
/// statements a nested `when`-expression needs emitted ahead of the statement
/// currently being lowered (see [`FnCtx::hoist`]).
pub(crate) struct FnCtx {
    pub(crate) locals: Vec<crate::ir::Local>,
    scopes: Vec<HashMap<String, LocalId>>,
    /// The lowered return type of the function this context belongs to.
    /// Lives here rather than being threaded as a parameter because
    /// *expression* lowering needs it too now that a `when`-expression can
    /// appear in an arbitrary nested position and its arm bodies may contain
    /// an explicit `return` (issue #170).
    pub(crate) ret_ty: KirType,
    /// Statements hoisted out of the expression currently being lowered, to
    /// be emitted immediately before the enclosing statement. See
    /// [`FnCtx::hoist`] for the full rationale; [`stmt::lower_stmt`] installs
    /// a fresh buffer per statement and drains it.
    hoisted: Vec<crate::ir::Stmt>,
}

impl FnCtx {
    fn new(ret_ty: KirType) -> Self {
        Self {
            locals: Vec::new(),
            scopes: vec![HashMap::new()],
            ret_ty,
            hoisted: Vec::new(),
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
        let id = self.declare_temp(name, ty);
        self.scopes
            .last_mut()
            .expect("root scope always present")
            .insert(name.to_string(), id);
        id
    }

    /// Declares a compiler-generated local that is deliberately *not* entered
    /// into any scope map: a hoisted `when`-expression's result temp or an
    /// evaluation-order spill (see [`FnCtx::keep_order`]) is referenced only
    /// by the `LocalId` returned here, never resolved by name, so it can
    /// neither shadow a user binding nor be captured by one.
    pub(crate) fn declare_temp(&mut self, name: &str, ty: KirType) -> LocalId {
        let id = self.locals.len();
        self.locals.push(crate::ir::Local {
            id,
            name: name.to_string(),
            ty,
        });
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

    /// Swaps in a fresh, empty hoist buffer and returns the enclosing one,
    /// to be handed back to [`FnCtx::end_hoist`]. Paired around each
    /// statement (so an `if` body's hoists stay inside that body rather than
    /// escaping past the `if`) and around the discarded first-arm type probe
    /// in [`stmt::lower_when_expr_value`].
    pub(crate) fn begin_hoist(&mut self) -> Vec<crate::ir::Stmt> {
        std::mem::take(&mut self.hoisted)
    }

    /// Restores the enclosing hoist buffer `begin_hoist` returned, yielding
    /// the statements hoisted since.
    pub(crate) fn end_hoist(&mut self, outer: Vec<crate::ir::Stmt>) -> Vec<crate::ir::Stmt> {
        std::mem::replace(&mut self.hoisted, outer)
    }

    /// Records a statement to be emitted immediately before the statement
    /// currently being lowered.
    ///
    /// KIR's `Expr` is a tree with no statement-sequencing of its own, so a
    /// `when` used as an expression in a nested position (`f(when n {...})`,
    /// `1 + when n {...}`) can't be lowered to an `Expr` directly — it
    /// desugars to a declare-only `Let` plus an `if`-chain that assigns into
    /// it, and those statements have to go *somewhere*. That somewhere is
    /// this buffer, which [`stmt::lower_stmt`] drains ahead of the statement
    /// it belongs to (issue #170; generalizes the `let`/`return`-position
    /// desugaring of issue #160).
    pub(crate) fn hoist(&mut self, stmt: crate::ir::Stmt) {
        self.hoisted.push(stmt);
    }

    /// A snapshot of the hoist buffer's length, taken before lowering a
    /// sub-expression so [`FnCtx::keep_order`] / [`FnCtx::forbid_hoist`] can
    /// tell whether that sub-expression hoisted anything.
    pub(crate) fn hoist_mark(&self) -> usize {
        self.hoisted.len()
    }

    /// Rejects a sub-expression that hoisted, at a position where the hoisted
    /// statements cannot legally be moved ahead of the enclosing statement:
    /// either because the sub-expression isn't evaluated exactly once there
    /// (a `while` condition runs per iteration; an `and`/`or` right operand,
    /// a `??` fallback and a `when` arm's own pattern test are all
    /// conditional), or because there is no enclosing statement at all (a
    /// task's parameter default, lowered standalone).
    ///
    /// This is the *default* posture: any site that lowers a sub-expression
    /// and does not explicitly opt into [`FnCtx::keep_order`] calls this
    /// instead, so an unhandled position produces a clear error rather than
    /// silently miscompiling evaluation order.
    pub(crate) fn forbid_hoist(
        &mut self,
        mark: usize,
        what: &str,
        span: &Span,
    ) -> Result<(), LowerError> {
        if self.hoisted.len() == mark {
            return Ok(());
        }
        self.hoisted.truncate(mark);
        // Two constructs hoist today — a nested `when` expression (issue #170)
        // and a struct literal/spread-update whose fields are written out of
        // declared order (issue #190) — so this message names the need, not
        // one specific syntax.
        Err(LowerError::unsupported(
            &format!(
                "a sub-expression that must be evaluated ahead of the enclosing statement, in {what}"
            ),
            span.clone(),
        ))
    }

    /// Binds `slot` to a fresh temp, replacing it in place with a read of
    /// that temp and returning the `Let` that has to run wherever the
    /// caller decides evaluation order demands.
    ///
    /// Returns `None` for a constant, which needs no sequencing at all —
    /// nothing can observe *when* a literal is "evaluated", so leaving it
    /// inline keeps the common case free of temps.
    ///
    /// Shared by [`FnCtx::keep_order`] (which splices the `Let`s in ahead of
    /// a hoisted chain) and [`FnCtx::pin_order`] (which appends them), so
    /// both agree on exactly which values are spillable.
    fn spill(
        &mut self,
        name: &str,
        slot: &mut crate::ir::Expr,
        span: &Span,
    ) -> Result<Option<crate::ir::Stmt>, LowerError> {
        use crate::ir::Expr;
        if matches!(
            slot,
            Expr::ConstInt(_)
                | Expr::ConstFloat(_)
                | Expr::ConstBool(_)
                | Expr::ConstStr(_)
                | Expr::MakeEnum { .. }
                | Expr::NullLit { .. }
        ) {
            return Ok(None);
        }
        let ty = slot.ty();
        if ty == KirType::Unit {
            // No `Expr` variant denotes a unit value, so there's nothing
            // to put back in `slot` after spilling it to a statement. The
            // checker doesn't accept a `none`-typed sub-expression in any
            // of these positions today, so this is a guard against a
            // future one arriving silently, not a reachable diagnostic.
            return Err(LowerError::unsupported(
                "a `none`-typed sub-expression that needs to be evaluated ahead of a sibling",
                span.clone(),
            ));
        }
        let temp = self.declare_temp(name, ty);
        let value = std::mem::replace(slot, Expr::Local { id: temp, ty });
        Ok(Some(crate::ir::Stmt::Let {
            local: temp,
            init: Some(value),
        }))
    }

    /// Preserves left-to-right evaluation order when a sub-expression hoists
    /// past siblings that were already lowered.
    ///
    /// Hoisting moves a `when`-expression's `if`-chain *ahead of* the whole
    /// enclosing statement, which would reorder it before every sibling to
    /// its left: `f(h(), when x { ... })` would run the chain before `h()`.
    /// So whenever lowering the newest sibling grew the buffer past `mark`,
    /// every previously-lowered sibling in `prior` is spilled into a temp
    /// bound at `mark` — i.e. still before the hoisted chain, but after
    /// everything hoisted by an earlier sibling — and rewritten to read that
    /// temp. Constants are skipped (see [`FnCtx::spill`]); a plain `Local`
    /// read is *not*, since a `when` arm body can reassign an outer local via
    /// `+=`. That includes a `Local` that is itself an earlier hoist temp
    /// (`f(when a {...}, when b {...})` spills the first result into a second
    /// local) — redundant, but cheaper than tracking every temp's provenance
    /// to prove the copy away.
    ///
    /// `prior` is in evaluation order and is rewritten in place. Call it with
    /// the siblings lowered so far, *before* pushing the newest one.
    pub(crate) fn keep_order(
        &mut self,
        mark: usize,
        prior: &mut [crate::ir::Expr],
        span: &Span,
    ) -> Result<(), LowerError> {
        if self.hoisted.len() == mark {
            return Ok(());
        }
        let mut spills = Vec::new();
        for slot in prior.iter_mut() {
            if let Some(stmt) = self.spill("<spill>", slot, span)? {
                spills.push(stmt);
            }
        }
        self.hoisted.splice(mark..mark, spills);
        Ok(())
    }

    /// Pins already-lowered siblings into temps *now*, in the order given, so
    /// that a node which assembles them in some *other* order can no longer
    /// reorder their evaluation.
    ///
    /// This is the struct-literal case (issue #190): a literal's fields are
    /// evaluated in source order but `Expr::MakeStruct` stores them in
    /// declared order, and codegen evaluates `MakeStruct`'s operands in the
    /// order they appear. Binding each field's value to a temp ahead of the
    /// enclosing statement leaves `MakeStruct` holding nothing but `Local`
    /// reads and constants, which are order-transparent, so assembling them
    /// in declared order is no longer observable.
    ///
    /// Unlike [`FnCtx::keep_order`] the `Let`s are *appended* rather than
    /// spliced at a mark: they must land after everything the siblings
    /// themselves hoisted, and `keep_order` has already bound any value that
    /// a hoist could have invalidated at the precise point it was still
    /// valid.
    ///
    /// Each temp is named `<pin.N>` after its position in `values`, so a KIR
    /// dump shows the evaluation order directly — the whole point of the
    /// transform, and otherwise indistinguishable once the values have been
    /// shuffled into declared order.
    pub(crate) fn pin_order(
        &mut self,
        values: &mut [crate::ir::Expr],
        span: &Span,
    ) -> Result<(), LowerError> {
        for (index, slot) in values.iter_mut().enumerate() {
            if let Some(stmt) = self.spill(&format!("<pin.{index}>"), slot, span)? {
                self.hoisted.push(stmt);
            }
        }
        Ok(())
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
    // Same structural-interning rationale as `lists`, for `map[str, V]`
    // value types.
    let maps: std::cell::RefCell<Vec<KirType>> = std::cell::RefCell::new(Vec::new());
    // Same structural-interning rationale as `lists`, for `set[T]` element
    // types.
    let sets: std::cell::RefCell<Vec<KirType>> = std::cell::RefCell::new(Vec::new());
    // Same structural-interning rationale as `lists`, for `T?` shapes.
    let nullables: std::cell::RefCell<Vec<KirType>> = std::cell::RefCell::new(Vec::new());
    // Same structural-interning rationale as `lists`, for tuple shapes.
    let tuples: std::cell::RefCell<Vec<TupleLayout>> = std::cell::RefCell::new(Vec::new());

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
                    &maps,
                    &sets,
                    &nullables,
                    &tuples,
                )?,
            ));
        }
        struct_layouts.push(StructLayout {
            id,
            name: type_decl.name.clone(),
            fields,
        });
    }

    // Pass 1c: register the synthetic `UserRaised { message: str }` struct
    // — the shape every caught error binds to (`raise` only ever produces
    // `UserRaised`; `catch e: Error` and `catch e: UserRaised` both bind
    // it) — only if the program actually uses `raise`/`try`/`catch`
    // anywhere. Unconditionally registering it would add a phantom struct
    // to every program's golden dump, including ones that never raise.
    let user_raised_struct_id: Option<StructId> = if program_uses_raise_or_try(program) {
        let id = struct_layouts.len();
        struct_layouts.push(StructLayout {
            id,
            name: "UserRaised".to_string(),
            fields: vec![("message".to_string(), KirType::Str)],
        });
        Some(id)
    } else {
        None
    };

    // Pass 2: collect every task signature (so calls resolve regardless of
    // declaration order — forward references, mutual/self recursion) and
    // every `use std/<name>` namespace binding (so namespace calls resolve
    // regardless of whether the `use` appears before or after they're used).
    for decl in &program.declarations {
        match &decl.kind {
            Decl::Task(task) => {
                let (params, ret) = decl::signature_of(
                    task,
                    &structs_by_name,
                    &enums_by_name,
                    &lists,
                    &maps,
                    &sets,
                    &nullables,
                    &tuples,
                )?;
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
    // default references either). Done via a bootstrap `LowerCtx` whose
    // `param_defaults` records only which parameters *have* a default
    // (`NotLoweredYet`), read straight off the AST — enough for a call inside
    // a default to arity-check its callee, which is all `lower_call` needs
    // unless it also omits a defaulted argument. That one case (a default
    // that relies on another default) is rejected rather than ordered around:
    // resolving it means lowering callees' defaults first, which has no answer
    // when two defaults call each other. Everything else about default
    // expressions (calls, namespace methods, literals) resolves normally.
    // Each default is lowered once, in a fresh param-free `FnCtx`, not per
    // call site.
    let bootstrap_param_defaults: HashMap<FuncId, Vec<ParamDefault>> = task_order
        .iter()
        .map(|task| {
            let sig = &funcs[&task.name];
            let states = task
                .params
                .iter()
                .map(|param| match param.default {
                    Some(_) => ParamDefault::NotLoweredYet,
                    None => ParamDefault::Required,
                })
                .collect();
            (sig.func_id, states)
        })
        .collect();
    let mut param_defaults: HashMap<FuncId, Vec<ParamDefault>> = HashMap::new();
    {
        let bootstrap_lcx = LowerCtx {
            funcs: &funcs,
            ns_bindings: &ns_bindings,
            structs_by_name: &structs_by_name,
            struct_layouts: &struct_layouts,
            enums_by_name: &enums_by_name,
            enum_layouts: &enum_layouts,
            lists: &lists,
            maps: &maps,
            sets: &sets,
            nullables: &nullables,
            tuples: &tuples,
            param_defaults: &bootstrap_param_defaults,
            artifacts,
            user_raised_struct_id,
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
        maps: &maps,
        sets: &sets,
        nullables: &nullables,
        tuples: &tuples,
        param_defaults: &param_defaults,
        artifacts,
        user_raised_struct_id,
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
    let mut ctx = FnCtx::new(KirType::Unit);
    let mut body = Vec::new();
    for decl in &program.declarations {
        if let Decl::Stmt(stmt) = &decl.kind {
            body.extend(stmt::lower_stmt(
                stmt,
                &mut ctx,
                &lcx,
                &mut span_table,
                stmt::TailSink::Discard,
            )?);
        }
    }
    functions.push(KirFunction {
        id: toplevel_id,
        name: "<toplevel>".to_string(),
        params: Vec::new(),
        ret: KirType::Unit,
        // Placeholder — `compute_can_raise` fills in the real value for
        // every function below, once every body (and thus every
        // `Stmt::Raise`/`CallTarget::Fn` call site) is known.
        can_raise: false,
        locals: ctx.locals,
        body,
    });

    compute_can_raise(&mut functions)?;
    if functions[toplevel_id].can_raise {
        return Err(LowerError::new(
            "an uncaught `raise` (or call to a function that can raise) reaches the top level \
             — wrap it in `try`/`catch` (propagating past the top level, changing \
             `keel_user_toplevel`'s fixed entry-point signature, is a later M2/M3 concern)"
                .to_string(),
            program
                .declarations
                .last()
                .map(|d| d.span.clone())
                .unwrap_or(0..0),
        ));
    }

    Ok(KirProgram {
        functions,
        toplevel: toplevel_id,
        structs: struct_layouts,
        enums: enum_layouts,
        lists: lists.into_inner(),
        maps: maps.into_inner(),
        sets: sets.into_inner(),
        nullables: nullables.into_inner(),
        tuples: tuples.into_inner(),
        span_table,
    })
}

/// Computes `can_raise` for every function, mutating it in place: a
/// function is `can_raise` iff it directly executes `Stmt::Raise` outside
/// any of its own `try` bodies, or makes an uncaught `CallTarget::Fn` call
/// (same condition) to another `can_raise` function — a fixpoint over the
/// call graph, since that "another function" may itself only be known
/// `can_raise` from a later function in declaration order (forward/mutual
/// recursion). A full re-scan per round (rather than a worklist) is simplest
/// and plenty fast for these program sizes; converges in at most
/// `functions.len()` rounds. Also rejects a `can_raise` function whose
/// return type isn't one the result-ABI's uniformly-boxed payload
/// representation models (`raise.rs`'s `emit_box_result_value`, on the
/// `keel-codegen` side): `Struct`/`Enum`/`Nullable` need `Value` marshaling
/// that doesn't exist yet, a later M2/M3 concern (the synthetic
/// `UserRaised` struct itself is fine — it's never a function's own `ret`).
fn compute_can_raise(functions: &mut [KirFunction]) -> Result<(), LowerError> {
    let mut can_raise = vec![false; functions.len()];
    loop {
        let mut changed = false;
        for (i, f) in functions.iter().enumerate() {
            if !can_raise[i] && block_escapes_uncaught(&f.body, 0, &can_raise) {
                can_raise[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (f, cr) in functions.iter_mut().zip(can_raise) {
        f.can_raise = cr;
        if cr
            && matches!(
                f.ret,
                KirType::Struct(_) | KirType::Enum(_) | KirType::Nullable(_)
            )
        {
            return Err(LowerError::new(
                format!(
                    "task `{}` can raise and returns `{}` — only int/float/bool/str/list/none \
                     return types are modeled for a can-raise function yet (struct/enum/nullable \
                     need `Value` marshaling, a later M2/M3 concern)",
                    f.name, f.ret
                ),
                0..0,
            ));
        }
    }
    Ok(())
}

/// Whether any statement in `block` escapes this function uncaught (see
/// `compute_can_raise`) — `try_depth` counts how many of this function's own
/// enclosing `try` bodies (not the AST's lexical nesting depth in general,
/// just `Stmt::TryCatch.body` specifically) the current position is inside;
/// `0` means "not caught by anything in this function," so a `raise`/an
/// uncaught call there really does need this function's own `can_raise`.
fn block_escapes_uncaught(block: &Block, try_depth: usize, can_raise: &[bool]) -> bool {
    block
        .iter()
        .any(|s| stmt_escapes_uncaught(s, try_depth, can_raise))
}

fn stmt_escapes_uncaught(stmt: &crate::ir::Stmt, try_depth: usize, can_raise: &[bool]) -> bool {
    use crate::ir::Stmt;
    match stmt {
        Stmt::Raise { .. } => try_depth == 0,
        Stmt::TryCatch { body, handler, .. } => {
            // `body`'s own raises/calls are caught by *this* try (this
            // function's `catch` clause is always `Error`/`UserRaised` —
            // see `ir.rs`'s `Stmt::TryCatch` doc — so every error this
            // function itself raises or receives is absorbed here); a
            // failure that already escaped an inner, nested try (deeper
            // `try_depth`) still stops right here too. `handler` runs at the
            // *current* depth — it isn't itself protected by the try it's
            // attached to.
            block_escapes_uncaught(body, try_depth + 1, can_raise)
                || block_escapes_uncaught(handler, try_depth, can_raise)
        }
        Stmt::Let { init, .. } => init
            .as_ref()
            .is_some_and(|init| expr_escapes_uncaught(init, try_depth, can_raise)),
        Stmt::Assign { value, .. } => expr_escapes_uncaught(value, try_depth, can_raise),
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            expr_escapes_uncaught(cond, try_depth, can_raise)
                || block_escapes_uncaught(then_branch, try_depth, can_raise)
                || block_escapes_uncaught(else_branch, try_depth, can_raise)
        }
        Stmt::While { cond, body } => {
            expr_escapes_uncaught(cond, try_depth, can_raise)
                || block_escapes_uncaught(body, try_depth, can_raise)
        }
        Stmt::ForIndex {
            low, high, body, ..
        } => {
            expr_escapes_uncaught(low, try_depth, can_raise)
                || expr_escapes_uncaught(high, try_depth, can_raise)
                || block_escapes_uncaught(body, try_depth, can_raise)
        }
        Stmt::ForEach { list, body, .. } => {
            expr_escapes_uncaught(list, try_depth, can_raise)
                || block_escapes_uncaught(body, try_depth, can_raise)
        }
        Stmt::Return(Some(e)) => expr_escapes_uncaught(e, try_depth, can_raise),
        Stmt::Return(None) => false,
        Stmt::Expr(e) => expr_escapes_uncaught(e, try_depth, can_raise),
    }
}

/// Whether `expr` (or anything nested inside it) is a `CallTarget::Fn` call
/// to an already-known `can_raise` function, at `try_depth == 0`. Recurses
/// into every `Expr` variant's own sub-expressions — a `can_raise` call
/// buried inside a `BinOp`/argument list/etc. still needs this function's
/// own result-ABI, and still needs its `is_err` branch checked at codegen
/// time regardless of how deeply nested it is (`keel-codegen`'s
/// `emit_call` does this inline, so no restriction on *where* a `can_raise`
/// call may appear is needed here beyond the try-depth check itself).
fn expr_escapes_uncaught(expr: &crate::ir::Expr, try_depth: usize, can_raise: &[bool]) -> bool {
    use crate::ir::Expr;
    match expr {
        Expr::ConstInt(_)
        | Expr::ConstFloat(_)
        | Expr::ConstBool(_)
        | Expr::ConstStr(_)
        | Expr::Local { .. }
        | Expr::MakeEnum { .. }
        | Expr::NullLit { .. } => false,
        Expr::UnOp { operand, .. } => expr_escapes_uncaught(operand, try_depth, can_raise),
        Expr::NullSome { value, .. } => expr_escapes_uncaught(value, try_depth, can_raise),
        Expr::BinOp { left, right, .. } => {
            expr_escapes_uncaught(left, try_depth, can_raise)
                || expr_escapes_uncaught(right, try_depth, can_raise)
        }
        Expr::NullCoalesce {
            nullable, fallback, ..
        } => {
            expr_escapes_uncaught(nullable, try_depth, can_raise)
                || expr_escapes_uncaught(fallback, try_depth, can_raise)
        }
        Expr::Index { list, index, .. } => {
            expr_escapes_uncaught(list, try_depth, can_raise)
                || expr_escapes_uncaught(index, try_depth, can_raise)
        }
        Expr::FieldGet { base, .. }
        | Expr::NullFieldGet { base, .. }
        | Expr::TupleGet { base, .. } => expr_escapes_uncaught(base, try_depth, can_raise),
        Expr::MakeStruct { fields, .. } => fields
            .iter()
            .any(|f| expr_escapes_uncaught(f, try_depth, can_raise)),
        Expr::MakeTuple { elems, .. } => elems
            .iter()
            .any(|e| expr_escapes_uncaught(e, try_depth, can_raise)),
        Expr::Call { target, args, .. } => {
            let self_escapes =
                try_depth == 0 && matches!(target, crate::ir::CallTarget::Fn(id) if can_raise[*id]);
            self_escapes
                || args
                    .iter()
                    .any(|a| expr_escapes_uncaught(a, try_depth, can_raise))
        }
    }
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

/// Whether `program` uses `raise`/`try`/`catch` anywhere in a task body or
/// a top-level statement — gates whether the synthetic `UserRaised` struct
/// (see `lower_program`'s Pass 1c) is registered at all.
fn program_uses_raise_or_try(program: &Program) -> bool {
    program.declarations.iter().any(|decl| match &decl.kind {
        Decl::Task(task) => block_uses_raise_or_try(&task.body),
        Decl::Stmt(stmt) => stmt_uses_raise_or_try(&stmt.kind),
        Decl::Type(_)
        | Decl::Interface(_)
        | Decl::Impl(_)
        | Decl::Test(_)
        | Decl::Extern(_)
        | Decl::Agent(_)
        | Decl::Use(_) => false,
    })
}

fn block_uses_raise_or_try(block: &keel_syntax::ast::Block) -> bool {
    block.iter().any(|stmt| stmt_uses_raise_or_try(&stmt.kind))
}

fn stmt_uses_raise_or_try(stmt: &keel_syntax::ast::Stmt) -> bool {
    use keel_syntax::ast::Stmt;
    match stmt {
        Stmt::Raise(_) | Stmt::TryCatch { .. } => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            block_uses_raise_or_try(then_body)
                || else_body.as_ref().is_some_and(block_uses_raise_or_try)
        }
        Stmt::While { body, .. } | Stmt::For { body, .. } => block_uses_raise_or_try(body),
        Stmt::When { arms, .. } => arms.iter().any(|arm| block_uses_raise_or_try(&arm.body)),
        Stmt::Let { .. }
        | Stmt::SelfAssign { .. }
        | Stmt::Return(_)
        | Stmt::AugAssign { .. }
        | Stmt::Assert { .. }
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Expr(_) => false,
    }
}

/// Converts a parsed type annotation to a `KirType`, rejecting every
/// variant outside the M0/M1/M2-so-far subset. `structs_by_name`/
/// `enums_by_name` resolve a bare `Named` type to a declared struct or enum
/// — checked after the built-in scalar names, so neither can shadow a
/// reserved type name.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ty_expr_to_kir(
    ty: &keel_syntax::ast::Node<keel_syntax::ast::TypeExpr>,
    structs_by_name: &HashMap<String, StructId>,
    enums_by_name: &HashMap<String, EnumId>,
    lists: &std::cell::RefCell<Vec<KirType>>,
    maps: &std::cell::RefCell<Vec<KirType>>,
    sets: &std::cell::RefCell<Vec<KirType>>,
    nullables: &std::cell::RefCell<Vec<KirType>>,
    tuples: &std::cell::RefCell<Vec<TupleLayout>>,
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
                maps,
                sets,
                nullables,
                tuples,
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
                maps,
                sets,
                nullables,
                tuples,
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
        TypeExpr::Map(key, value) => {
            // `TypeExpr::Map` boxes bare `TypeExpr`s, not `Node<TypeExpr>`s
            // (no span of their own), same situation as `TypeExpr::List`.
            let key_node = keel_syntax::ast::Node::new((**key).clone(), ty.span.clone());
            let key_ty = ty_expr_to_kir(
                &key_node,
                structs_by_name,
                enums_by_name,
                lists,
                maps,
                sets,
                nullables,
                tuples,
            )?;
            if key_ty != KirType::Str {
                return Err(LowerError::unsupported(
                    "map key type other than str (int/bool keys are a later M2/M3 concern)",
                    ty.span.clone(),
                ));
            }
            let value_node = keel_syntax::ast::Node::new((**value).clone(), ty.span.clone());
            let value_ty = ty_expr_to_kir(
                &value_node,
                structs_by_name,
                enums_by_name,
                lists,
                maps,
                sets,
                nullables,
                tuples,
            )?;
            if !is_list_element_ty(value_ty) {
                return Err(LowerError::unsupported(
                    "map value type other than int/float/bool/str (struct/enum values need \
                     Value marshaling, a later M2/M3 concern)",
                    ty.span.clone(),
                ));
            }
            Ok(KirType::Map(intern_map(maps, value_ty)))
        }
        TypeExpr::Set(inner) => {
            let inner_node = keel_syntax::ast::Node::new((**inner).clone(), ty.span.clone());
            let elem_ty = ty_expr_to_kir(
                &inner_node,
                structs_by_name,
                enums_by_name,
                lists,
                maps,
                sets,
                nullables,
                tuples,
            )?;
            if !is_list_element_ty(elem_ty) {
                return Err(LowerError::unsupported(
                    "set element type other than int/float/bool/str (struct/enum elements \
                     need Value marshaling, a later M2/M3 concern)",
                    ty.span.clone(),
                ));
            }
            Ok(KirType::Set(intern_set(sets, elem_ty)))
        }
        TypeExpr::Struct(_) => Err(LowerError::unsupported(
            "inline struct type",
            ty.span.clone(),
        )),
        TypeExpr::Tuple(items) => {
            // `TypeExpr::Tuple` holds bare `TypeExpr`s with no spans of their
            // own, same situation as `TypeExpr::List` above.
            let mut elems = Vec::with_capacity(items.len());
            for item in items {
                let item_node = keel_syntax::ast::Node::new(item.clone(), ty.span.clone());
                let elem_ty = ty_expr_to_kir(
                    &item_node,
                    structs_by_name,
                    enums_by_name,
                    lists,
                    maps,
                    sets,
                    nullables,
                    tuples,
                )?;
                if !is_tuple_element_ty(elem_ty) {
                    return Err(LowerError::unsupported(
                        "tuple element type other than int/float/bool/str or a nested tuple \
                         (container/struct/enum/nullable elements need the `Value` marshaling a \
                         by-value tuple deliberately avoids)",
                        ty.span.clone(),
                    ));
                }
                elems.push(elem_ty);
            }
            Ok(KirType::Tuple(intern_tuple(tuples, elems)))
        }
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

/// Interns `value` into `maps`, returning its `MapId` — same structural-
/// interning rationale as [`intern_list`] (the key is always `str`, so only
/// the value type needs interning — see `ir.rs`'s `MapId` doc).
pub(crate) fn intern_map(maps: &std::cell::RefCell<Vec<KirType>>, value: KirType) -> MapId {
    let mut maps = maps.borrow_mut();
    if let Some(id) = maps.iter().position(|t| *t == value) {
        return id;
    }
    maps.push(value);
    maps.len() - 1
}

/// Interns `elem` into `sets`, returning its `SetId` — same structural-
/// interning rationale as [`intern_list`].
pub(crate) fn intern_set(sets: &std::cell::RefCell<Vec<KirType>>, elem: KirType) -> SetId {
    let mut sets = sets.borrow_mut();
    if let Some(id) = sets.iter().position(|t| *t == elem) {
        return id;
    }
    sets.push(elem);
    sets.len() - 1
}

/// `true` for the element types a tuple can hold today. Wider than
/// [`is_list_element_ty`] in one direction — nested tuples are allowed, since
/// a by-value aggregate nests for free with no `Value` marshaling — and
/// deliberately excludes containers, structs, enums, and nullables, which
/// would need exactly that marshaling. See `ir.rs`'s `TupleLayout` doc.
pub(crate) fn is_tuple_element_ty(ty: KirType) -> bool {
    matches!(
        ty,
        KirType::I64 | KirType::F64 | KirType::Bool | KirType::Str | KirType::Tuple(_)
    )
}

/// Interns `elems` into `tuples`, returning its `TupleId` — same structural-
/// interning rationale as [`intern_list`]: `(str, int)` written twice in a
/// program is one `TupleId` (`SPEC.md` §2.8 makes tuples structural), unlike
/// `StructId`'s nominal, declaration-order interning.
pub(crate) fn intern_tuple(
    tuples: &std::cell::RefCell<Vec<TupleLayout>>,
    elems: Vec<KirType>,
) -> TupleId {
    let mut tuples = tuples.borrow_mut();
    if let Some(id) = tuples.iter().position(|t| t.elems == elems) {
        return id;
    }
    let id = tuples.len();
    tuples.push(TupleLayout { id, elems });
    id
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
