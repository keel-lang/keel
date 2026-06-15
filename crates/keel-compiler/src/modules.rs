//! Module graph loader.
//!
//! Resolves the `use` declarations of an entry file into a [`ModuleGraph`]:
//! every transitively imported `.keel` file is read and parsed exactly once,
//! relative paths resolve from the importing file's directory, and circular
//! imports are rejected with the full cycle path. `std/<name>` targets are
//! validated against the runtime catalog and never touch the filesystem.
//!
//! The loader owns I/O and resolution only. Name collisions, member lookup,
//! and visibility are checked later by the type checker, which sees the
//! whole graph.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::{LabeledSpan, NamedSource, Result};

use crate::ast::{Decl, Program, UseDecl, UseKind, UseSource};
use crate::lexer::Span;

/// Lex and parse a single module's source into a [`Program`].
///
/// The compiler crate parses directly via the syntax layer rather than going
/// through the top-level `session` API, which lives above it.
fn parse_source(src: &str, name: &str) -> Result<(Program, NamedSource<String>)> {
    let named = NamedSource::new(name, src.to_string());
    let tokens = crate::lexer::lex(src, &named)?;
    let program = crate::parser::parse(tokens, src.len(), &named)?;
    Ok((program, named))
}

/// What a `use` declaration resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleTarget {
    /// A std module — the canonical catalog namespace name (`"file"`, `"ai"`).
    Std(String),
    /// A local module — index into [`ModuleGraph::modules`].
    Local(usize),
}

/// A module-namespace binding introduced by `use ... [as x]`.
#[derive(Debug, Clone)]
pub struct ModuleBindingDecl {
    /// The identifier bound in the importing file's scope.
    pub name: String,
    pub target: ModuleTarget,
    /// Span of the whole `use` declaration, for diagnostics.
    pub span: Span,
}

/// One symbol imported by `use A, B as C from ...`.
#[derive(Debug, Clone)]
pub struct SymbolImportDecl {
    /// The identifier bound in the importing file's scope.
    pub local: String,
    /// The declaration name in the source module.
    pub original: String,
    pub target: ModuleTarget,
    /// Span of the imported symbol name token.
    pub span: Span,
}

/// Resolved imports of one module, in declaration order.
#[derive(Debug, Clone, Default)]
pub struct ModuleImports {
    pub bindings: Vec<ModuleBindingDecl>,
    pub symbols: Vec<SymbolImportDecl>,
}

/// One loaded source file together with its resolved imports.
#[derive(Debug)]
pub struct ModuleUnit {
    /// Canonical path. `None` only for an in-memory entry (REPL, tests).
    pub path: Option<PathBuf>,
    /// Default namespace name — the file stem.
    pub name: String,
    pub source: NamedSource<String>,
    pub program: Program,
    pub imports: ModuleImports,
}

/// The transitively loaded program: dependencies first, entry file last.
#[derive(Debug)]
pub struct ModuleGraph {
    pub modules: Vec<ModuleUnit>,
}

impl ModuleGraph {
    /// The entry module — the file that was run, checked, or tested.
    #[must_use]
    pub fn entry(&self) -> &ModuleUnit {
        self.modules.last().expect("module graph is never empty")
    }

    /// Index of the entry module.
    #[must_use]
    pub fn entry_index(&self) -> usize {
        self.modules.len() - 1
    }

    /// True when the program is a single file with no local imports.
    #[must_use]
    pub fn is_single_module(&self) -> bool {
        self.modules.len() == 1
    }
}

/// Load the full module graph for an already-read entry source.
///
/// `entry_path` anchors relative imports; without it (in-memory source),
/// any `use "./..."` declaration is an error but `use std/...` still works.
///
/// # Errors
///
/// Returns an error if any imported file cannot be read or parsed, a
/// relative import cannot be resolved, an unknown `std` module is named,
/// or the imports form a cycle.
pub fn load_graph(
    entry_src: &str,
    entry_name: &str,
    entry_path: Option<&Path>,
) -> Result<ModuleGraph> {
    let (program, source) = parse_source(entry_src, entry_name)?;
    let mut loader = Loader {
        modules: Vec::new(),
        by_path: HashMap::new(),
        stack: Vec::new(),
    };
    let entry_canonical = entry_path.and_then(|p| p.canonicalize().ok().or(Some(p.to_path_buf())));
    loader.load_unit(
        program,
        source,
        entry_canonical,
        module_name_for(entry_path, entry_name),
    )?;
    Ok(ModuleGraph {
        modules: loader.modules,
    })
}

