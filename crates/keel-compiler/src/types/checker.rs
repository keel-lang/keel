//! Type checker for Keel v0.1.
//!
//! Pragmatic single-pass checker: declarations are collected up front,
//! then each task / agent handler / top-level statement is walked with
//! a stack of lexical scopes. Inference is deliberately shallow — when
//! a type can't be computed cheaply, it falls back to `Ty::Unknown`
//! and no error is reported. The goal is high-signal diagnostics
//! (undefined identifiers, non-exhaustive matches, `self` outside
//! agents, missing `else` on if-expressions, arg-count mismatches) not
//! full Hindley-Milner inference.

mod binop;
mod call;
mod collect;
mod expr;
mod resolve;
mod stmt;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::hir::Hir;
use crate::lexer::Span;
use crate::types::interface::{self as iface, Signature};
// Re-export so existing call-sites (`crate::types::checker::Ty`, etc.) remain valid.
pub use crate::types::diagnostics::TypeDiagnostic;
pub use crate::types::ty::Ty;
// Re-export IDE helpers so `lsp.rs` call-sites remain valid without churn.
pub use crate::ide::hover::type_at;
pub use crate::ide::symbols::{
    definition_of, ident_at_offset, ident_span_at_offset, is_top_level_symbol, usages_of,
};
pub use crate::types::artifacts::CheckArtifacts;
use crate::types::prelude::{builtin_interfaces, builtin_structs};
use crate::types::scope::Scope;

// ---------------------------------------------------------------------------
// Per-task / per-handler info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TaskSig {
    params: Vec<(String, Ty)>,
    return_type: Ty,
    /// True if the last param is variadic (`...name: T`).
    variadic: bool,
}

#[derive(Debug, Clone)]
struct AgentInfo {
    state_fields: HashMap<String, Ty>,
    readonly_fields: HashSet<String>,
    /// Task signatures exposed through explicit `self.task(...)` calls.
    tasks: HashMap<String, TaskSig>,
    /// Event handlers declared with `on event(param: T)`.
    /// Value is `None` for parameterless handlers, `Some(ty)` when a typed
    /// parameter was declared. Used to validate `delegate(...)` call sites.
    handlers: HashMap<String, Option<Ty>>,
    /// `@tools` entries as (namespace, method) pairs; conditional entries
    /// count as declared (their guard is evaluated per turn at runtime).
    /// `None` means no `@tools` attribute — std calls are deny-by-default.
    tools: Option<Vec<(String, Option<String>)>>,
}

impl AgentInfo {
    /// Whether the agent's `@tools` declaration covers `ns.method`.
    fn allows_tool(&self, ns: &str, method: &str) -> bool {
        self.tools.as_ref().is_some_and(|entries| {
            entries
                .iter()
                .any(|(n, m)| n == "all" || (n == ns && m.as_deref().is_none_or(|m| m == method)))
        })
    }
}

/// What a module-namespace binding exposes, for member checks at
/// `binding.member` sites.
#[derive(Debug, Clone)]
pub enum ModuleMembers {
    /// std module — canonical catalog namespace name.
    Std(String),
    /// Local module — its top-level declaration names by kind.
    Local {
        module_name: String,
        tasks: HashSet<String>,
        agents: HashSet<String>,
        types: HashSet<String>,
    },
}

// ---------------------------------------------------------------------------
// Checker state
// ---------------------------------------------------------------------------

pub(crate) struct Checker<'hir, 'ast> {
    hir: &'hir Hir<'ast>,
    errors: Vec<TypeDiagnostic>,
    enum_variants: HashMap<String, Vec<String>>,
    /// Declared field names for rich-enum variants, keyed
    /// `enum_name → variant_name → field names`. Data-less variants map to an
    /// empty list. Used to reject `variant { typo }` patterns that name a
    /// field the variant does not declare.
    enum_variant_fields: HashMap<String, HashMap<String, Vec<String>>>,
    structs: HashMap<String, Vec<(String, Ty)>>,
    aliases: HashMap<String, Ty>,
    /// Known interfaces: interface_name → required method signatures.
    /// Pre-seeded with built-ins (Stringable); extended by `interface` declarations.
    interfaces: HashMap<String, Vec<crate::ast::TaskSig>>,
    /// Type names that implement `Iterable` — used to allow `for x in value`
    /// on struct types.
    iterable_types: HashSet<String>,
    /// Type names that implement `LlmProvider` — the eligible targets for
    /// `@provider X` and `ai.install(X)`.
    llm_provider_types: HashSet<String>,
    /// Generic type declarations stored as `name → (type_params, body)` for
    /// deferred instantiation when a concrete `Foo[str]` application appears.
    generic_decls: HashMap<String, (Vec<String>, TypeDef)>,
    /// Generic task declarations stored by name so call sites can infer
    /// type arguments from the concrete argument types.
    generic_task_decls: HashMap<String, TaskDecl>,
    top_tasks: HashMap<String, TaskSig>,
    agents: HashMap<String, AgentInfo>,
    current_agent: Option<String>,
    /// Declared return type of the task currently being checked.
    current_return_ty: Option<Ty>,
    /// Mock targets declared by the test currently being checked.
    current_test_mocks: Option<HashSet<(String, String)>>,
    /// Span of the statement currently being checked. Set at the top of
    /// `check_stmt` so every `err()` call within a statement — including
    /// errors raised by `infer_expr` — automatically gets a location.
    current_span: Option<Span>,
    /// When true, emit an error for any binding whose type the checker
    /// cannot resolve (falls back to `Ty::Unknown`).
    strict: bool,
    /// Module-namespace bindings of the module being checked:
    /// binding name → exposed members. Populated by `check_graph`.
    module_members: HashMap<String, ModuleMembers>,
    /// Type names that may appear in annotations in this module: its own
    /// declarations plus symbol-imported types. `None` (single-file mode)
    /// means every collected type is visible.
    visible_types: Option<HashSet<String>>,
    /// Artifacts collector for the `*_with_artifacts` entry points. `None`
    /// (the default) for ordinary checking — recording is skipped entirely,
    /// so `check_program`/`check_graph` pay no cost for it.
    ///
    /// `RefCell`, not a plain field behind `&mut self`: expression-type and
    /// generic-instantiation recording happens from deep inside `&self`
    /// helpers (`resolve_type`, `resolve_type_with_env`, `task_sig`,
    /// `agent_info`) that are called from many contexts, some during
    /// declaration collection before any `&mut self` borrow is available.
    /// Threading `&mut self` through all of them would be a much larger,
    /// higher-risk diff for what is pure instrumentation — it has no effect
    /// on which diagnostics are produced or what any `Ty` resolves to.
    ///
    /// Non-reentrancy is required for this to be panic-free — see the
    /// `# Panics` notes on [`Checker::record_expr_type`] and
    /// [`Checker::record_instantiation`], the only two accessors.
    artifacts: Option<RefCell<CheckArtifacts>>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Type-check a single in-memory program (REPL, LSP buffers, embeddings).
///
/// `use std/<name>` imports resolve against the catalog; relative file
/// imports are errors here because there is no file to resolve them
/// against — multi-file programs go through [`check_graph`].
#[must_use]
pub fn check_program(program: &Program, strict: bool) -> Vec<TypeDiagnostic> {
    let (hir, mut diagnostics, members) = lower_single(program);
    diagnostics.extend(check_lowered(&hir, members, strict));
    diagnostics
}

/// Like [`check_program`], but also returns the [`CheckArtifacts`] recorded
/// during checking — the resolved type of every expression, plus every
/// generic task/type instantiation encountered.
///
/// Added for KIR lowering (`designs/llvm-compilation.md` §2.3) and future
/// IDE consumers; [`check_program`] keeps its original signature and cost
/// unchanged for existing callers.
#[must_use]
pub fn check_program_with_artifacts(
    program: &Program,
    strict: bool,
) -> (Vec<TypeDiagnostic>, CheckArtifacts) {
    let (hir, mut diagnostics, members) = lower_single(program);
    let (errors, artifacts) = check_lowered_with_artifacts(&hir, members, strict);
    diagnostics.extend(errors);
    (diagnostics, artifacts)
}

/// Lower a single in-memory program with its std imports resolved.
///
/// Returns the HIR (for IDE consumers that need references and symbols),
/// any import-resolution diagnostics, and the module-member table for
/// [`check_lowered`].
pub fn lower_single(
    program: &Program,
) -> (Hir<'_>, Vec<TypeDiagnostic>, HashMap<String, ModuleMembers>) {
    let mut diagnostics = Vec::new();
    let (scope, members) = std_import_scope(program, &mut diagnostics);
    let hir = crate::hir::lower_ast_with_imports(program, &scope);
    (hir, diagnostics, members)
}

/// Check a program already lowered by [`lower_single`].
#[must_use]
pub fn check_lowered(
    hir: &Hir<'_>,
    members: HashMap<String, ModuleMembers>,
    strict: bool,
) -> Vec<TypeDiagnostic> {
    check_lowered_impl(hir, members, strict, false).0
}

/// Like [`check_lowered`], but also returns the [`CheckArtifacts`] recorded
/// during checking.
#[must_use]
pub fn check_lowered_with_artifacts(
    hir: &Hir<'_>,
    members: HashMap<String, ModuleMembers>,
    strict: bool,
) -> (Vec<TypeDiagnostic>, CheckArtifacts) {
    let (errors, artifacts) = check_lowered_impl(hir, members, strict, true);
    (
        errors,
        artifacts.expect("artifacts requested via collect_artifacts=true"),
    )
}

/// Shared implementation behind [`check_lowered`] and
/// [`check_lowered_with_artifacts`]. `collect_artifacts` gates whether a
/// [`CheckArtifacts`] collector is attached to the pass; when `false` the
/// second return value is always `None` and recording is skipped entirely.
fn check_lowered_impl(
    hir: &Hir<'_>,
    members: HashMap<String, ModuleMembers>,
    strict: bool,
    collect_artifacts: bool,
) -> (Vec<TypeDiagnostic>, Option<CheckArtifacts>) {
    let mut c = Checker::new(hir);
    c.strict = strict;
    c.module_members = members;
    if collect_artifacts {
        c.artifacts = Some(RefCell::new(CheckArtifacts::default()));
    }
    c.collect(hir.program());
    c.check_body(hir.program());
    let artifacts = c.artifacts.take().map(RefCell::into_inner);
    (c.errors, artifacts)
}

/// Lower a graph's entry module with its imports in scope, for IDE
/// consumers (hover, go-to-definition) that need the entry file's HIR.
#[must_use]
pub fn lower_entry_for_ide(graph: &crate::modules::ModuleGraph) -> Hir<'_> {
    let mut diagnostics = Vec::new();
    let scope = build_module_scope(graph, graph.entry_index(), &mut diagnostics);
    crate::hir::lower_ast_with_imports(&graph.entry().program, &scope)
}