fn module_name_for(path: Option<&Path>, fallback_name: &str) -> String {
    let stem = path
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .map(str::to_owned);
    stem.unwrap_or_else(|| {
        fallback_name
            .strip_suffix(".keel")
            .unwrap_or(fallback_name)
            .to_owned()
    })
}

/// The set of valid `use std/<name>` targets.
pub(crate) fn std_module_names() -> &'static std::collections::HashSet<String> {
    crate::types::prelude::namespace_names()
}

struct Loader {
    /// Finished modules in post-order (dependencies before importers).
    modules: Vec<ModuleUnit>,
    /// Canonical path → index in `modules` for finished modules.
    by_path: HashMap<PathBuf, usize>,
    /// Paths currently being loaded (DFS stack) for cycle detection.
    stack: Vec<PathBuf>,
}

impl Loader {
    /// Load one parsed unit: resolve its imports (recursing into local
    /// files), then append it to the finished list. Returns its index.
    fn load_unit(
        &mut self,
        program: Program,
        source: NamedSource<String>,
        path: Option<PathBuf>,
        name: String,
    ) -> Result<usize> {
        if let Some(p) = &path {
            self.stack.push(p.clone());
        }
        let imports = self.resolve_imports(&program, &source, path.as_deref());
        if path.is_some() {
            self.stack.pop();
        }
        let imports = imports?;
        let index = self.modules.len();
        if let Some(p) = &path {
            self.by_path.insert(p.clone(), index);
        }
        self.modules.push(ModuleUnit {
            path,
            name,
            source,
            program,
            imports,
        });
        Ok(index)
    }

    fn resolve_imports(
        &mut self,
        program: &Program,
        source: &NamedSource<String>,
        importer_path: Option<&Path>,
    ) -> Result<ModuleImports> {
        let mut imports = ModuleImports::default();
        for node in &program.declarations {
            let Decl::Use(UseDecl { kind }) = &node.kind else {
                continue;
            };
            match kind {
                UseKind::Module {
                    source: use_source,
                    alias,
                } => {
                    let target =
                        self.resolve_target(use_source, source, importer_path, &node.span)?;
                    let name = alias
                        .clone()
                        .unwrap_or_else(|| use_source.default_binding().to_owned());
                    require_ident(&name, source, &node.span)?;
                    imports.bindings.push(ModuleBindingDecl {
                        name,
                        target,
                        span: node.span.clone(),
                    });
                }
                UseKind::Symbols {
                    items,
                    source: use_source,
                } => {
                    let target =
                        self.resolve_target(use_source, source, importer_path, &node.span)?;
                    for item in items {
                        imports.symbols.push(SymbolImportDecl {
                            local: item.alias.clone().unwrap_or_else(|| item.name.clone()),
                            original: item.name.clone(),
                            target: target.clone(),
                            span: item.name_span.clone(),
                        });
                    }
                }
            }
        }
        Ok(imports)
    }

    fn resolve_target(
        &mut self,
        use_source: &UseSource,
        source: &NamedSource<String>,
        importer_path: Option<&Path>,
        span: &Span,
    ) -> Result<ModuleTarget> {
        match use_source {
            UseSource::Module(segments) => resolve_std_target(segments, source, span),
            UseSource::File(raw) => self.resolve_file_target(raw, source, importer_path, span),
        }
    }

    fn resolve_file_target(
        &mut self,
        raw: &str,
        source: &NamedSource<String>,
        importer_path: Option<&Path>,
        span: &Span,
    ) -> Result<ModuleTarget> {
        if !raw.ends_with(".keel") {
            return Err(use_error(
                source,
                span,
                format!("imported file `{raw}` must have a `.keel` extension"),
                None,
            ));
        }
        let Some(importer) = importer_path else {
            return Err(use_error(
                source,
                span,
                format!("cannot resolve `{raw}` without a source file path"),
                Some("relative imports are unavailable for in-memory programs".into()),
            ));
        };
        let base = importer.parent().unwrap_or_else(|| Path::new("."));
        let joined = base.join(raw);
        let canonical = joined.canonicalize().map_err(|err| {
            use_error(
                source,
                span,
                format!("cannot read `{}`: {err}", joined.display()),
                None,
            )
        })?;

        if let Some(&index) = self.by_path.get(&canonical) {
            return Ok(ModuleTarget::Local(index));
        }
        if self.stack.contains(&canonical) {
            let mut cycle: Vec<String> = self
                .stack
                .iter()
                .skip_while(|p| **p != canonical)
                .map(|p| display_name(p))
                .collect();
            cycle.push(display_name(&canonical));
            return Err(use_error(
                source,
                span,
                format!("circular import: {}", cycle.join(" → ")),
                Some("move the shared declarations into a third file both can import".into()),
            ));
        }

        let text = std::fs::read_to_string(&canonical).map_err(|err| {
            use_error(
                source,
                span,
                format!("cannot read `{}`: {err}", canonical.display()),
                None,
            )
        })?;
        let display = canonical.to_string_lossy().to_string();
        let (program, module_source) = parse_source(&text, &display)?;
        let name = module_name_for(Some(&canonical), &display);
        require_ident(&name, source, span)?;
        let index = self.load_unit(program, module_source, Some(canonical), name)?;
        Ok(ModuleTarget::Local(index))
    }
}

fn resolve_std_target(
    segments: &[String],
    source: &NamedSource<String>,
    span: &Span,
) -> Result<ModuleTarget> {
    let path = segments.join("/");
    if segments.len() == 2 && segments[0] == "std" {
        let name = &segments[1];
        if std_module_names().contains(name) {
            return Ok(ModuleTarget::Std(name.clone()));
        }
        // `agent` itself is a keyword and never parses here; `agents` is the
        // plural users will guess at.
        if name == "agents" {
            return Err(use_error(
                source,
                span,
                "there is no `std/agents` module".into(),
                Some(
                    "agent verbs are built into the language: run, stop, send, delegate, broadcast"
                        .into(),
                ),
            ));
        }
        let mut known: Vec<&str> = std_module_names().iter().map(String::as_str).collect();
        known.sort_unstable();
        return Err(use_error(
            source,
            span,
            format!("unknown std module `std/{name}`"),
            Some(format!("available std modules: {}", known.join(", "))),
        ));
    }
    if segments.first().map(String::as_str) == Some("std") {
        return Err(use_error(
            source,
            span,
            format!("invalid std module path `{path}`"),
            Some("std modules are flat: use std/<name>".into()),
        ));
    }
    Err(use_error(
        source,
        span,
        format!("unsupported package path `{path}`"),
        Some("only `std/<name>` and relative file imports are available; community packages are reserved for a future release".into()),
    ))
}