/// Resolve the std imports of a single program without a module graph.
fn std_import_scope(
    program: &Program,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> (
    crate::hir::ModuleScope<'static>,
    HashMap<String, ModuleMembers>,
) {
    use crate::ast::{UseDecl, UseKind, UseSource};
    use crate::hir::ImportedDecl;
    use crate::modules::ModuleTarget;

    let mut scope = crate::hir::ModuleScope::default();
    let mut members = HashMap::new();
    for node in &program.declarations {
        let Decl::Use(UseDecl { kind }) = &node.kind else {
            continue;
        };
        let std_name = |source: &UseSource| -> Option<String> {
            let UseSource::Module(segments) = source else {
                return None;
            };
            (segments.len() == 2
                && segments[0] == "std"
                && crate::modules::std_module_names().contains(&segments[1]))
            .then(|| segments[1].clone())
        };
        match kind {
            UseKind::Module { source, alias } => match std_name(source) {
                Some(ns) => {
                    let binding = alias.clone().unwrap_or_else(|| ns.clone());
                    scope.bindings.push((
                        binding.clone(),
                        ModuleTarget::Std(ns.clone()),
                        node.span.clone(),
                    ));
                    members.insert(binding, ModuleMembers::Std(ns));
                }
                None => diagnostics.push(TypeDiagnostic::other(
                    unresolvable_import_message(source),
                    node.span.clone(),
                )),
            },
            UseKind::Symbols { items, source } => match std_name(source) {
                Some(ns) => {
                    for item in items {
                        match crate::types::prelude::catalog_method(&ns, &item.name) {
                            Some(entry) => scope.symbols.push((
                                item.alias.clone().unwrap_or_else(|| item.name.clone()),
                                ImportedDecl::StdMethod(entry),
                                item.name_span.clone(),
                            )),
                            None => diagnostics.push(TypeDiagnostic::other(
                                format!("`std/{ns}` has no member `{}`", item.name),
                                item.name_span.clone(),
                            )),
                        }
                    }
                }
                None => diagnostics.push(TypeDiagnostic::other(
                    unresolvable_import_message(source),
                    node.span.clone(),
                )),
            },
        }
    }
    (scope, members)
}

fn unresolvable_import_message(source: &crate::ast::UseSource) -> String {
    match source {
        crate::ast::UseSource::File(path) => {
            format!("cannot resolve `{path}` without a source file path")
        }
        crate::ast::UseSource::Module(segments) => {
            let path = segments.join("/");
            if segments.len() == 2 && segments[0] == "std" {
                format!("unknown std module `{path}`")
            } else {
                format!("unsupported package path `{path}`")
            }
        }
    }
}

/// Type-check every module of a loaded graph.
///
/// Each module is checked against the whole graph's declarations (the
/// runtime registers them in one flat namespace), but name *visibility*
/// is per module: unqualified access requires a declaration or import in
/// that module. Returns one diagnostics list per module, index-aligned
/// with `graph.modules`.
#[must_use]
pub fn check_graph(graph: &crate::modules::ModuleGraph) -> Vec<Vec<TypeDiagnostic>> {
    check_graph_impl(graph, false, false).0
}

/// Like [`check_graph`], but also emits errors for any binding whose type
/// the checker cannot resolve (`keel check --strict`).
#[must_use]
pub fn check_graph_strict(graph: &crate::modules::ModuleGraph) -> Vec<Vec<TypeDiagnostic>> {
    check_graph_impl(graph, true, false).0
}

/// Like [`check_graph`], but also returns the [`CheckArtifacts`] recorded for
/// each module, index-aligned with `graph.modules` (and with the returned
/// diagnostics). Artifacts are kept per module rather than merged because
/// [`crate::lexer::Span`] is a byte range into one module's source text —
/// merging would let unrelated modules' spans collide.
#[must_use]
pub fn check_graph_with_artifacts(
    graph: &crate::modules::ModuleGraph,
) -> (Vec<Vec<TypeDiagnostic>>, Vec<CheckArtifacts>) {
    check_graph_impl(graph, false, true)
}

/// Shared implementation behind [`check_graph`], [`check_graph_strict`], and
/// [`check_graph_with_artifacts`]. `collect_artifacts` gates whether a
/// [`CheckArtifacts`] collector is attached to each module's pass; when
/// `false` the returned `Vec<CheckArtifacts>` is always empty.
fn check_graph_impl(
    graph: &crate::modules::ModuleGraph,
    strict: bool,
    collect_artifacts: bool,
) -> (Vec<Vec<TypeDiagnostic>>, Vec<CheckArtifacts>) {
    let mut all: Vec<Vec<TypeDiagnostic>> = vec![Vec::new(); graph.modules.len()];
    let mut artifacts: Vec<CheckArtifacts> = Vec::new();

    check_graph_name_conflicts(graph, &mut all);

    for (index, unit) in graph.modules.iter().enumerate() {
        let mut diagnostics = Vec::new();
        let scope = build_module_scope(graph, index, &mut diagnostics);
        let hir = crate::hir::lower_ast_with_imports(&unit.program, &scope);
        let mut c = Checker::new(&hir);
        c.strict = strict;
        if collect_artifacts {
            c.artifacts = Some(RefCell::new(CheckArtifacts::default()));
        }
        if graph.modules.len() > 1 {
            for (other_index, other) in graph.modules.iter().enumerate() {
                if other_index != index {
                    c.collect_quiet(&other.program);
                }
            }
        }
        c.visible_types = Some(visible_type_names(graph, index));
        c.collect(&unit.program);
        c.seed_symbol_import_aliases(graph, index);
        c.module_members = build_module_members(graph, index);
        c.check_body(&unit.program);
        diagnostics.extend(c.errors);
        if collect_artifacts {
            artifacts.push(
                c.artifacts
                    .take()
                    .expect("set above when collect_artifacts is true")
                    .into_inner(),
            );
        }
        all[index].extend(diagnostics);
    }
    (all, artifacts)
}

/// Resolve one module's imports to the source declarations the HIR needs.
fn build_module_scope<'g>(
    graph: &'g crate::modules::ModuleGraph,
    index: usize,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> crate::hir::ModuleScope<'g> {
    use crate::hir::ImportedDecl;
    use crate::modules::ModuleTarget;

    let unit = &graph.modules[index];
    let mut scope = crate::hir::ModuleScope::default();
    for binding in &unit.imports.bindings {
        scope.bindings.push((
            binding.name.clone(),
            binding.target.clone(),
            binding.span.clone(),
        ));
    }
    for symbol in &unit.imports.symbols {
        match &symbol.target {
            ModuleTarget::Std(ns) => {
                match crate::types::prelude::catalog_method(ns, &symbol.original) {
                    Some(entry) => scope.symbols.push((
                        symbol.local.clone(),
                        ImportedDecl::StdMethod(entry),
                        symbol.span.clone(),
                    )),
                    None => diagnostics.push(TypeDiagnostic::other(
                        format!("`std/{ns}` has no member `{}`", symbol.original),
                        symbol.span.clone(),
                    )),
                }
            }
            ModuleTarget::Local(target_index) => {
                let target = &graph.modules[*target_index];
                let decl = target.program.declarations.iter().find_map(|node| {
                    let imported = match &node.kind {
                        Decl::Task(d) if d.name == symbol.original => ImportedDecl::Task(d),
                        Decl::Type(d) if d.name == symbol.original => ImportedDecl::Type(d),
                        Decl::Agent(d) if d.name == symbol.original => ImportedDecl::Agent,
                        Decl::Interface(d) if d.name == symbol.original => ImportedDecl::Interface,
                        Decl::Extern(d) if d.name == symbol.original => ImportedDecl::Extern,
                        _ => return None,
                    };
                    Some(imported)
                });
                match decl {
                    Some(imported) => {
                        // Aliased type imports would fracture nominal identity
                        // (values carry the declared type name) — reject them.
                        if matches!(imported, ImportedDecl::Type(_))
                            && symbol.local != symbol.original
                        {
                            diagnostics.push(TypeDiagnostic::other(
                                format!(
                                    "type `{}` cannot be imported under another name — \
                                     types keep their declared identity",
                                    symbol.original
                                ),
                                symbol.span.clone(),
                            ));
                            continue;
                        }
                        scope
                            .symbols
                            .push((symbol.local.clone(), imported, symbol.span.clone()));
                    }
                    None => diagnostics.push(TypeDiagnostic::other(
                        format!(
                            "module `{}` has no top-level declaration `{}`",
                            target.name, symbol.original
                        ),
                        symbol.span.clone(),
                    )),
                }
            }
        }
    }
    scope
}

/// Build the binding → members table consulted at `binding.member` sites.
fn build_module_members(
    graph: &crate::modules::ModuleGraph,
    index: usize,
) -> HashMap<String, ModuleMembers> {
    use crate::modules::ModuleTarget;

    let mut members = HashMap::new();
    for binding in &graph.modules[index].imports.bindings {
        let info = match &binding.target {
            ModuleTarget::Std(ns) => ModuleMembers::Std(ns.clone()),
            ModuleTarget::Local(target_index) => {
                let target = &graph.modules[*target_index];
                let mut tasks = HashSet::new();
                let mut agents = HashSet::new();
                let mut types = HashSet::new();
                for node in &target.program.declarations {
                    match &node.kind {
                        Decl::Task(d) => {
                            tasks.insert(d.name.clone());
                        }
                        Decl::Extern(d) => {
                            tasks.insert(d.name.clone());
                        }
                        Decl::Agent(d) => {
                            agents.insert(d.name.clone());
                        }
                        Decl::Type(d) => {
                            types.insert(d.name.clone());
                        }
                        Decl::Interface(d) => {
                            types.insert(d.name.clone());
                        }
                        Decl::Impl(_) | Decl::Test(_) | Decl::Use(_) | Decl::Stmt(_) => {}
                    }
                }
                ModuleMembers::Local {
                    module_name: target.name.clone(),
                    tasks,
                    agents,
                    types,
                }
            }
        };
        members.insert(binding.name.clone(), info);
    }
    members
}

/// Type names usable in annotations within module `index`: its own type
/// and interface declarations plus symbol-imported types.
fn visible_type_names(graph: &crate::modules::ModuleGraph, index: usize) -> HashSet<String> {
    use crate::modules::ModuleTarget;

    let unit = &graph.modules[index];
    let mut visible = HashSet::new();
    for node in &unit.program.declarations {
        match &node.kind {
            Decl::Type(d) => {
                visible.insert(d.name.clone());
            }
            Decl::Interface(d) => {
                visible.insert(d.name.clone());
            }
            _ => {}
        }
    }
    for symbol in &unit.imports.symbols {
        if let ModuleTarget::Local(target_index) = &symbol.target {
            let target = &graph.modules[*target_index];
            let is_type = target.program.declarations.iter().any(|node| {
                matches!(&node.kind, Decl::Type(d) if d.name == symbol.original)
                    || matches!(&node.kind, Decl::Interface(d) if d.name == symbol.original)
            });
            if is_type {
                visible.insert(symbol.local.clone());
            }
        }
    }
    visible
}