fn require_ident(name: &str, source: &NamedSource<String>, span: &Span) -> Result<()> {
    let valid = !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        return Ok(());
    }
    Err(use_error(
        source,
        span,
        format!("`{name}` is not a valid module name"),
        Some("bind the import explicitly: use \"./path.keel\" as <name>".into()),
    ))
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn use_error(
    source: &NamedSource<String>,
    span: &Span,
    message: String,
    help: Option<String>,
) -> miette::Report {
    let label = LabeledSpan::at(span.clone(), message.clone());
    match help {
        Some(help) => miette::miette!(labels = vec![label], help = help, "{message}"),
        None => miette::miette!(labels = vec![label], "{message}"),
    }
    .with_source_code(source.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("create module file");
        file.write_all(content.as_bytes()).expect("write module");
        path
    }

    #[test]
    fn loads_single_file_graph() {
        let graph = load_graph("task t() { }", "main.keel", None).expect("load");
        assert!(graph.is_single_module());
        assert_eq!(graph.entry().name, "main");
    }

    #[test]
    fn loads_local_import_with_dependency_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_file(
            dir.path(),
            "validation.keel",
            "task email(s: str) -> bool { true }\n",
        );
        let entry = write_file(dir.path(), "main.keel", "use \"./validation.keel\"\n");
        let src = std::fs::read_to_string(&entry).unwrap();
        let graph = load_graph(&src, "main.keel", Some(&entry)).expect("load");
        assert_eq!(graph.modules.len(), 2);
        assert_eq!(graph.modules[0].name, "validation");
        assert_eq!(graph.entry().name, "main");
        assert_eq!(
            graph.entry().imports.bindings[0].target,
            ModuleTarget::Local(0)
        );
        assert_eq!(graph.entry().imports.bindings[0].name, "validation");
    }

    #[test]
    fn shared_import_is_loaded_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_file(dir.path(), "shared.keel", "task s() { }\n");
        write_file(dir.path(), "a.keel", "use \"./shared.keel\"\n");
        write_file(dir.path(), "b.keel", "use \"./shared.keel\"\n");
        let entry = write_file(
            dir.path(),
            "main.keel",
            "use \"./a.keel\"\nuse \"./b.keel\"\n",
        );
        let src = std::fs::read_to_string(&entry).unwrap();
        let graph = load_graph(&src, "main.keel", Some(&entry)).expect("load");
        let shared_count = graph.modules.iter().filter(|m| m.name == "shared").count();
        assert_eq!(shared_count, 1, "shared module must be parsed exactly once");
        assert_eq!(graph.modules.len(), 4);
    }

    #[test]
    fn circular_import_is_rejected_with_cycle_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_file(dir.path(), "a.keel", "use \"./b.keel\"\n");
        write_file(dir.path(), "b.keel", "use \"./a.keel\"\n");
        let entry = write_file(dir.path(), "main.keel", "use \"./a.keel\"\n");
        let src = std::fs::read_to_string(&entry).unwrap();
        let err = load_graph(&src, "main.keel", Some(&entry)).expect_err("cycle must fail");
        let msg = format!("{err:?}");
        assert!(msg.contains("circular import"), "got: {msg}");
        assert!(
            msg.contains("a.keel → b.keel → a.keel"),
            "cycle path should be spelled out, got: {msg}"
        );
    }

    #[test]
    fn missing_file_errors_with_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entry = write_file(dir.path(), "main.keel", "use \"./nope.keel\"\n");
        let src = std::fs::read_to_string(&entry).unwrap();
        let err = load_graph(&src, "main.keel", Some(&entry)).expect_err("missing file");
        assert!(format!("{err:?}").contains("nope.keel"));
    }

    #[test]
    fn std_import_resolves_against_catalog() {
        let graph = load_graph("use std/file\n", "main.keel", None).expect("load");
        assert_eq!(
            graph.entry().imports.bindings[0].target,
            ModuleTarget::Std("file".into())
        );
        assert_eq!(graph.entry().imports.bindings[0].name, "file");
    }

    #[test]
    fn std_alias_binds_alias_name() {
        let graph = load_graph("use std/file as f\n", "main.keel", None).expect("load");
        assert_eq!(graph.entry().imports.bindings[0].name, "f");
    }

    #[test]
    fn unknown_std_module_lists_available() {
        let err = load_graph("use std/nope\n", "main.keel", None).expect_err("unknown std");
        let msg = format!("{err:?}");
        assert!(msg.contains("unknown std module"), "got: {msg}");
        assert!(msg.contains("file"), "should list available modules: {msg}");
    }

    #[test]
    fn std_agents_gets_dissolution_hint() {
        let err = load_graph("use std/agents\n", "main.keel", None).expect_err("no std/agents");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("built into the language"),
            "should explain agent verbs are ambient: {msg}"
        );
    }

    #[test]
    fn community_paths_are_reserved() {
        let err = load_graph("use community/crm\n", "main.keel", None).expect_err("reserved");
        assert!(format!("{err:?}").contains("reserved"));
    }

    #[test]
    fn file_import_without_entry_path_errors() {
        let err = load_graph("use \"./x.keel\"\n", "main.keel", None).expect_err("no anchor path");
        assert!(format!("{err:?}").contains("without a source file path"));
    }

    #[test]
    fn symbol_import_records_local_and_original_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_file(
            dir.path(),
            "models.keel",
            "type Classifier { name: str }\ntype Urgency = low | high\n",
        );
        let entry = write_file(
            dir.path(),
            "main.keel",
            "use Classifier, Urgency as U from \"./models.keel\"\n",
        );
        let src = std::fs::read_to_string(&entry).unwrap();
        let graph = load_graph(&src, "main.keel", Some(&entry)).expect("load");
        let symbols = &graph.entry().imports.symbols;
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].local, "Classifier");
        assert_eq!(symbols[0].original, "Classifier");
        assert_eq!(symbols[1].local, "U");
        assert_eq!(symbols[1].original, "Urgency");
    }
}