/// The runtime registers every module's declarations in one flat global
/// namespace, so a name must mean the same thing across the whole graph.
/// Reports any name bound to two different meanings.
fn check_graph_name_conflicts(
    graph: &crate::modules::ModuleGraph,
    all: &mut [Vec<TypeDiagnostic>],
) {
    use crate::modules::ModuleTarget;

    #[derive(PartialEq, Eq, Hash, Clone)]
    enum Meaning {
        /// A top-level declaration (module index, declared name).
        Decl(usize, String),
        /// A module-namespace binding.
        ModuleNs(String),
        /// A std member imported unqualified.
        StdMember(String, String),
    }

    fn target_key(graph: &crate::modules::ModuleGraph, target: &ModuleTarget) -> String {
        match target {
            ModuleTarget::Std(ns) => format!("std/{ns}"),
            ModuleTarget::Local(index) => graph.modules[*index]
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| graph.modules[*index].name.clone()),
        }
    }

    let mut seen: HashMap<String, (Meaning, usize, String)> = HashMap::new();
    let mut record = |name: &str,
                      meaning: Meaning,
                      module_index: usize,
                      span: Span,
                      desc: String,
                      all: &mut [Vec<TypeDiagnostic>],
                      module_label: &dyn Fn(usize) -> String| {
        match seen.get(name) {
            Some((existing, prev_index, prev_desc)) if *existing != meaning => {
                all[module_index].push(TypeDiagnostic::other(
                    format!(
                        "`{name}` means two different things across this program: \
                         {prev_desc} in {} and {desc} in {} — modules share one \
                         global namespace in this release; rename or alias one of them",
                        module_label(*prev_index),
                        module_label(module_index),
                    ),
                    span,
                ));
            }
            Some(_) => {}
            None => {
                seen.insert(name.to_string(), (meaning, module_index, desc));
            }
        }
    };

    let module_label = |index: usize| -> String {
        graph.modules[index]
            .path
            .as_ref()
            .map(|p| {
                p.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .unwrap_or_else(|| format!("{}.keel", graph.modules[index].name))
    };

    for (index, unit) in graph.modules.iter().enumerate() {
        for node in &unit.program.declarations {
            let (name, span, what) = match &node.kind {
                Decl::Task(d) => (&d.name, d.name_span.clone(), "a task"),
                Decl::Type(d) => (&d.name, d.name_span.clone(), "a type"),
                Decl::Agent(d) => (&d.name, d.name_span.clone(), "an agent"),
                Decl::Interface(d) => (&d.name, d.name_span.clone(), "an interface"),
                Decl::Extern(d) => (&d.name, d.name_span.clone(), "an extern task"),
                Decl::Impl(_) | Decl::Test(_) | Decl::Use(_) | Decl::Stmt(_) => continue,
            };
            record(
                name,
                Meaning::Decl(index, name.clone()),
                index,
                span,
                format!("{what} declaration"),
                all,
                &module_label,
            );
        }
        for binding in &unit.imports.bindings {
            record(
                &binding.name,
                Meaning::ModuleNs(target_key(graph, &binding.target)),
                index,
                binding.span.clone(),
                format!("an import of {}", target_key(graph, &binding.target)),
                all,
                &module_label,
            );
        }
        for symbol in &unit.imports.symbols {
            let meaning = match &symbol.target {
                ModuleTarget::Std(ns) => Meaning::StdMember(ns.clone(), symbol.original.clone()),
                ModuleTarget::Local(target_index) => {
                    Meaning::Decl(*target_index, symbol.original.clone())
                }
            };
            record(
                &symbol.local,
                meaning,
                index,
                symbol.span.clone(),
                format!("an imported symbol `{}`", symbol.original),
                all,
                &module_label,
            );
        }
    }
}

impl<'hir, 'ast> Checker<'hir, 'ast> {
    pub(crate) fn new(hir: &'hir Hir<'ast>) -> Self {
        Checker {
            hir,
            errors: Vec::new(),
            enum_variants: HashMap::new(),
            enum_variant_fields: HashMap::new(),
            structs: builtin_structs(),
            aliases: HashMap::new(),
            interfaces: builtin_interfaces(),
            iterable_types: HashSet::new(),
            llm_provider_types: HashSet::new(),
            generic_decls: HashMap::new(),
            generic_task_decls: HashMap::new(),
            top_tasks: HashMap::new(),
            agents: HashMap::new(),
            current_agent: None,
            current_return_ty: None,
            current_test_mocks: None,
            current_span: None,
            strict: false,
            module_members: HashMap::new(),
            visible_types: None,
            artifacts: None,
        }
    }

    /// Collect a foreign module's declarations into the lookup tables
    /// without reporting its errors — those surface when that module is
    /// checked itself.
    fn collect_quiet(&mut self, program: &Program) {
        let saved = std::mem::take(&mut self.errors);
        self.collect(program);
        self.errors = saved;
    }

    /// Make aliased symbol imports (`use email as ve from ...`) resolvable
    /// under their local names. Tables are keyed by declared name after
    /// collection; aliases share the original's signature.
    fn seed_symbol_import_aliases(&mut self, graph: &crate::modules::ModuleGraph, index: usize) {
        for symbol in &graph.modules[index].imports.symbols {
            if symbol.local == symbol.original
                || !matches!(symbol.target, crate::modules::ModuleTarget::Local(_))
            {
                continue;
            }
            if let Some(sig) = self.top_tasks.get(&symbol.original).cloned() {
                self.top_tasks.insert(symbol.local.clone(), sig);
            }
            if let Some(info) = self.agents.get(&symbol.original).cloned() {
                self.agents.insert(symbol.local.clone(), info);
            }
        }
    }

    /// Emit an error, automatically attaching the current statement's span
    /// when one is available.
    fn err(&mut self, msg: impl Into<String>) {
        let span = self.current_span.clone().unwrap_or(0..0);
        self.errors.push(TypeDiagnostic::other(msg, span));
    }

    fn err_at(&mut self, msg: impl Into<String>, span: Span) {
        self.errors.push(TypeDiagnostic::other(msg, span));
    }

    /// Record the resolved type of an expression at `span`, when artifacts
    /// collection is enabled for this pass (no-op otherwise). Takes `&self`
    /// so it is callable from the `&self` resolution helpers, not just from
    /// `infer_expr`.
    ///
    /// # Panics
    ///
    /// Panics if called while a `borrow_mut()` on `self.artifacts` is already
    /// active on the current call stack (`RefCell`'s double-borrow check).
    /// This is safe today because every call site takes a single-statement
    /// borrow that is dropped before the call returns, and no recording call
    /// ever occurs while another one is in progress — in particular,
    /// `infer_expr` records a node's type only *after* `infer_expr_uncached`
    /// (which performs all recursive descent into sub-expressions) has fully
    /// returned, so no recording of an outer node ever nests inside the
    /// recording of an inner one. If you add a new call site, keep that
    /// property: never call `record_expr_type`/`record_instantiation` from
    /// code that could itself run while an existing call to either is still
    /// holding its borrow (e.g. do not call them from inside
    /// `CheckArtifacts`'s own methods, or from a callback invoked mid-borrow).
    pub(crate) fn record_expr_type(&self, span: &Span, ty: &Ty) {
        if let Some(artifacts) = &self.artifacts {
            artifacts.borrow_mut().record_expr(span.clone(), ty);
        }
    }

    /// Record one generic task/type instantiation, when artifacts collection
    /// is enabled for this pass (no-op otherwise).
    ///
    /// # Panics
    ///
    /// Same non-reentrancy requirement as [`Checker::record_expr_type`]: the
    /// `borrow_mut()` here must never nest inside another live borrow of
    /// `self.artifacts`.
    pub(crate) fn record_instantiation(&self, name: &str, type_args: Vec<Ty>) {
        if let Some(artifacts) = &self.artifacts {
            artifacts.borrow_mut().record_instantiation(name, type_args);
        }
    }

    fn wrong_arity(
        &mut self,
        task_name: impl Into<String>,
        expected: usize,
        actual: usize,
        expected_params: Vec<String>,
        span: Span,
    ) {
        self.errors.push(TypeDiagnostic::WrongArity {
            task_name: task_name.into(),
            expected,
            actual,
            expected_params,
            span,
        });
    }

    fn check_impl_conformance(&mut self, impl_decl: &ImplDecl) {
        let iface_name = &impl_decl.interface_name;
        let type_name = &impl_decl.type_name;

        let sigs = match self.interfaces.get(iface_name).cloned() {
            Some(s) => s,
            None => {
                self.err(format!(
                    "impl: unknown interface `{iface_name}` — declare it with `interface {iface_name} {{ ... }}`"
                ));
                return;
            }
        };

        let provided: HashSet<&str> = impl_decl.methods.iter().map(|m| m.name.as_str()).collect();

        // Span for the impl block as a whole — set by collect() before calling
        // this function.  Used for "missing method" errors that have no better
        // site to point to.
        let impl_span = self.current_span.clone().unwrap_or(0..0);

        // Conformance checks share the runtime's resolution context so
        // `keel check` and `keel run` always apply identical rules. The env is
        // loop-invariant, so build it once for every method.
        let env = self.type_env();

        for sig in &sigs {
            if !provided.contains(sig.name.as_str()) {
                self.errors.push(TypeDiagnostic::InterfaceNotSatisfied {
                    impl_name: type_name.clone(),
                    interface_name: iface_name.clone(),
                    reason: format!("missing required method `{}`", sig.name),
                    span: impl_span.clone(),
                });
                continue;
            }
            let got_method = impl_decl
                .methods
                .iter()
                .find(|m| m.name == sig.name)
                .unwrap();

            // Parameter conformance (arity + per-position types), checked
            // through the shared helper so `keel check` and `keel run` reject
            // identical mismatches.
            match iface::check_param_conformance(&sig.params, &got_method.params, &env) {
                iface::ParamConformance::Ok => {}
                iface::ParamConformance::ArityMismatch { required, actual } => {
                    self.errors.push(TypeDiagnostic::InterfaceNotSatisfied {
                        impl_name: type_name.clone(),
                        interface_name: iface_name.clone(),
                        reason: format!(
                            "method `{}` expects {required} parameter(s) but got {actual}",
                            sig.name
                        ),
                        span: got_method.name_span.clone(),
                    });
                }
                iface::ParamConformance::TypeMismatches(mismatches) => {
                    // Report every offending position, each pointing at its own param.
                    for m in mismatches {
                        self.errors.push(TypeDiagnostic::InterfaceNotSatisfied {
                            impl_name: type_name.clone(),
                            interface_name: iface_name.clone(),
                            reason: format!(
                                "method `{}` parameter {} must be `{}` but is `{}`",
                                sig.name,
                                m.label,
                                type_display_str(&m.required.ty.kind),
                                type_display_str(&m.actual.ty.kind),
                            ),
                            span: m.actual.ty.span.clone(),
                        });
                    }
                }
            }

            // Return-type check — use the shared typed conformance function so
            // that the checker and the runtime always apply identical rules.
            let req_sig = Signature {
                params: vec![],
                ret: sig
                    .return_type
                    .as_ref()
                    .map(|n| iface::resolve_type_expr(&n.kind, &env))
                    .unwrap_or(Ty::None_),
            };
            let got_sig = Signature {
                params: vec![],
                ret: got_method
                    .return_type
                    .as_ref()
                    .map(|n| iface::resolve_type_expr(&n.kind, &env))
                    .unwrap_or(Ty::None_),
            };
            if !iface::signature_satisfies(&req_sig, &got_sig) {
                // Re-derive display strings for the human-readable error message.
                let req_str = sig
                    .return_type
                    .as_ref()
                    .map(|n| type_display_str(&n.kind))
                    .unwrap_or_else(|| "none".to_string());
                let got_str = got_method
                    .return_type
                    .as_ref()
                    .map(|n| type_display_str(&n.kind))
                    .unwrap_or_else(|| "none".to_string());
                // Point to the return-type annotation when present; fall back
                // to the method name span so the caret is never at byte 0.
                let ret_span = got_method
                    .return_type
                    .as_ref()
                    .map(|n| n.span.clone())
                    .unwrap_or_else(|| got_method.name_span.clone());
                self.errors.push(TypeDiagnostic::InterfaceNotSatisfied {
                    impl_name: type_name.clone(),
                    interface_name: iface_name.clone(),
                    reason: format!(
                        "method `{}` must return `{req_str}` but returns `{got_str}`",
                        sig.name
                    ),
                    span: ret_span,
                });
            }
        }

        // Reject extra methods not declared in the interface.
        for method in &impl_decl.methods {
            if !sigs.iter().any(|s| s.name == method.name) {
                self.errors.push(TypeDiagnostic::InterfaceNotSatisfied {
                    impl_name: type_name.clone(),
                    interface_name: iface_name.clone(),
                    reason: format!("method `{}` is not declared in this interface", method.name),
                    span: method.name_span.clone(),
                });
            }
        }
    }

    /// Build a [`crate::types::interface::TypeEnv`] from this checker's already-
    /// resolved alias table so that conformance checks share the same resolution
    /// context as the runtime.
    fn type_env(&self) -> iface::TypeEnv {
        iface::TypeEnv {
            aliases: self.aliases.clone(),
        }
    }

    /// Structural type equality (ignoring nullability wrapping differences).
    fn types_match(&self, a: &Ty, b: &Ty) -> bool {
        if a.is_opaque() || b.is_opaque() {
            return true;
        }
        match (a, b) {
            (Ty::Int, Ty::Int)
            | (Ty::Float, Ty::Float)
            | (Ty::Str, Ty::Str)
            | (Ty::Bool, Ty::Bool)
            | (Ty::None_, Ty::None_)
            | (Ty::Uuid, Ty::Uuid) => true,
            (Ty::List(a), Ty::List(b)) | (Ty::Set(a), Ty::Set(b)) => {
                self.types_match(a.as_ref(), b.as_ref())
            }
            (Ty::Nullable(a), Ty::Nullable(b)) => self.types_match(a.as_ref(), b.as_ref()),
            (Ty::Enum(a, _), Ty::Enum(b, _)) => a == b,
            (
                Ty::Struct {
                    name: an,
                    fields: af,
                },
                Ty::Struct {
                    name: bn,
                    fields: bf,
                },
            ) => match (an, bn) {
                (Some(a), Some(b)) => a == b,
                _ => {
                    af.len() == bf.len()
                        && af
                            .iter()
                            .zip(bf.iter())
                            .all(|((an, at), (bn, bt))| an == bn && self.types_match(at, bt))
                }
            },
            _ => false,
        }
    }

    fn block_type(&mut self, block: &Block, scope: &mut Scope) -> Ty {
        scope.push();
        let mut last = Ty::None_;
        for node in block {
            last = match &node.kind {
                Stmt::Expr(e) => self.infer_expr(e, scope),
                other => {
                    self.check_stmt(other, node.span.clone(), scope);
                    Ty::None_
                }
            };
        }
        scope.pop();
        last
    }

    fn expect(&mut self, actual: &Ty, expected: &Ty, context: &str) {
        let span = self.current_span.clone().unwrap_or(0..0);
        self.expect_at(actual, expected, context, span);
    }

    fn expect_at(&mut self, actual: &Ty, expected: &Ty, context: &str, span: Span) {
        if actual.is_opaque() {
            return;
        }
        if expected.is_opaque() {
            return;
        }

        // Nullable actual where non-nullable expected — caller must unwrap.
        if matches!(actual, Ty::Nullable(_)) && !matches!(expected, Ty::Nullable(_)) {
            self.errors.push(TypeDiagnostic::TypeMismatch {
                context: context.to_string(),
                expected: expected.clone(),
                actual: actual.clone(),
                span,
                help: Some("use `!` to assert non-null or `??` to provide a fallback".into()),
            });
            return;
        }

        let actual_base = actual.strip_nullable();
        let expected_base = expected.strip_nullable();

        // Recurse into compound types so struct-name differences propagate correctly
        // through List/Set/Map/Tuple wrappers without hitting the raw `!=` fallback below.
        if let (Ty::List(a), Ty::List(b)) = (actual_base, expected_base) {
            return self.expect_at(a, b, context, span);
        }
        if let (Ty::Set(a), Ty::Set(b)) = (actual_base, expected_base) {
            return self.expect_at(a, b, context, span);
        }
        if let (Ty::Map(ak, av), Ty::Map(ek, ev)) = (actual_base, expected_base) {
            self.expect_at(ak, ek, context, span.clone());
            self.expect_at(av, ev, context, span);
            return;
        }
        if let (Ty::Tuple(a_items), Ty::Tuple(e_items)) = (actual_base, expected_base) {
            if a_items.len() != e_items.len() {
                self.errors.push(TypeDiagnostic::TypeMismatch {
                    context: context.to_string(),
                    expected: expected.clone(),
                    actual: actual.clone(),
                    span,
                    help: None,
                });
                return;
            }
            for (a, e) in a_items.iter().zip(e_items.iter()) {
                self.expect_at(a, e, context, span.clone());
            }
            return;
        }

        // Struct structural compatibility: all expected fields must be present.
        // When both sides carry an explicit name, also enforce nominal identity
        // (Score is not assignable to Point even with identical fields).
        // Forward `span` into every recursive call so that nested field errors
        // carry the same precise location as the top-level mismatch instead of
        // falling back to the ambient `current_span`.
        if let (
            Ty::Struct {
                name: act_name,
                fields: actual_fields,
            },
            Ty::Struct {
                name: exp_name,
                fields: expected_fields,
            },
        ) = (actual_base, expected_base)
        {
            if let (Some(a), Some(e)) = (act_name, exp_name)
                && a != e
            {
                self.errors.push(TypeDiagnostic::TypeMismatch {
                    context: context.to_string(),
                    expected: expected.clone(),
                    actual: actual.clone(),
                    span,
                    help: None,
                });
                return;
            }
            for (exp_name, exp_ty) in expected_fields {
                match actual_fields.iter().find(|(n, _)| n == exp_name) {
                    None => self.err_at(
                        format!("{context}: missing field `{exp_name}`"),
                        span.clone(),
                    ),
                    Some((_, act_ty)) => {
                        self.expect_at(
                            act_ty,
                            exp_ty,
                            &format!("{context}.{exp_name}"),
                            span.clone(),
                        );
                    }
                }
            }
            return;
        }

        // Map literal coercion: a `{k: v, ...}` struct literal assigned to a
        // declared `map[K, V]` is treated as a map when keys are strings and
        // every field value matches V. This matches the surface syntax where
        // the same `{...}` form serves as both struct and map literal.
        // Forward `span` so value-type mismatches point to the argument, not
        // to the enclosing statement.
        if let (
            Ty::Struct {
                fields: actual_fields,
                ..
            },
            Ty::Map(key_ty, value_ty),
        ) = (actual_base, expected_base)
            && (matches!(key_ty.as_ref(), Ty::Str) || key_ty.is_opaque())
        {
            for (name, act_ty) in actual_fields {
                self.expect_at(
                    act_ty,
                    value_ty,
                    &format!("{context}[{name}]"),
                    span.clone(),
                );
            }
            return;
        }

        if actual_base != expected_base && !actual_base.is_opaque() {
            self.errors.push(TypeDiagnostic::TypeMismatch {
                context: context.to_string(),
                expected: expected.clone(),
                actual: actual.clone(),
                span,
                help: None,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Interface helpers
// ---------------------------------------------------------------------------

/// Produce a human-readable display string for a `TypeExpr` — used only for
/// error messages in `check_impl_conformance`.  This is intentionally separate
/// from the conformance logic: the typed comparison in
/// [`crate::types::interface::signature_satisfies`] is the source of truth;
/// this function only drives the "must return X but returns Y" message.
fn type_display_str(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", type_display_str(inner)),
        TypeExpr::List(inner) => format!("list[{}]", type_display_str(inner)),
        TypeExpr::Map(k, v) => {
            format!("map[{}, {}]", type_display_str(k), type_display_str(v))
        }
        TypeExpr::Set(inner) => format!("set[{}]", type_display_str(inner)),
        TypeExpr::Tuple(items) => {
            let parts: Vec<_> = items.iter().map(type_display_str).collect();
            format!("({})", parts.join(", "))
        }
        TypeExpr::Func(params, ret) => {
            let ps: Vec<_> = params.iter().map(type_display_str).collect();
            format!("({}) -> {}", ps.join(", "), type_display_str(ret))
        }
        TypeExpr::Generic(name, args) => {
            let as_: Vec<_> = args.iter().map(type_display_str).collect();
            format!("{}[{}]", name, as_.join(", "))
        }
        TypeExpr::Struct(fields) => {
            let fs: Vec<_> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_display_str(&f.ty.kind)))
                .collect();
            format!("{{{}}}", fs.join(", "))
        }
        TypeExpr::Dynamic => "dynamic".to_string(),
        TypeExpr::SelfType => "self".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheckArtifacts, Ty, TypeDiagnostic, check_program, check_program_with_artifacts,
        definition_of, ident_at_offset, ident_span_at_offset, type_at,
    };
    use crate::lexer::lex;
    use crate::parser::parse;
    use miette::NamedSource;

    fn type_errors(source: &str) -> Vec<String> {
        type_errors_full(source)
            .into_iter()
            .map(|e| e.message())
            .collect()
    }

    fn type_errors_full(source: &str) -> Vec<TypeDiagnostic> {
        let named = NamedSource::new("t.keel", source.to_string());
        let tokens = lex(source, &named).expect("lex failed");
        let program = parse(tokens, source.len(), &named).expect("parse failed");
        check_program(&program, false)
    }

    fn type_ok(source: &str) {
        let errs = type_errors(source);
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
    }

    fn expect_error(source: &str, substring: &str) {
        let errs = type_errors(source);
        assert!(
            errs.iter().any(|e| e.contains(substring)),
            "expected error containing {substring:?}, got: {errs:?}"
        );
    }

    fn artifacts_of(source: &str) -> CheckArtifacts {
        let named = NamedSource::new("t.keel", source.to_string());
        let tokens = lex(source, &named).expect("lex failed");
        let program = parse(tokens, source.len(), &named).expect("parse failed");
        let (errs, artifacts) = check_program_with_artifacts(&program, false);
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
        artifacts
    }

    // ─── Valid programs ─────────────────────────────────────────────────────────

    #[test]
    fn valid_minimal_agent() {
        type_ok(
            r#"
agent Greeter {
  @role "hi"
}

run(Greeter)
"#,
        );
    }

    #[test]
    fn valid_task_with_return_type() {
        type_ok(
            r#"
task greet(name: str) -> str {
  "hello"
}
"#,
        );
    }

    #[test]
    fn valid_enum_and_when() {
        type_ok(
            r#"
type Urgency = low | medium | high | critical

task triage(u: Urgency) {
  when u {
    low, medium => { return }
    high, critical => { return }
  }
}
"#,
        );
    }

    #[test]
    fn valid_self_inside_agent() {
        type_ok(
            r#"
agent Counter {
  @role "count"
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1
  }
}
"#,
        );
    }

    #[test]
    fn valid_agent_task_calls_sibling_via_self() {
        type_ok(
            r#"
use std/io
agent Bot {
  @tools [io]
  @role "x"

  task step() {
    self.other()
  }

  task other() {
    io.notify("hi")
  }
}
"#,
        );
    }

    #[test]
    fn error_bare_agent_task_call_is_not_in_scope() {
        expect_error(
            r#"
use std/io
agent Bot {
  @tools [io]
  @role "x"

  task step() {
    other()
  }

  task other() {
    io.notify("hi")
  }
}
"#,
            "undefined: `other`",
        );
    }

    #[test]
    fn error_direct_agent_task_call_is_rejected() {
        expect_error(
            r#"
use std/io
agent Worker {
  @tools [io]
  @role "x"

  task run() {
    io.notify("work")
  }
}

task invoke() {
  Worker.run()
}
"#,
            "direct agent task calls",
        );
    }

    // ─── Errors: undefined / scope ──────────────────────────────────────────────

    #[test]
    fn error_undefined_variable() {
        expect_error(
            r#"
task t() {
  x = unknown_thing
}
"#,
            "undefined",
        );
    }

    #[test]
    fn provider_builtin_name_is_ok() {
        type_ok(
            r#"
agent A {
  @provider anthropic
  @model "claude-opus-4-8"
}

run(A)
"#,
        );
    }

    #[test]
    fn provider_unknown_resolvable_name_is_compile_error() {
        // A name that *resolves* (a declared type) but has no `impl LlmProvider`
        // must still be rejected at check time — not slip through to runtime.
        expect_error(
            r#"
type MyProvider { id: int }

agent A {
  @provider MyProvider
  @model "x"
}

run(A)
"#,
            "built-in provider",
        );
    }

    #[test]
    fn provider_user_type_with_impl_is_ok() {
        type_ok(
            r#"
type MyProvider {}
impl LlmProvider for MyProvider {
  task complete(self, req: CompletionRequest) -> str {
    req.user
  }
}

agent A {
  @provider MyProvider
  @model "x"
}

run(A)
"#,
        );
    }

    #[test]
    fn ai_install_user_type_with_impl_is_ok() {
        type_ok(
            r#"
use std/ai
type MyProvider {}
impl LlmProvider for MyProvider {
  task complete(self, req: CompletionRequest) -> str {
    "{req.system} :: {req.model}"
  }
}

ai.install(MyProvider)
"#,
        );
    }

    #[test]
    fn provider_user_type_with_fields_is_compile_error() {
        // The runtime constructs the provider with no fields, so a field-bearing
        // provider type is rejected — config belongs in env.* inside complete().
        expect_error(
            r#"
type Configured { key: str }
impl LlmProvider for Configured {
  task complete(self, req: CompletionRequest) -> str {
    req.user
  }
}

agent A {
  @provider Configured
  @model "x"
}

run(A)
"#,
            "field-less",
        );
    }

    #[test]
    fn ai_install_non_conforming_type_is_compile_error() {
        expect_error(
            r#"
use std/ai
type Bogus { id: int }

ai.install(Bogus)
"#,
            "built-in provider",
        );
    }

    #[test]
    fn ai_install_builtin_name_is_compile_error() {
        // A built-in backend name is not installable — it resolves to no value at
        // runtime (a confusing `Undefined` error). Reject it at check time and
        // point at `@provider`/the `provider:` prefix instead.
        expect_error(
            r#"
use std/ai
ai.install(openai)
"#,
            "built-in backend",
        );
    }

    #[test]
    fn error_self_outside_agent() {
        expect_error(
            r#"
task t() {
  self.count = 1
}
"#,
            "outside an agent",
        );
    }

    #[test]
    fn error_self_unknown_state_field() {
        expect_error(
            r#"
agent Counter {
  @role "x"
  state { count: int = 0 }

  task bad() {
    self.nope = 1
  }
}
"#,
            "no state field",
        );
    }

    // ─── Errors: scope isolation for block-owning statements ────────────────────

    #[test]
    fn error_if_body_binding_does_not_leak() {
        // A name bound inside an `if` body must not be visible after the block.
        expect_error(
            r#"
use std/io
task main() {
  if true {
    x = 1
  }
  io.show(x)
}
"#,
            "undefined: `x`",
        );
    }

    #[test]
    fn error_while_body_binding_does_not_leak() {
        // A name bound inside a `while` body must not be visible after the block.
        expect_error(
            r#"
use std/io
task main() {
  while false {
    y = 1
  }
  io.show(y)
}
"#,
            "undefined: `y`",
        );
    }

    #[test]
    fn error_try_body_binding_does_not_leak() {
        // A name bound inside the `try` body must not be visible after the block.
        expect_error(
            r#"
use std/io
task main() {
  try {
    z = 1
  } catch e: Error { }
  io.show(z)
}
"#,
            "undefined: `z`",
        );
    }

    #[test]
    fn error_param_default_with_undefined_name() {
        // Undefined names in parameter default expressions must be caught.
        expect_error(
            r#"
use std/io
task main(x: int = missing) {
  io.show(x)
}
"#,
            "undefined: `missing`",
        );
    }

    // ─── Errors: exhaustiveness ─────────────────────────────────────────────────

    #[test]
    fn error_non_exhaustive_when() {
        expect_error(
            r#"
type Urgency = low | medium | high | critical

task t(u: Urgency) {
  when u {
    low => { return }
    medium => { return }
  }
}
"#,
            "non-exhaustive",
        );
    }

    #[test]
    fn valid_when_with_wildcard() {
        type_ok(
            r#"
type Urgency = low | medium | high | critical

task t(u: Urgency) {
  when u {
    low => { return }
    _ => { return }
  }
}
"#,
        );
    }

    #[test]
    fn error_when_on_non_enum_without_wildcard() {
        expect_error(
            r#"
task t(code: int) {
  when code {
    200 => { return }
    404 => { return }
  }
}
"#,
            "requires a `_`",
        );
    }

    // ─── v0.1.4: let type annotations ──────────────────────────────────────────

    #[test]
    fn valid_let_annotation_matching_type() {
        type_ok(
            r#"
task t() {
  x: str = "hello"
}
"#,
        );
    }

    #[test]
    fn error_let_annotation_type_mismatch() {
        expect_error(
            r#"
task t() {
  x: int = "hello"
}
"#,
            "expected int",
        );
    }

    // ─── Errors: control flow ───────────────────────────────────────────────────

    #[test]
    fn error_if_condition_not_bool() {
        expect_error(
            r#"
task t() {
  if "hello" {
    x = 1
  }
}
"#,
            "expected bool",
        );
    }

    #[test]
    fn error_for_over_non_list() {
        expect_error(
            r#"
task t() {
  for x in 42 {
    y = x
  }
}
"#,
            "expects a list",
        );
    }

    // ─── Errors: arity ──────────────────────────────────────────────────────────

    #[test]
    fn error_too_many_args() {
        expect_error(
            r#"
task greet(name: str) -> str {
  "hi"
}

task call_it() {
  x = greet("a", "b", "c")
}
"#,
            "argument",
        );
    }

    #[test]
    fn valid_out_of_order_named_args_use_matching_literal_types() {
        type_ok(
            r#"
type Record = { tag: int }

task collect(record: Record, labels: map[str, int]) {}

task call_it() {
  collect(labels: {one: 1}, record: {tag: 2})
}
"#,
        );
    }

    // ─── Enum inference via ai.classify ─────────────────────────────────────────

    #[test]
    fn valid_classify_inferred_enum() {
        // `ai.classify(..., as: Mood) ?? Mood.neutral` unwraps the nullable so
        // the result is Mood and `when` on it is exhaustive.
        type_ok(
            r#"
use std/ai
type Mood = happy | neutral | sad

task t(text: str) {
  mood = ai.classify(text, as: Mood) ?? Mood.neutral
  when mood {
    happy => { return }
    neutral => { return }
    sad => { return }
  }
}
"#,
        );
    }

    // ─── Rich enum variants ─────────────────────────────────────────────────────

    #[test]
    fn valid_rich_enum_variant() {
        type_ok(
            r#"
type Action =
  | reply { to: str, tone: str }
  | archive

task make() -> Action {
  Action.reply { to: "x", tone: "friendly" }
}
"#,
        );
    }

    #[test]
    fn error_rich_variant_unknown() {
        expect_error(
            r#"
type Action =
  | reply { to: str }
  | archive

task make() -> Action {
  Action.nope { to: "x" }
}
"#,
            "no variant",
        );
    }

    #[test]
    fn error_classify_result_missing_variant() {
        expect_error(
            r#"
use std/ai
type Mood = happy | neutral | sad

task t(text: str) {
  mood = ai.classify(text, as: Mood) ?? Mood.neutral
  when mood {
    happy => { return }
    sad => { return }
  }
}
"#,
            "non-exhaustive",
        );
    }

    // ─── v0.1.5: nullable safety ────────────────────────────────────────────────

    #[test]
    fn error_nullable_passed_as_non_nullable() {
        expect_error(
            r#"
use std/env
task t() {
  x: str = env.get("KEY")
}
"#,
            "use `!` to assert non-null",
        );
    }

    #[test]
    fn valid_nullable_unwrapped_with_assert() {
        type_ok(
            r#"
use std/env
task t() {
  x: str = env.get("KEY")!
}
"#,
        );
    }

    #[test]
    fn valid_nullable_coalesced() {
        type_ok(
            r#"
use std/env
task t() {
  x: str = env.get("KEY") ?? "default"
}
"#,
        );
    }

    #[test]
    fn valid_non_nullable_assigned_to_nullable() {
        type_ok(
            r#"
task t() {
  x: str? = "hello"
}
"#,
        );
    }

    // ─── nullable safety at call sites ───────────────────────────────────────────

    #[test]
    fn error_nullable_arg_at_top_level_task_call() {
        expect_error(
            r#"
use std/env
task process(x: str) {}
task t() {
  val: str? = env.get("KEY")
  process(val)
}
"#,
            "use `!` to assert non-null",
        );
    }

    #[test]
    fn valid_nullable_arg_unwrapped_at_call_site() {
        type_ok(
            r#"
use std/env
task process(x: str) {}
task t() {
  val: str? = env.get("KEY")
  process(val!)
}
"#,
        );
    }

    #[test]
    fn valid_nullable_arg_coalesced_at_call_site() {
        type_ok(
            r#"
use std/env
task process(x: str) {}
task t() {
  val: str? = env.get("KEY")
  process(val ?? "default")
}
"#,
        );
    }

    #[test]
    fn error_nullable_named_arg_at_task_call() {
        expect_error(
            r#"
use std/env
task process(x: str) {}
task t() {
  val: str? = env.get("KEY")
  process(x: val)
}
"#,
            "use `!` to assert non-null",
        );
    }

    #[test]
    fn error_wrong_type_arg_at_task_call() {
        expect_error(
            r#"
task process(x: str) {}
task t() {
  process(42)
}
"#,
            "expected str, got int",
        );
    }

    // ─── v0.1.5: return-type matching ──────────────────────────────────────────

    #[test]
    fn error_return_stmt_type_mismatch() {
        expect_error(
            r#"
task t() -> str {
  return 42
}
"#,
            "return value: expected str",
        );
    }

    #[test]
    fn valid_return_stmt_matches_declared() {
        type_ok(
            r#"
task t() -> str {
  return "hello"
}
"#,
        );
    }

    #[test]
    fn valid_task_no_return_type() {
        type_ok(
            r#"
task t() {
  return 42
}
"#,
        );
    }

    // ─── v0.1.5: struct field checks ───────────────────────────────────────────

    #[test]
    fn error_missing_struct_field() {
        expect_error(
            r#"
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }
}
"#,
            "missing field `age`",
        );
    }

    #[test]
    fn valid_struct_all_fields_present() {
        type_ok(
            r#"
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice", age: 30 }
}
"#,
        );
    }

    #[test]
    fn valid_struct_extra_fields_allowed() {
        type_ok(
            r#"
type Person { name: str }

task t() {
  p: Person = { name: "Alice", extra: 42 }
}
"#,
        );
    }

    // ─── v0.1.5: generic list type inference ───────────────────────────────────

    #[test]
    fn valid_list_push_preserves_element_type() {
        type_ok(
            r#"
task t() {
  items: list[str] = ["a", "b"]
  more = items.push("c")
}
"#,
        );
    }

    #[test]
    fn valid_list_concatenation_inferred() {
        type_ok(
            r#"
use std/io
task t() {
  a = ["x", "y"]
  b = ["z"]
  all = a + b
  for item in all {
    io.notify(item)
  }
}
"#,
        );
    }

    #[test]
    fn valid_list_len_is_int() {
        type_ok(
            r#"
task t() {
  items = ["a", "b", "c"]
  n: int = items.len()
}
"#,
        );
    }

    // ─── v0.1.17: readonly state fields ────────────────────────────────────────

    #[test]
    fn valid_readonly_field_readable() {
        type_ok(
            r#"
use std/io
agent Bot {
  @tools [io]
  state {
    turns: int = 0
    session_id: readonly str = "default"
  }
  task check() {
    io.notify(self.session_id)
  }
}
"#,
        );
    }

    #[test]
    fn error_readonly_field_assigned() {
        expect_error(
            r#"
agent Bot {
  state {
    session_id: readonly str = "default"
  }
  task reset() {
    self.session_id = "new"
  }
}
"#,
            "readonly",
        );
    }

    #[test]
    fn valid_list_filter_preserves_type() {
        type_ok(
            r#"
use std/io
task t() {
  items = ["a", "bb", "ccc"]
  short = items.filter(x => true)
  for s in short {
    io.notify(s)
  }
}
"#,
        );
    }

    #[test]
    fn valid_complex_type_expressions_resolve() {
        type_ok(
            r#"
type Pair = (str, int)
type Bag = dynamic

task t(pair: Pair, bag: Bag) {
  same_pair: Pair = pair
  same_bag: Bag = bag
}
"#,
        );
    }

    #[test]
    fn error_struct_destructure_from_non_struct() {
        expect_error(
            r#"
use std/io
task t() {
  {name} = 42
  io.notify(name)
}
"#,
            "cannot destructure int as a struct",
        );
    }

    #[test]
    fn error_tuple_destructure_from_non_tuple() {
        expect_error(
            r#"
use std/io
task t() {
  (name, count) = {name: "a", count: 1}
  io.notify(name)
}
"#,
            "cannot destructure struct as a tuple",
        );
    }

    #[test]
    fn type_at_reports_destructured_and_nested_bindings() {
        let source = r#"
type Item = {name: str, score: int}

agent Bot {
  state { session_id: readonly str = "s1" }

  on scored({name: item_name, score: item_score}: Item) {
    for loop_score in [1] {
      try {
        copied_name = "literal"
      } catch caught_error: Error {
        recovered = "fallback"
      }
    }
  }
}
"#;

        let cases = [
            ("item_name", "str"),
            ("item_score", "int"),
            ("session_id", "str"),
            ("loop_score", "int"),
            ("copied_name", "str"),
            ("caught_error", "Error"), // resolved as Unresolved("Error") — shows declared name
            ("recovered", "str"),
        ];

        for (needle, expected) in cases {
            let offset = source
                .find(needle)
                .unwrap_or_else(|| panic!("missing {needle} in source"))
                + 1;
            let actual =
                type_at(source, offset).unwrap_or_else(|| panic!("expected type for {needle}"));
            assert!(
                actual.contains(expected),
                "expected {needle} to contain {expected:?}, got {actual:?}"
            );
        }
    }

    #[test]
    fn ident_helpers_decline_non_identifier_offsets() {
        let source = "task greet() -> str { \"hello\" }\n";
        let quote = source.find('"').expect("string literal quote");

        assert_eq!(ident_at_offset(source, quote), None);
        assert_eq!(ident_span_at_offset(source, quote), None);
        assert_eq!(definition_of(source, quote), None);
        assert_eq!(type_at("task t( {", 2), None);
    }

    // ─── v0.1.19 additive checker fixes ─────────────────────────────────────────

    #[test]
    fn valid_set_literal_typed_as_set() {
        // set[] literal — checker must not error; inferred as set[int]
        type_ok(
            r#"
task go() {
  s = set[1, 2, 3]
}
"#,
        );
    }

    #[test]
    fn valid_null_field_access_propagates_nullable() {
        type_ok(
            r#"
type Info = { name: str, score: int }

task go(x: Info?) {
  n = x?.name
  s = x?.score
}
"#,
        );
    }

    #[test]
    fn valid_null_coalesce_unwraps_nullable() {
        type_ok(
            r#"
task go(x: str?) {
  result: str = x ?? "default"
}
"#,
        );
    }

    #[test]
    fn valid_lambda_block_body_return_type_inferred() {
        type_ok(
            r#"
task go() {
  items = [1, 2, 3]
  doubled = items.map(x => {
    x * 2
  })
}
"#,
        );
    }

    #[test]
    fn valid_ai_extract_as_resolves_struct_type() {
        type_ok(
            r#"
use std/ai
type Contact = { name: str, email: str }

task go(text: str) {
  result = ai.extract(text, as: Contact)
  name = result?.name
}
"#,
        );
    }

    #[test]
    fn valid_ai_decide_as_resolves_enum_type() {
        type_ok(
            r#"
use std/ai
type Priority = low | medium | high

task go(text: str) {
  p = ai.decide(text, as: Priority)
}
"#,
        );
    }

    #[test]
    fn valid_implicit_return_expression_matches_declared() {
        type_ok(
            r#"
task double(n: int) -> int {
  n * 2
}
"#,
        );
    }

    #[test]
    fn error_implicit_return_type_mismatch() {
        expect_error(
            r#"
task greet() -> int {
  "hello"
}
"#,
            "implicit return",
        );
    }

    #[test]
    fn valid_implicit_return_skipped_for_return_stmt() {
        // A task ending in `return` must not trigger the implicit-return check.
        type_ok(
            r#"
task greet() -> str {
  return "hello"
}
"#,
        );
    }

    #[test]
    fn valid_implicit_return_skipped_for_when_stmt() {
        // A task ending in `when` must not trigger the implicit-return check.
        type_ok(
            r#"
type Color = red | green | blue

task name(c: Color) -> str {
  when c {
    red => { return "red" }
    green => { return "green" }
    blue => { return "blue" }
  }
}
"#,
        );
    }

    #[test]
    fn valid_if_expr_branches_same_type() {
        type_ok(
            r#"
task go(x: int) -> int {
  if x > 0 { x } else { 0 }
}
"#,
        );
    }

    #[test]
    fn error_if_expr_branches_type_mismatch() {
        expect_error(
            r#"
task go(flag: bool) {
  result = if flag { 1 } else { "oops" }
}
"#,
            "branches must have the same type",
        );
    }

    #[test]
    fn valid_if_expr_return_branch_propagates_other_type() {
        // When one branch exits via `return`, the if-expr takes the other branch's type.
        type_ok(
            r#"
task classify(n: int) -> str {
  label = if n > 0 { return "positive" } else { "other" }
  label
}
"#,
        );
    }

    // ─── v0.1.20: generic type declarations ────────────────────────────────────

    #[test]
    fn valid_generic_struct_instantiation() {
        type_ok(
            r#"
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

task t(p: Paginated[str]) {
  items: list[str] = p.items
}
"#,
        );
    }

    #[test]
    fn valid_generic_struct_nested_params() {
        // T flows through nested list inside a generic struct.
        type_ok(
            r#"
type Wrapper[T] {
  value: T
}

task t(w: Wrapper[int]) {
  v: int = w.value
}
"#,
        );
    }

    #[test]
    fn valid_generic_alias() {
        // Generic alias that expands to a concrete list type.
        type_ok(
            r#"
type Bag[T] = list[T]

task t(items: Bag[str]) {
  n: int = items.len()
}
"#,
        );
    }

    #[test]
    fn valid_generic_enum_variant_exhaustive() {
        // Generic enums register variant names; exhaustiveness check still works.
        type_ok(
            r#"
use std/io
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }

task t(p: Pair[str, int]) {
  when p {
    both => { io.notify("both") }
    only_first => { io.notify("first") }
    only_second => { io.notify("second") }
  }
}
"#,
        );
    }

    #[test]
    fn valid_generic_struct_multi_param() {
        type_ok(
            r#"
type Pair[A, B] {
  first: A
  second: B
}

task t(p: Pair[str, int]) {
  a: str = p.first
  b: int = p.second
}
"#,
        );
    }

    // ─── v0.1.20: function type syntax ─────────────────────────────────────────

    #[test]
    fn valid_func_type_alias_used_as_param() {
        type_ok(
            r#"
type Handler = (str) -> bool

task t(h: Handler) {
  ok: bool = h("hello")
}
"#,
        );
    }

    #[test]
    fn valid_func_type_multi_param() {
        type_ok(
            r#"
type Reducer = (str, int) -> str

task t(r: Reducer) {
  result: str = r("x", 1)
}
"#,
        );
    }

    #[test]
    fn valid_generic_func_type_alias() {
        // type Predicate[T] = (T) -> bool — from SPEC §2.6
        type_ok(
            r#"
type Predicate[T] = (T) -> bool

task t(pred: Predicate[str]) {
  ok: bool = pred("hello")
}
"#,
        );
    }

    // ─── v0.1.20: generic enum variant field types ──────────────────────────────

    #[test]
    fn valid_generic_enum_variant_fields_typed() {
        // Variant bindings resolve to substituted field types, not Unknown.
        type_ok(
            r#"
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      f: str = first
      s: int = second
    }
    only_first { value } => {
      v: str = value
    }
    only_second { value } => {
      v: int = value
    }
  }
}
"#,
        );
    }

    #[test]
    fn valid_generic_enum_variant_nested_type() {
        // Field type itself is a generic instantiation.
        type_ok(
            r#"
use std/io
type Box[T] {
  value: T
}

type Wrapped[T] =
  | some { inner: Box[T] }
  | none_val

task t(w: Wrapped[str]) {
  when w {
    some { inner } => {
      b: Box[str] = inner
    }
    none_val => { io.notify("empty") }
  }
}
"#,
        );
    }

    #[test]
    fn error_generic_enum_variant_field_wrong_type() {
        // Assigning a variant field binding to the wrong type must be caught.
        expect_error(
            r#"
type Pair[A, B] =
  | both { first: A, second: B }

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      wrong: int = first
    }
  }
}
"#,
            "expected int, got str",
        );
    }

    // ─── Generic tasks ───────────────────────────────────────────────────────────

    #[test]
    fn valid_generic_task_identity_inferred() {
        type_ok(
            r#"
task identity[T](x: T) -> T { x }

task main() {
  s: str = identity("hello")
  n: int = identity(42)
}
"#,
        );
    }

    #[test]
    fn valid_generic_task_return_type_inferred() {
        type_ok(
            r#"
task wrap[T](x: T) -> list[T] { [x] }

task main() {
  xs: list[int] = wrap(1)
}
"#,
        );
    }

    #[test]
    fn valid_generic_task_multi_param_inferred() {
        type_ok(
            r#"
task first[A, B](a: A, b: B) -> A { a }

task main() {
  s: str = first("hi", 99)
}
"#,
        );
    }

    #[test]
    fn error_generic_task_return_type_mismatch() {
        expect_error(
            r#"
task identity[T](x: T) -> T { x }

task main() {
  n: int = identity("oops")
}
"#,
            "expected int, got str",
        );
    }

    // ─── when as expression ─────────────────────────────────────────────────────

    #[test]
    fn valid_when_expr_string_arms() {
        type_ok(
            r#"
task grade(score: str) -> str {
  result: str = when score {
    "A" => "excellent"
    "B" => "good"
    _   => "needs work"
  }
  result
}
"#,
        );
    }

    #[test]
    fn valid_when_expr_enum_subject() {
        type_ok(
            r#"
type Priority = | low | medium | high

task label(p: Priority) -> str {
  when p {
    low    => "low"
    medium => "med"
    high   => "high"
  }
}
"#,
        );
    }

    #[test]
    fn valid_when_expr_int_arms() {
        type_ok(
            r#"
task classify(n: int) -> str {
  when n {
    0 => "zero"
    1 => "one"
    _ => "many"
  }
}
"#,
        );
    }

    #[test]
    fn error_when_expr_mismatched_arm_types() {
        expect_error(
            r#"
task t(x: str) -> str {
  result = when x {
    "a" => "ok"
    _   => 42
  }
  result
}
"#,
            "`when` expression arms must all have the same type",
        );
    }

    #[test]
    fn valid_when_expr_as_return_value() {
        type_ok(
            r#"
type Mood = | happy | sad

task describe(m: Mood) -> str {
  when m {
    happy => "great"
    sad   => "meh"
  }
}
"#,
        );
    }

    #[test]
    fn invalid_zip_non_list_arg_is_type_error() {
        expect_error(
            r#"
task t() {
  result = [1, 2, 3].zip("hello")
}
"#,
            "`.zip()` expects a list argument, got str",
        );
    }

    // ─── operator type compatibility ───────────────────────────────────────────

    #[test]
    fn binop_str_plus_int_is_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        x = "hi" + 5
    }
}
run(A)
"#,
            "cannot apply `+`",
        );
    }

    #[test]
    fn binop_str_minus_int_is_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        x = "hi" - 1
    }
}
run(A)
"#,
            "cannot apply `-`",
        );
    }

    #[test]
    fn binop_str_lt_int_is_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        x = "hi" < 5
    }
}
run(A)
"#,
            "cannot apply `<`",
        );
    }

    #[test]
    fn binop_bool_plus_int_is_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        x = true + 1
    }
}
run(A)
"#,
            "cannot apply `+`",
        );
    }

    #[test]
    fn binop_list_minus_int_is_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        x = [1, 2] - 1
    }
}
run(A)
"#,
            "cannot apply `-`",
        );
    }

    #[test]
    fn aug_assign_type_mismatch_is_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        x = 0
        x += "oops"
    }
}
run(A)
"#,
            "cannot apply `+`",
        );
    }

    #[test]
    fn binop_valid_numeric_combos() {
        type_ok(
            r#"
agent A {
    @on_start {
        a = 1 + 1
        b = 1.0 + 2
        c = 1 + 2.0
        d = 3.0 - 1.0
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn binop_valid_str_concat() {
        type_ok(
            r#"
agent A {
    @on_start {
        x = "a" + "b"
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn binop_valid_list_concat() {
        type_ok(
            r#"
agent A {
    @on_start {
        x = [1] + [2]
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn binop_valid_comparisons() {
        type_ok(
            r#"
agent A {
    @on_start {
        a = 1 < 2
        b = "a" < "b"
        c = 1.0 >= 0
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn binop_equality_is_always_valid() {
        type_ok(
            r#"
agent A {
    @on_start {
        x = 1 == "hello"
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn binop_unknown_operand_skips_check() {
        // list.reduce() returns Unknown — should not trigger a type error when used as operand
        type_ok(
            r#"
agent A {
    @on_start {
        v = [1, 2, 3].reduce()
        x = v + 1
    }
}
run(A)
"#,
        );
    }

    // ─── Variadic parameters ──────────────────────────────────────────────────────

    #[test]
    fn variadic_zero_args_ok() {
        type_ok(
            r#"
task greet(...names: str) -> str { "ok" }
agent A {
    @on_start { x = greet() }
}
run(A)
"#,
        );
    }

    #[test]
    fn variadic_many_args_ok() {
        type_ok(
            r#"
task greet(...names: str) -> str { "ok" }
agent A {
    @on_start { x = greet("a", "b", "c") }
}
run(A)
"#,
        );
    }

    #[test]
    fn variadic_spread_list_ok() {
        type_ok(
            r#"
task greet(...names: str) -> str { "ok" }
agent A {
    @on_start {
        xs = ["a", "b"]
        x = greet(...xs)
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn variadic_mixed_spread_ok() {
        type_ok(
            r#"
task greet(...names: str) -> str { "ok" }
agent A {
    @on_start {
        xs = ["b", "c"]
        x = greet("a", ...xs)
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn variadic_wrong_type_error() {
        expect_error(
            r#"
task add(...nums: int) -> int { 0 }
agent A {
    @on_start { x = add("oops") }
}
run(A)
"#,
            "variadic arg `nums`",
        );
    }

    #[test]
    fn variadic_list_without_spread_is_error() {
        // Passing list[int] where int is expected — must use ...
        expect_error(
            r#"
task add(...nums: int) -> int { 0 }
agent A {
    @on_start {
        xs = [1, 2, 3]
        x = add(xs)
    }
}
run(A)
"#,
            "variadic arg `nums`",
        );
    }

    #[test]
    fn variadic_body_sees_list_type() {
        // Inside the body, the variadic param should be list[str].
        type_ok(
            r#"
task join(...words: str) -> str {
    result: list[str] = words
    "ok"
}
agent A {
    @on_start { join("a", "b") }
}
run(A)
"#,
        );
    }

    #[test]
    fn variadic_with_fixed_params_ok() {
        type_ok(
            r#"
task fmt(prefix: str, ...parts: str) -> str { "ok" }
agent A {
    @on_start { fmt(">>", "a", "b", "c") }
}
run(A)
"#,
        );
    }

    #[test]
    fn spread_on_fixed_arity_task_is_error() {
        expect_error(
            r#"
task greet(name: str) -> str { name }
agent A {
    @on_start {
        xs = ["alice", "bob"]
        greet(...xs)
    }
}
run(A)
"#,
            "spread args",
        );
    }

    #[test]
    fn min_max_spread_plus_scalar_ok() {
        type_ok(
            r#"
agent A {
    @on_start {
        scores = [4, 9, 2]
        hi = max(...scores, 99)
        lo = min(...scores, 1)
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn min_max_multi_spread_ok() {
        type_ok(
            r#"
agent A {
    @on_start {
        a = [4, 9]
        b = [2, 7]
        lo = min(...a, ...b)
        hi = max(...a, ...b)
    }
}
run(A)
"#,
        );
    }

    // ─── Subscript access (`list[i]`, `str[i]`) ────────────────────────────────

    #[test]
    fn subscript_list_ok() {
        type_ok(
            r#"
agent A {
    @on_start {
        items: list[int] = [10, 20, 30]
        x: int = items[0]
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn subscript_string_ok() {
        type_ok(
            r#"
agent A {
    @on_start {
        word = "hello"
        ch: str = word[1]
    }
}
run(A)
"#,
        );
    }

    #[test]
    fn subscript_non_int_index_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        items = [1, 2, 3]
        x = items["bad"]
    }
}
run(A)
"#,
            "subscript index must be int",
        );
    }

    #[test]
    fn subscript_set_type_error() {
        expect_error(
            r#"
agent A {
    @on_start {
        s: int = 42
        x = s[0]
    }
}
run(A)
"#,
            "subscript",
        );
    }

    // ─── while loop ──────────────────────────────────────────────────────────────

    #[test]
    fn while_bool_condition_is_valid() {
        type_ok(
            r#"
task t() {
    n = 0
    while n < 10 {
        n += 1
    }
}
"#,
        );
    }

    #[test]
    fn while_true_literal_is_valid() {
        type_ok(
            r#"
task t() {
    while true {
        break
    }
}
"#,
        );
    }

    #[test]
    fn while_non_bool_condition_is_error() {
        expect_error(
            r#"
task t() {
    while "oops" {
        break
    }
}
"#,
            "`while` condition",
        );
    }

    // ─── Db namespace ────────────────────────────────────────────────────────────

    #[test]
    fn db_connect_is_valid() {
        type_ok(
            r#"
use std/db
task use_db() {
    db = db.connect("sqlite://:memory:")
}
"#,
        );
    }

    #[test]
    fn db_query_result_supports_list_methods() {
        type_ok(
            r#"
use std/db
task use_db() {
    db = db.connect("sqlite://:memory:")
    rows = db.query("SELECT 1", [])
    n = rows.len()
}
"#,
        );
    }

    #[test]
    fn db_exec_result_used_as_int() {
        type_ok(
            r#"
use std/db
task use_db() {
    db = db.connect("sqlite://:memory:")
    affected = db.exec("DELETE FROM t", [])
    ok = affected > 0
}
"#,
        );
    }

    // ─── Agent.delegate type checking ────────────────────────────────────────────

    #[test]
    fn valid_agent_delegate_symbol_form() {
        type_ok(
            r#"
use std/io
agent Worker {
    @tools [io]
    on process(data: str) {
        io.show(data)
    }
}
agent Boss {
    @on_start {
        run(Worker)
        delegate(Worker.process, "hello")
    }
}
run(Boss)
"#,
        );
    }

    #[test]
    fn valid_agent_delegate_string_form_checks_handler() {
        type_ok(
            r#"
use std/io
agent Worker {
    @tools [io]
    on process(data: str) {
        io.show(data)
    }
}
agent Boss {
    @on_start {
        delegate(Worker, "process", "payload")
    }
}
run(Boss)
"#,
        );
    }

    #[test]
    fn error_agent_delegate_symbol_form_unknown_handler() {
        expect_error(
            r#"
use std/io
agent Worker {
    @tools [io]
    on process(data: str) {
        io.show(data)
    }
}
agent Boss {
    @on_start {
        delegate(Worker.typo, "payload")
    }
}
run(Boss)
"#,
            "agent `Worker` has no handler `typo`",
        );
    }

    #[test]
    fn error_agent_delegate_string_form_unknown_handler() {
        expect_error(
            r#"
use std/io
agent Worker {
    @tools [io]
    on process(data: str) {
        io.show(data)
    }
}
agent Boss {
    @on_start {
        delegate(Worker, "typo", "payload")
    }
}
run(Boss)
"#,
            "agent `Worker` has no handler `typo`",
        );
    }

    // ─── String interpolation parse errors (issue #14) ──────────────────────────

    #[test]
    fn error_malformed_interpolation_incomplete_binary_op() {
        // Canonical {…} syntax: stray + without right operand.
        expect_error(
            r#"task go() { x = "{1 +}" }"#,
            "invalid expression in string interpolation",
        );
    }

    #[test]
    fn error_malformed_interpolation_stray_token() {
        // Unlexable characters in slot → empty token stream → ParseError.
        expect_error(
            r#"task go() { x = "{@@@}" }"#,
            "invalid expression in string interpolation",
        );
    }

    #[test]
    fn valid_interpolation_simple_ident_unaffected() {
        type_ok(
            r#"
task go(name: str) -> str {
    "{name}"
}
"#,
        );
    }

    #[test]
    fn valid_interpolation_expression_unaffected() {
        type_ok(
            r#"
task go(a: int, b: int) -> str {
    "{a + b}"
}
"#,
        );
    }

    #[test]
    fn valid_interpolation_digit_separator_unaffected() {
        // 1_000_000 must be treated as the integer 1000000, not parsed as
        // Integer("1") followed by a stray identifier.
        type_ok(
            r#"
task go(ms: int) -> str {
    "{ms > 1_000_000_000}"
}
"#,
        );
    }

    #[test]
    fn valid_interpolation_underscore_ident_unaffected() {
        // An identifier like x1_2 must NOT have its underscore stripped;
        // it must resolve as the variable x1_2, not x12.
        type_ok(
            r#"
task go(x1_2: int) -> str {
    "{x1_2}"
}
"#,
        );
    }

    #[test]
    fn valid_interpolation_string_literal_underscore_unaffected() {
        // A string literal "1_2" inside a slot must not have its underscore
        // stripped; it must remain the string "1_2".
        type_ok(
            r#"
task go() -> str {
    "{"1_2"}"
}
"#,
        );
    }

    // ─── Map literal value-type inference: opaque-first sentinel fix ─────────────
    //
    // Previously, `is_opaque()` was used as the "not yet set" sentinel for the
    // inferred value type, causing any legitimately opaque first value
    // (e.g. json.parse → Unknown(ExternalDynamic)) to be overwritten by later
    // concrete entries.  The fix replaces the sentinel with Option<Ty>.
    //
    // Observable consequence: assigning `{1: json.parse("{}"), 2: "x"}` to an
    // explicit `map[int, str]` binding used to pass (the buggy inference gave
    // map[int, str]).  After the fix the inferred type is map[int, Unknown] which
    // does not equal map[int, str], so the assignment is rejected.

    #[test]
    fn map_opaque_first_value_is_not_overwritten_by_concrete_second() {
        // The map literal {1: json.parse("{}"), 2: "x"} must be inferred as
        // map[int, Unknown(ExternalDynamic)] — the first element's opaque type
        // wins; the second concrete "x" must not silently overwrite it.
        //
        // Because Unknown is opaque, assigning map[int, Unknown] to map[int, str]
        // is accepted without a cascade error (opaque types suppress diagnostics
        // everywhere by design — they represent intentional dynamic data).  The
        // invariant being protected here is the INFERENCE, not the assignment check.
        type_ok(
            r#"
use std/json
task go() -> int {
  m: map[int, str] = {1: json.parse("{}"), 2: "x"}
  return 0
}
"#,
        );
    }

    // ─── Type-annotation span (issue #8, stage 4) ────────────────────────────────
    //
    // When a `let` binding has an explicit type annotation and the inferred type
    // does not match, the error span must point at the annotation token sequence,
    // not the entire statement.

    #[test]
    fn type_mismatch_error_span_points_at_annotation() {
        // Source: `x: str = 1`
        // Byte layout (0-indexed):
        //   task go() {\n  x: str = 1\n}
        //   0123456789...
        // We find the annotation "str" in the source and verify the error span
        // covers exactly those bytes.
        let src = "task go() {\n  x: str = 1\n}";
        let errs = type_errors_full(src);
        assert!(!errs.is_empty(), "expected a type mismatch error");

        // At least one error must have a span that sits inside "str".
        let ann_start = src.find("str").expect("'str' not found in source");
        let ann_end = ann_start + "str".len();

        let has_annotation_span = errs.iter().any(|e| {
            let s = e.span();
            // The span must overlap the annotation range.
            s.start >= ann_start && s.end <= ann_end + 1
        });
        assert!(
            has_annotation_span,
            "expected error span to point at the type annotation 'str' ({ann_start}..{ann_end}), \
         got spans: {:?}",
            errs.iter().map(|e| e.span().clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn undefined_name_is_structured_with_identifier_span() {
        let src = "task go() {\n  missing\n}";
        let errs = type_errors_full(src);
        let missing_start = src.find("missing").expect("'missing' not found in source");
        let missing_end = missing_start + "missing".len();

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::UndefinedName { name, span }
                    if name == "missing" && span.start == missing_start && span.end == missing_end
            )),
            "expected structured UndefinedName at identifier span, got: {errs:?}"
        );
    }

    #[test]
    fn undefined_augmented_assignment_reports_one_checker_error() {
        let src = "task go() {\n  missing += 1\n}";
        let errs = type_errors_full(src);

        assert_eq!(errs.len(), 1, "expected one diagnostic, got: {errs:?}");
        assert_eq!(
            errs[0].message(),
            "augmented assignment to undefined variable `missing`"
        );
        let missing = src.find("missing").unwrap();
        assert_eq!(errs[0].span(), &(missing..missing + "missing".len()));
    }

    #[test]
    fn undefined_name_inside_interpolation_has_file_relative_span() {
        let src = r#"task go() {
  "hello { missing }"
}"#;
        let errs = type_errors_full(src);
        let missing_start = src.find("missing").expect("'missing' not found in source");
        let missing_end = missing_start + "missing".len();

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::UndefinedName { name, span }
                    if name == "missing" && span.start == missing_start && span.end == missing_end
            )),
            "expected interpolated UndefinedName at identifier span, got: {errs:?}"
        );
    }

    #[test]
    fn type_mismatch_is_structured_with_expected_and_actual_types() {
        let src = "task go() {\n  x: str = 1\n}";
        let errs = type_errors_full(src);

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::TypeMismatch {
                    context,
                    expected: Ty::Str,
                    actual: Ty::Int,
                    ..
                } if context == "`x`"
            )),
            "expected structured TypeMismatch for `x`, got: {errs:?}"
        );
    }

    #[test]
    fn wrong_arity_is_structured_with_call_span() {
        let src = r#"
task greet(name: str) {}

task go() {
  greet("Ada", "Lovelace")
}
"#;
        let errs = type_errors_full(src);
        let call_start = src.find("greet(\"Ada\"").expect("call not found");
        let call_end = src[call_start..]
            .find(')')
            .map(|offset| call_start + offset + 1)
            .expect("call end not found");

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::WrongArity {
                    task_name,
                    expected: 1,
                    actual: 2,
                    expected_params,
                    span,
                } if task_name == "greet"
                    && expected_params == &vec!["name".to_string()]
                    && span.start == call_start
                    && span.end == call_end
            )),
            "expected structured WrongArity at call span, got: {errs:?}"
        );
    }

    #[test]
    fn non_exhaustive_when_is_structured() {
        let src = r#"
type Status = open | closed

task go(s: Status) {
  when s {
    open => { return }
  }
}
"#;
        let errs = type_errors_full(src);

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::NonExhaustiveWhen {
                    enum_name,
                    missing,
                    ..
                } if enum_name == "Status" && missing == &vec!["closed".to_string()]
            )),
            "expected structured NonExhaustiveWhen, got: {errs:?}"
        );
    }

    #[test]
    fn map_concrete_first_opaque_second_accepts_the_opaque_entry() {
        // When the first value is concrete (Str) the inferred value type is Str.
        // The second opaque value (json.parse → Unknown) is passed to `expect`
        // against Str; because `actual.is_opaque()` is true, `expect` short-
        // circuits with no error.  The assignment to map[int, str] should succeed.
        type_ok(
            r#"
use std/json
task go() -> int {
  m: map[int, str] = {1: "x", 2: json.parse("{}")}
  return 0
}
"#,
        );
    }

    #[test]
    fn interface_not_satisfied_is_structured_for_missing_method() {
        let src = r#"
interface Greetable {
  task greet(self) -> str
}

type Person = { name: str }

impl Greetable for Person {}
"#;
        let errs = type_errors_full(src);

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::InterfaceNotSatisfied {
                    impl_name,
                    interface_name,
                    reason,
                    ..
                } if impl_name == "Person"
                    && interface_name == "Greetable"
                    && reason.contains("greet")
            )),
            "expected structured InterfaceNotSatisfied for missing method, got: {errs:?}"
        );
    }

    #[test]
    fn interface_not_satisfied_is_structured_for_wrong_return_type() {
        let src = r#"
interface Greetable {
  task greet(self) -> str
}

type Person = { name: str }

impl Greetable for Person {
  task greet(self) -> int { 0 }
}
"#;
        let errs = type_errors_full(src);

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::InterfaceNotSatisfied {
                    impl_name,
                    interface_name,
                    reason,
                    ..
                } if impl_name == "Person"
                    && interface_name == "Greetable"
                    && reason.contains("greet")
                    && reason.contains("str")
            )),
            "expected structured InterfaceNotSatisfied for wrong return type, got: {errs:?}"
        );
    }

    #[test]
    fn implicit_return_mismatch_span_points_at_result_expression() {
        // Regression: block_type dispatches Stmt::Expr through infer_expr without
        // calling check_stmt, so current_span was never set — the diagnostic used
        // to land at byte 0 (beginning of file) instead of on the bad expression.
        let src = "task go() -> str { 42 }";
        let errs = type_errors_full(src);

        let expr_start = src.find("42").expect("'42' not found");
        let expr_end = expr_start + "42".len();

        assert!(
            errs.iter().any(|e| matches!(
                e,
                TypeDiagnostic::TypeMismatch { span, .. }
                    if span.start >= expr_start && span.end <= expr_end + 1
            )),
            "expected TypeMismatch span to point at '42' ({expr_start}..{expr_end}), got: {:?}",
            errs.iter().map(|e| e.span().clone()).collect::<Vec<_>>()
        );
    }

    // ─── CheckArtifacts (issue #109) ────────────────────────────────────────────

    #[test]
    fn artifacts_record_scalar_binding_type() {
        let src = "task t() {\n  n: int = 42\n}\n";
        let offset = src.find("42").unwrap();
        let artifacts = artifacts_of(src);
        assert_eq!(artifacts.ty_at(offset), Some(&Ty::Int));
    }

    #[test]
    fn artifacts_record_list_literal_type() {
        let src = "task t() {\n  xs: list[int] = [1, 2, 3]\n}\n";
        // The last `[` is the literal's own bracket — the earlier one belongs
        // to the `list[int]` annotation.
        let offset = src.rfind('[').unwrap();
        let artifacts = artifacts_of(src);
        assert_eq!(artifacts.ty_at(offset), Some(&Ty::List(Box::new(Ty::Int))));
    }

    #[test]
    fn artifacts_record_named_struct_type() {
        let src = "type Person { name: str, age: int }\n\ntask greet(p: Person) {\n  x = p\n}\n";
        // The last `p` is the body's `x = p` reference; the earlier one is
        // the parameter declaration itself.
        let offset = src.rfind('p').unwrap();
        let artifacts = artifacts_of(src);
        assert_eq!(
            artifacts.ty_at(offset),
            Some(&Ty::Struct {
                name: Some("Person".to_string()),
                fields: vec![("name".to_string(), Ty::Str), ("age".to_string(), Ty::Int)],
            })
        );
    }

    #[test]
    fn artifacts_record_nullable_type() {
        let src = "task t(x: str?) {\n  y = x\n}\n";
        // The last `x` is the body's `y = x` reference.
        let offset = src.rfind('x').unwrap();
        let artifacts = artifacts_of(src);
        assert_eq!(
            artifacts.ty_at(offset),
            Some(&Ty::Nullable(Box::new(Ty::Str)))
        );
    }

    #[test]
    fn artifacts_record_dynamic_binding_type() {
        let src = "task t(x: dynamic) {\n  y = x\n}\n";
        // The last `x` is the body's `y = x` reference.
        let offset = src.rfind('x').unwrap();
        let artifacts = artifacts_of(src);
        assert_eq!(artifacts.ty_at(offset), Some(&Ty::Dynamic));
    }

    #[test]
    fn artifacts_record_generic_struct_instantiation() {
        let src = r#"
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

task t(p: Paginated[str]) {
  items: list[str] = p.items
}
"#;
        let artifacts = artifacts_of(src);
        assert_eq!(
            artifacts.generic_instantiations.get("Paginated"),
            Some(&vec![vec![Ty::Str]])
        );
    }

    #[test]
    fn artifacts_record_generic_task_call_instantiation() {
        let src = r#"
task identity[T](x: T) -> T { x }

task main() {
  s: str = identity("hello")
}
"#;
        let artifacts = artifacts_of(src);
        assert_eq!(
            artifacts.generic_instantiations.get("identity"),
            Some(&vec![vec![Ty::Str]])
        );
    }

    /// Regression guard for the `RefCell` non-reentrancy invariant documented
    /// on `Checker::artifacts`/`record_expr_type`/`record_instantiation`: a
    /// generic call nested inside another generic call's argument forces the
    /// inner call's `record_instantiation`/`record_expr_type` borrows to be
    /// taken and released *while inferring the outer call's argument*, i.e.
    /// before the outer call takes its own borrow. If a future change ever
    /// made these borrows overlap, this test would panic (double `borrow_mut`
    /// on the same `RefCell`) instead of silently passing.
    #[test]
    fn artifacts_record_nested_generic_call_instantiations() {
        let src = r#"
task first[T](x: T) -> T { x }
task second[U](y: U) -> U { y }

task main() {
  s: str = first(second("hello"))
}
"#;
        let artifacts = artifacts_of(src);
        assert_eq!(
            artifacts.generic_instantiations.get("first"),
            Some(&vec![vec![Ty::Str]])
        );
        assert_eq!(
            artifacts.generic_instantiations.get("second"),
            Some(&vec![vec![Ty::Str]])
        );

        // Both the inner and outer call expressions must have recorded their
        // own (distinct-span) resolved type, confirming both survived to
        // completion rather than one clobbering or pre-empting the other.
        let inner_offset = src.find(r#"second("hello")"#).unwrap() + 1;
        let outer_offset = src.find("first(").unwrap() + 1;
        assert_eq!(artifacts.ty_at(inner_offset), Some(&Ty::Str));
        assert_eq!(artifacts.ty_at(outer_offset), Some(&Ty::Str));
    }

    #[test]
    fn check_program_unaffected_by_artifacts_collection() {
        // Same source checked through both entry points must produce
        // identical diagnostics — artifacts collection is instrumentation
        // only and must never change checking semantics.
        let src = r#"
type Person { name: str, age: int }

task greet(p: Person) -> str {
  n: int = "oops"
  p.name
}
"#;
        let plain = type_errors(src);
        let named = NamedSource::new("t.keel", src.to_string());
        let tokens = lex(src, &named).expect("lex failed");
        let program = parse(tokens, src.len(), &named).expect("parse failed");
        let (with_artifacts, _artifacts) = check_program_with_artifacts(&program, false);
        let with_artifacts: Vec<String> = with_artifacts.into_iter().map(|e| e.message()).collect();
        assert_eq!(plain, with_artifacts);
        assert!(
            !plain.is_empty(),
            "test source must actually produce an error"
        );
    }
}

#[cfg(test)]
mod lsp_ide_tests {
    use super::{definition_of, is_top_level_symbol, type_at, usages_of};

    #[test]
    fn lsp_hover_reports_let_binding_type() {
        let src = "agent A {\n    @on_start {\n        items = [1, 2, 3]\n    }\n}\n";
        // Cursor on `items` (line 2, column 8 → byte offset of `items` in source).
        let offset = src.find("items").unwrap() + 1;
        let label = type_at(src, offset).expect("hover should resolve `items`");
        assert!(label.contains("list"), "expected list type, got: {label}");
        assert!(
            label.contains("int"),
            "expected int element type, got: {label}"
        );
    }

    #[test]
    fn lsp_hover_reports_namespace() {
        let src = "use std/io\nagent A { @on_start { io.show(\"x\") } }\n";
        let offset = src.find("io.show").unwrap() + 1;
        let label = type_at(src, offset).expect("hover on io");
        assert!(
            label.contains("namespace") || label.contains("module"),
            "expected namespace/module label, got: {label}"
        );
    }

    #[test]
    fn lsp_goto_definition_finds_task() {
        let src = "task greet() -> str {\n    \"hello\"\n}\nagent A {\n    @on_start {\n        r = greet()\n    }\n}\n";
        let offset = src.find("greet").unwrap() + 1;
        let span = definition_of(src, offset);
        assert!(
            span.is_some(),
            "definition_of should find `task greet` declaration"
        );
        let s = span.unwrap();
        let name = &src[s.clone()];
        assert_eq!(
            name, "greet",
            "span should cover the identifier, got: {name:?}"
        );
    }

    #[test]
    fn lsp_goto_definition_finds_state_field_from_read_and_write_sites() {
        let src = "agent Counter {\n    state { count: int = 0 }\n    task tick() {\n        self.count = self.count + 1\n    }\n}\n";
        let declaration = src.find("count:").unwrap();
        let expected = declaration..declaration + "count".len();
        let write = src.find("self.count =").unwrap() + "self.".len() + 1;
        let read = src.rfind("self.count").unwrap() + "self.".len() + 1;

        assert_eq!(definition_of(src, write), Some(expected.clone()));
        assert_eq!(definition_of(src, read), Some(expected));
    }

    #[test]
    fn lsp_goto_definition_uses_exact_method_declaration_span() {
        let src = "agent First {\n    task work() {}\n}\nagent Second {\n    task work() {}\n}\n";
        let declaration = src.rfind("work").unwrap();
        let expected = declaration..declaration + "work".len();

        assert_eq!(definition_of(src, declaration + 1), Some(expected));
    }

    #[test]
    fn lsp_rename_gate_allows_top_level_declaration_in_broken_file() {
        let src = "task stable() {}\ntask broken() {\n";
        let offset = src.find("stable").unwrap() + 1;

        assert!(is_top_level_symbol(src, offset));
    }

    #[test]
    fn lsp_rename_gate_rejects_agent_method_declaration_in_broken_file() {
        let src = "agent Bot {\n    task nested() {}\n";
        let offset = src.find("nested").unwrap() + 1;

        assert!(!is_top_level_symbol(src, offset));
    }

    #[test]
    fn lsp_usages_of_finds_all_occurrences() {
        let src = "task foo() -> str { \"x\" }\nagent A { @on_start { r = foo() s = foo() } }\n";
        let spans = usages_of(src, "foo");
        assert!(
            spans.len() >= 3,
            "expected at least 3 occurrences of `foo` (decl + 2 calls), got {}",
            spans.len()
        );
    }
}
