//! Read-only high-level intermediate representation for semantic analysis.
//!
//! The HIR borrows the parser AST and adds stable symbol IDs, resolved
//! identifier references, and brace-literal classification. The interpreter
//! still executes the AST in v0.1; checker and IDE consumers use this index so
//! semantic decisions have one lowering boundary.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::lexer::Span;
use crate::types::diagnostics::TypeDiagnostic;
use crate::types::prelude::prelude_names;

/// Stable identifier for a symbol within one lowered program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(usize);

/// Semantic category of a declared or bound symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Top-level task declaration.
    TopTask,
    /// Agent declaration.
    Agent,
    /// Enum type declaration.
    Enum,
    /// Struct or alias type declaration.
    TypeName,
    /// Interface declaration.
    Interface,
    /// Extern declaration.
    Extern,
    /// Agent task, impl method, or interface method.
    Method,
    /// Local binding, parameter, state field, or handler parameter.
    Binding,
}

/// Symbol declaration stored by the HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Stable symbol identifier.
    pub id: SymbolId,
    /// Declared source name.
    pub name: String,
    /// Semantic declaration category.
    pub kind: SymbolKind,
    /// Best available source span for the declaration site.
    pub span: Span,
}

/// Semantic category returned by name resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// Top-level task declaration.
    TopTask,
    /// Agent declaration.
    Agent,
    /// Enum type declaration.
    Enum,
    /// Struct or alias type declaration.
    TypeName,
    /// Auto-imported namespace or built-in free identifier.
    PreludeNamespace,
    /// Lexically scoped local binding.
    Local,
    /// Name could not be resolved.
    Unresolved,
}

/// Resolved identifier reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    /// Referenced symbol when the name resolves to a source declaration.
    pub symbol: Option<SymbolId>,
    /// Semantic category of the resolved name.
    pub kind: NameKind,
}

impl Resolution {
    const UNRESOLVED: Self = Self {
        symbol: None,
        kind: NameKind::Unresolved,
    };
}

/// Map-key category assigned to a lowered brace literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKeyKind {
    /// String-keyed map.
    Str,
    /// Integer-keyed map.
    Int,
    /// Boolean-keyed map.
    Bool,
}

/// Semantic classification assigned to a parsed brace literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    /// Structural record literal.
    Struct,
    /// Map literal with one consistent key category.
    Map(MapKeyKind),
    /// Map literal containing incompatible key categories.
    InvalidMixedKeys,
}

/// Borrowed HIR index produced after parsing.
pub struct Hir<'ast> {
    program: &'ast Program,
    symbols: Vec<Symbol>,
    globals: HashMap<String, Resolution>,
    references: HashMap<(usize, usize), Resolution>,
    literal_kinds: HashMap<(usize, usize), LiteralKind>,
    diagnostics: Vec<TypeDiagnostic>,
}

impl<'ast> Hir<'ast> {
    /// Return the parser AST borrowed by this index.
    #[must_use]
    pub fn program(&self) -> &'ast Program {
        self.program
    }

    /// Return all symbols declared or bound in this program.
    #[must_use]
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    /// Return diagnostics emitted while lowering.
    #[must_use]
    pub fn diagnostics(&self) -> &[TypeDiagnostic] {
        &self.diagnostics
    }

    /// Resolve a global name.
    #[must_use]
    pub fn resolve_global(&self, name: &str) -> Resolution {
        let resolution = self
            .globals
            .get(name)
            .copied()
            .unwrap_or(Resolution::UNRESOLVED);
        assert!(
            !matches!(resolution.kind, NameKind::Local),
            "HIR global index contains a local symbol"
        );
        resolution
    }

    /// Resolve the identifier expression at `span`.
    #[must_use]
    pub fn resolution_at(&self, span: &Span) -> Resolution {
        self.references
            .get(&span_key(span))
            .copied()
            .unwrap_or(Resolution::UNRESOLVED)
    }

    /// Return the semantic brace-literal kind for an AST expression.
    #[must_use]
    pub fn literal_kind(&self, expr: &SpannedExpr) -> Option<LiteralKind> {
        self.literal_kinds.get(&span_key(&expr.span)).copied()
    }

    /// Return a symbol by ID.
    #[must_use]
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.0)
    }
}

/// Lower a parsed AST into a read-only HIR index.
#[must_use]
pub fn lower_ast(program: &Program) -> Hir<'_> {
    Lowerer::new(program).lower()
}

fn span_key(span: &Span) -> (usize, usize) {
    (span.start, span.end)
}

struct Lowerer<'ast> {
    program: &'ast Program,
    symbols: Vec<Symbol>,
    globals: HashMap<String, Resolution>,
    references: HashMap<(usize, usize), Resolution>,
    literal_kinds: HashMap<(usize, usize), LiteralKind>,
    diagnostics: Vec<TypeDiagnostic>,
    scopes: Vec<HashMap<String, SymbolId>>,
    aliases: HashMap<String, TypeExpr>,
    structs: HashMap<String, Vec<Field>>,
    top_tasks: HashMap<String, Vec<Param>>,
    agent_tasks: HashMap<String, HashMap<String, SymbolId>>,
    agent_fields: HashMap<String, HashMap<String, SymbolId>>,
    current_agent: Option<String>,
}

impl<'ast> Lowerer<'ast> {
    fn new(program: &'ast Program) -> Self {
        Self {
            program,
            symbols: Vec::new(),
            globals: HashMap::new(),
            references: HashMap::new(),
            literal_kinds: HashMap::new(),
            diagnostics: Vec::new(),
            scopes: vec![HashMap::new()],
            aliases: HashMap::new(),
            structs: HashMap::new(),
            top_tasks: HashMap::new(),
            agent_tasks: HashMap::new(),
            agent_fields: HashMap::new(),
            current_agent: None,
        }
    }

    fn lower(mut self) -> Hir<'ast> {
        self.collect_globals();
        for node in &self.program.declarations {
            self.lower_decl(&node.kind, &node.span);
        }
        Hir {
            program: self.program,
            symbols: self.symbols,
            globals: self.globals,
            references: self.references,
            literal_kinds: self.literal_kinds,
            diagnostics: self.diagnostics,
        }
    }

    fn collect_globals(&mut self) {
        for name in prelude_names() {
            self.globals.insert(
                name,
                Resolution {
                    symbol: None,
                    kind: NameKind::PreludeNamespace,
                },
            );
        }

        for node in &self.program.declarations {
            if let crate::ast::Decl::Type(decl) = &node.kind {
                let kind = match &decl.def {
                    TypeDef::SimpleEnum(_) | TypeDef::RichEnum(_) => NameKind::Enum,
                    TypeDef::Struct(_) | TypeDef::Alias(_) => NameKind::TypeName,
                };
                let symbol_kind = match kind {
                    NameKind::Enum => SymbolKind::Enum,
                    NameKind::TypeName => SymbolKind::TypeName,
                    _ => unreachable!("type declarations lower to enum or type symbols"),
                };
                let id = self.add_symbol(&decl.name, symbol_kind, decl.name_span.clone());
                self.globals.insert(
                    decl.name.clone(),
                    Resolution {
                        symbol: Some(id),
                        kind,
                    },
                );
                match &decl.def {
                    TypeDef::Alias(node) => {
                        self.aliases.insert(decl.name.clone(), node.kind.clone());
                    }
                    TypeDef::Struct(fields) => {
                        self.structs.insert(decl.name.clone(), fields.clone());
                    }
                    TypeDef::SimpleEnum(_) | TypeDef::RichEnum(_) => {}
                }
            }
        }

        for node in &self.program.declarations {
            if let crate::ast::Decl::Agent(decl) = &node.kind {
                let id = self.add_symbol(&decl.name, SymbolKind::Agent, decl.name_span.clone());
                self.globals.insert(
                    decl.name.clone(),
                    Resolution {
                        symbol: Some(id),
                        kind: NameKind::Agent,
                    },
                );
                let mut tasks = HashMap::new();
                let mut fields = HashMap::new();
                for item in &decl.items {
                    match item {
                        AgentItem::Task(task) => {
                            let id = self.add_symbol(
                                &task.name,
                                SymbolKind::Method,
                                task.name_span.clone(),
                            );
                            tasks.insert(task.name.clone(), id);
                        }
                        AgentItem::State(state_fields) => {
                            for field in state_fields {
                                let id = self.add_symbol(
                                    &field.name,
                                    SymbolKind::Binding,
                                    field.name_span.clone(),
                                );
                                fields.insert(field.name.clone(), id);
                            }
                        }
                        AgentItem::Attribute(_) | AgentItem::On(_) => {}
                    }
                }
                self.agent_tasks.insert(decl.name.clone(), tasks);
                self.agent_fields.insert(decl.name.clone(), fields);
            }
        }

        for node in &self.program.declarations {
            if let crate::ast::Decl::Task(decl) = &node.kind {
                let id = self.add_symbol(&decl.name, SymbolKind::TopTask, decl.name_span.clone());
                self.globals.insert(
                    decl.name.clone(),
                    Resolution {
                        symbol: Some(id),
                        kind: NameKind::TopTask,
                    },
                );
                self.top_tasks
                    .insert(decl.name.clone(), decl.params.clone());
            }
        }
    }

    fn lower_decl(&mut self, decl: &crate::ast::Decl, _span: &Span) {
        match decl {
            crate::ast::Decl::Interface(decl) => {
                self.add_symbol(&decl.name, SymbolKind::Interface, decl.name_span.clone());
            }
            crate::ast::Decl::Extern(decl) => {
                self.add_symbol(&decl.name, SymbolKind::Extern, decl.name_span.clone());
            }
            _ => {}
        }

        match decl {
            crate::ast::Decl::Type(_) | crate::ast::Decl::Use(_) => {}
            crate::ast::Decl::Interface(decl) => {
                for method in &decl.methods {
                    self.add_symbol(&method.name, SymbolKind::Method, method.name_span.clone());
                    for param in &method.params {
                        self.add_binding(&param.name, param.name_span.clone());
                        if let Some(default) = &param.default {
                            self.lower_expr(default, Some(&param.ty.kind));
                        }
                    }
                }
            }
            crate::ast::Decl::Impl(decl) => {
                for method in &decl.methods {
                    self.lower_task(method, true);
                }
            }
            crate::ast::Decl::Task(decl) => self.lower_task(decl, false),
            crate::ast::Decl::Extern(decl) => {
                for param in &decl.params {
                    self.add_binding(&param.name, param.name_span.clone());
                    if let Some(default) = &param.default {
                        self.lower_expr(default, Some(&param.ty.kind));
                    }
                }
            }
            crate::ast::Decl::Agent(decl) => {
                let previous_agent = self.current_agent.replace(decl.name.clone());
                for item in &decl.items {
                    self.lower_agent_item(item);
                }
                self.current_agent = previous_agent;
            }
            crate::ast::Decl::Stmt(node) => self.lower_stmt(&node.kind, &node.span, None),
        }
    }

    fn lower_agent_item(&mut self, item: &AgentItem) {
        match item {
            AgentItem::Attribute(attr) => match &attr.body {
                AttributeBody::Expr(expr) => self.lower_expr(expr, None),
                AttributeBody::Block(block) => self.lower_block(block, None),
                AttributeBody::Tools(entries) => {
                    for entry in entries {
                        if let Some(condition) = &entry.condition {
                            self.lower_expr(condition, None);
                        }
                    }
                }
            },
            AgentItem::State(fields) => {
                for field in fields {
                    self.lower_expr(&field.default, Some(&field.ty.kind));
                }
            }
            AgentItem::Task(task) => self.lower_task(task, true),
            AgentItem::On(handler) => {
                self.push_scope();
                if let Some(param) = &handler.param {
                    if let Some(default) = &param.default {
                        self.lower_expr(default, Some(&param.ty.kind));
                    }
                    self.define_binding(&param.name, param.name_span.clone());
                }
                self.lower_block(&handler.body, None);
                self.pop_scope();
            }
        }
    }

    fn lower_task(&mut self, task: &TaskDecl, nested: bool) {
        if nested && self.current_agent_task(&task.name).is_none() {
            self.add_symbol(&task.name, SymbolKind::Method, task.name_span.clone());
        }
        for param in &task.params {
            if let Some(default) = &param.default {
                self.lower_expr(default, Some(&param.ty.kind));
            }
        }
        self.push_scope();
        for param in &task.params {
            self.define_binding(&param.name, param.name_span.clone());
        }
        self.lower_block(&task.body, task.return_type.as_ref().map(|node| &node.kind));
        self.pop_scope();
    }

    fn lower_block(&mut self, block: &Block, return_ty: Option<&TypeExpr>) {
        for (index, node) in block.iter().enumerate() {
            let expected = (index + 1 == block.len()).then_some(return_ty).flatten();
            self.lower_stmt(&node.kind, &node.span, expected);
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt, span: &Span, return_ty: Option<&TypeExpr>) {
        match stmt {
            Stmt::Let { binding, ty, value } => {
                self.lower_expr(value, ty.as_ref().map(|node| &node.kind));
                self.define_binding(binding, span.clone());
            }
            Stmt::SelfAssign {
                field,
                field_span,
                value,
            } => {
                if let Some(symbol) = self.current_agent_field(field) {
                    self.references.insert(
                        span_key(field_span),
                        Resolution {
                            symbol: Some(symbol),
                            kind: NameKind::Local,
                        },
                    );
                }
                self.lower_expr(value, None);
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.lower_expr(value, return_ty);
                }
            }
            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                self.lower_expr(iter, None);
                self.push_scope();
                self.define_binding(binding, span.clone());
                if let Some(filter) = filter {
                    self.lower_expr(filter, None);
                }
                self.lower_block(body, return_ty);
                self.pop_scope();
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.lower_expr(cond, None);
                self.push_scope();
                self.lower_block(then_body, return_ty);
                self.pop_scope();
                if let Some(else_body) = else_body {
                    self.push_scope();
                    self.lower_block(else_body, return_ty);
                    self.pop_scope();
                }
            }
            Stmt::When { subject, arms } => {
                self.lower_expr(subject, None);
                self.lower_when_arms(arms, return_ty);
            }
            Stmt::TryCatch { body, catches } => {
                self.push_scope();
                self.lower_block(body, return_ty);
                self.pop_scope();
                for catch in catches {
                    self.push_scope();
                    self.define_local(&catch.name, catch.ty.span.clone());
                    self.lower_block(&catch.body, return_ty);
                    self.pop_scope();
                }
            }
            Stmt::AugAssign {
                name,
                name_span,
                rhs,
                ..
            } => {
                self.lower_reference_without_diagnostic(name, name_span.clone());
                self.lower_expr(rhs, None);
            }
            Stmt::Raise(expr) => self.lower_expr(expr, None),
            Stmt::While { cond, body } => {
                self.lower_expr(cond, None);
                self.push_scope();
                self.lower_block(body, return_ty);
                self.pop_scope();
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Expr(expr) => self.lower_expr(expr, return_ty),
        }
    }

    fn lower_when_arms(&mut self, arms: &[WhenArm], return_ty: Option<&TypeExpr>) {
        for arm in arms {
            self.push_scope();
            for pattern in &arm.patterns {
                match pattern {
                    Pattern::Literal(expr) => self.lower_expr(expr, None),
                    Pattern::Variant { bindings, .. } => {
                        for binding in bindings {
                            if binding != "_" {
                                self.define_local(binding, exprless_span());
                            }
                        }
                    }
                    Pattern::Ident(_) | Pattern::Wildcard => {}
                }
            }
            if let Some(guard) = &arm.guard {
                self.lower_expr(guard, None);
            }
            self.lower_block(&arm.body, return_ty);
            self.pop_scope();
        }
    }

    fn lower_expr(&mut self, expr: &SpannedExpr, expected: Option<&TypeExpr>) {
        match &expr.kind {
            crate::ast::Expr::Ident(name) => {
                self.lower_reference(name, expr.span.clone());
            }
            crate::ast::Expr::StructLit(fields) => {
                let kind = self.classify_literal(fields, expected);
                self.literal_kinds.insert(span_key(&expr.span), kind);
                for (key, value) in fields {
                    let child_expected = self.literal_value_expected(expected, key);
                    self.lower_expr(value, child_expected.as_ref());
                }
            }
            crate::ast::Expr::StringLit(parts) => {
                for part in parts {
                    if let StringPart::Interpolation(value, _) = part {
                        self.lower_expr(value, None);
                    }
                }
            }
            crate::ast::Expr::FieldAccess(object, _)
            | crate::ast::Expr::NullFieldAccess(object, _)
            | crate::ast::Expr::NullAssert(object)
            | crate::ast::Expr::Duration { value: object, .. } => self.lower_expr(object, None),
            crate::ast::Expr::StructSpreadUpdate { base, overrides } => {
                self.lower_expr(base, expected);
                for (_, value) in overrides {
                    self.lower_expr(value, None);
                }
            }
            crate::ast::Expr::ListLit(items) | crate::ast::Expr::SetLit(items) => {
                let child_expected = self.collection_item_expected(expected);
                for item in items {
                    self.lower_expr(item, child_expected.as_ref());
                }
            }
            crate::ast::Expr::TupleLit(items) => {
                let expected_items = self.tuple_items_expected(expected);
                for (index, item) in items.iter().enumerate() {
                    self.lower_expr(item, expected_items.get(index));
                }
            }
            crate::ast::Expr::BinaryOp { left, right, .. }
            | crate::ast::Expr::NullCoalesce(left, right)
            | crate::ast::Expr::Pipeline(left, right)
            | crate::ast::Expr::Range(left, right) => {
                self.lower_expr(left, None);
                self.lower_expr(right, None);
            }
            crate::ast::Expr::UnaryOp { expr, .. } => self.lower_expr(expr, None),
            crate::ast::Expr::Call { callee, args } => {
                self.lower_expr(callee, None);
                if let crate::ast::Expr::SelfAccess {
                    field: method,
                    field_span,
                } = &callee.kind
                    && let Some(symbol) = self.current_agent_task(method)
                {
                    self.references.insert(
                        span_key(field_span),
                        Resolution {
                            symbol: Some(symbol),
                            kind: NameKind::Local,
                        },
                    );
                }
                let params = match &callee.kind {
                    crate::ast::Expr::Ident(name) => self.top_tasks.get(name).cloned(),
                    _ => None,
                };
                self.lower_call_args(args, params.as_deref());
            }
            crate::ast::Expr::MethodCall {
                object,
                method,
                args,
            } => {
                self.lower_expr(object, None);
                if matches!(object.kind, crate::ast::Expr::SelfRef)
                    && let Some(symbol) = self.current_agent_task(method)
                {
                    self.references.insert(
                        span_key(&expr.span),
                        Resolution {
                            symbol: Some(symbol),
                            kind: NameKind::Local,
                        },
                    );
                }
                for arg in args {
                    self.lower_expr(&arg.value, None);
                }
            }
            crate::ast::Expr::Cast { expr, ty } => self.lower_expr(expr, Some(&ty.kind)),
            crate::ast::Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } => {
                self.lower_expr(cond, None);
                self.push_scope();
                self.lower_block(then_body, expected);
                self.pop_scope();
                self.push_scope();
                self.lower_block(else_body, expected);
                self.pop_scope();
            }
            crate::ast::Expr::WhenExpr { subject, arms } => {
                self.lower_expr(subject, None);
                self.lower_when_arms(arms, expected);
            }
            crate::ast::Expr::Lambda { params, body } => {
                self.push_scope();
                for param in params {
                    self.define_local(&param.name, expr.span.clone());
                }
                match body {
                    LambdaBody::Expr(expr) => self.lower_expr(expr, None),
                    LambdaBody::Block(block) => self.lower_block(block, None),
                }
                self.pop_scope();
            }
            crate::ast::Expr::Index { object, index } => {
                self.lower_expr(object, None);
                self.lower_expr(index, None);
            }
            crate::ast::Expr::EnumVariant { fields, .. } => {
                for (_, value) in fields {
                    self.lower_expr(value, None);
                }
            }
            crate::ast::Expr::SelfAccess { field, field_span } => {
                if let Some(symbol) = self.current_agent_field(field) {
                    self.references.insert(
                        span_key(field_span),
                        Resolution {
                            symbol: Some(symbol),
                            kind: NameKind::Local,
                        },
                    );
                }
            }
            crate::ast::Expr::Integer(_)
            | crate::ast::Expr::Float(_)
            | crate::ast::Expr::Bool(_)
            | crate::ast::Expr::None_
            | crate::ast::Expr::SelfRef => {}
        }
    }

    fn classify_literal(
        &mut self,
        fields: &[(MapLitKey, SpannedExpr)],
        expected: Option<&TypeExpr>,
    ) -> LiteralKind {
        let has_int = fields
            .iter()
            .any(|(key, _)| matches!(key, MapLitKey::Int(_)));
        let has_bool = fields
            .iter()
            .any(|(key, _)| matches!(key, MapLitKey::Bool(_)));
        let has_str = fields
            .iter()
            .any(|(key, _)| matches!(key, MapLitKey::Ident(_) | MapLitKey::Str(_)));

        if (has_int && has_bool) || (has_str && (has_int || has_bool)) {
            return LiteralKind::InvalidMixedKeys;
        }
        if has_int {
            return LiteralKind::Map(MapKeyKind::Int);
        }
        if has_bool {
            return LiteralKind::Map(MapKeyKind::Bool);
        }
        if self
            .resolve_expected(expected)
            .is_some_and(|ty| matches!(ty, TypeExpr::Map(_, _)))
        {
            LiteralKind::Map(MapKeyKind::Str)
        } else {
            LiteralKind::Struct
        }
    }

    fn lower_call_args(&mut self, args: &[CallArg], params: Option<&[Param]>) {
        let expected = params
            .map(|params| call_arg_expected_types(params, args))
            .unwrap_or_else(|| vec![None; args.len()]);
        for (arg, expected) in args.iter().zip(expected.iter()) {
            self.lower_expr(&arg.value, expected.as_ref());
        }
    }

    fn literal_value_expected(
        &self,
        expected: Option<&TypeExpr>,
        key: &MapLitKey,
    ) -> Option<TypeExpr> {
        match self.resolve_expected(expected)? {
            TypeExpr::Map(_, value) => Some(*value),
            TypeExpr::Struct(fields) => key.as_str().and_then(|key| {
                fields
                    .iter()
                    .find(|field| field.name == key)
                    .map(|field| field.ty.kind.clone())
            }),
            TypeExpr::Named(name) => self.structs.get(&name).and_then(|fields| {
                key.as_str().and_then(|key| {
                    fields
                        .iter()
                        .find(|field| field.name == key)
                        .map(|field| field.ty.kind.clone())
                })
            }),
            _ => None,
        }
    }

    fn collection_item_expected(&self, expected: Option<&TypeExpr>) -> Option<TypeExpr> {
        match self.resolve_expected(expected)? {
            TypeExpr::List(item) | TypeExpr::Set(item) => Some(*item),
            _ => None,
        }
    }

    fn tuple_items_expected(&self, expected: Option<&TypeExpr>) -> Vec<TypeExpr> {
        match self.resolve_expected(expected) {
            Some(TypeExpr::Tuple(items)) => items,
            _ => Vec::new(),
        }
    }

    fn resolve_expected(&self, expected: Option<&TypeExpr>) -> Option<TypeExpr> {
        let mut ty = expected?.clone();
        let mut visited = HashSet::new();
        loop {
            match ty {
                TypeExpr::Named(ref name) if self.aliases.contains_key(name) => {
                    if !visited.insert(name.clone()) {
                        return None;
                    }
                    ty = self.aliases[name].clone();
                }
                _ => return Some(ty),
            }
        }
    }

    fn lower_reference(&mut self, name: &str, span: Span) -> Resolution {
        self.lower_reference_with_diagnostic(name, span, true)
    }

    fn lower_reference_without_diagnostic(&mut self, name: &str, span: Span) -> Resolution {
        self.lower_reference_with_diagnostic(name, span, false)
    }

    fn lower_reference_with_diagnostic(
        &mut self,
        name: &str,
        span: Span,
        emit_diagnostic: bool,
    ) -> Resolution {
        let resolution = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .map(|symbol| Resolution {
                symbol: Some(symbol),
                kind: NameKind::Local,
            })
            .unwrap_or_else(|| {
                self.globals
                    .get(name)
                    .copied()
                    .unwrap_or(Resolution::UNRESOLVED)
            });
        if emit_diagnostic && matches!(resolution.kind, NameKind::Unresolved) {
            self.diagnostics.push(TypeDiagnostic::UndefinedName {
                name: name.to_string(),
                span: span.clone(),
            });
        }
        self.references.insert(span_key(&span), resolution);
        resolution
    }

    fn define_binding(&mut self, binding: &Binding, span: Span) {
        match binding {
            Binding::Ident(name) => self.define_local(name, span),
            Binding::Destruct(DestructPat::Struct(fields)) => {
                for (_, name) in fields {
                    self.define_local(name, span.clone());
                }
            }
            Binding::Destruct(DestructPat::Tuple(names)) => {
                for name in names {
                    self.define_local(name, span.clone());
                }
            }
        }
    }

    fn add_binding(&mut self, binding: &Binding, span: Span) {
        match binding {
            Binding::Ident(name) => {
                self.add_symbol(name, SymbolKind::Binding, span);
            }
            Binding::Destruct(DestructPat::Struct(fields)) => {
                for (_, name) in fields {
                    self.add_symbol(name, SymbolKind::Binding, span.clone());
                }
            }
            Binding::Destruct(DestructPat::Tuple(names)) => {
                for name in names {
                    self.add_symbol(name, SymbolKind::Binding, span.clone());
                }
            }
        }
    }

    fn define_local(&mut self, name: &str, span: Span) {
        let id = self.add_symbol(name, SymbolKind::Binding, span);
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), id);
        }
    }

    fn add_symbol(&mut self, name: &str, kind: SymbolKind, span: Span) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol {
            id,
            name: name.to_string(),
            kind,
            span,
        });
        id
    }

    fn current_agent_task(&self, name: &str) -> Option<SymbolId> {
        self.current_agent
            .as_ref()
            .and_then(|agent| self.agent_tasks.get(agent))
            .and_then(|tasks| tasks.get(name))
            .copied()
    }

    fn current_agent_field(&self, name: &str) -> Option<SymbolId> {
        self.current_agent
            .as_ref()
            .and_then(|agent| self.agent_fields.get(agent))
            .and_then(|fields| fields.get(name))
            .copied()
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn call_arg_expected_types(params: &[Param], args: &[CallArg]) -> Vec<Option<TypeExpr>> {
    let mut expected = vec![None; args.len()];
    for (index, arg) in args.iter().enumerate() {
        if let Some(name) = arg.name.as_deref()
            && let Some(param) = params
                .iter()
                .find(|param| binding_ident(&param.name) == Some(name))
        {
            expected[index] = Some(param.ty.kind.clone());
        }
    }

    let mut positional = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| arg.name.is_none() && !arg.spread)
        .map(|(index, _)| index);
    for param in params {
        if binding_ident(&param.name)
            .is_some_and(|name| args.iter().any(|arg| arg.name.as_deref() == Some(name)))
        {
            continue;
        }
        if param.variadic {
            for index in positional.by_ref() {
                expected[index] = Some(param.ty.kind.clone());
            }
            break;
        }
        if let Some(index) = positional.next() {
            expected[index] = Some(param.ty.kind.clone());
        }
    }
    expected
}

fn binding_ident(binding: &Binding) -> Option<&str> {
    match binding {
        Binding::Ident(name) => Some(name),
        Binding::Destruct(_) => None,
    }
}

fn exprless_span() -> Span {
    0..0
}

#[cfg(test)]
mod tests {
    use miette::NamedSource;

    use super::*;

    fn lower(source: &str) -> Hir<'static> {
        let named = NamedSource::new("test.keel", source.to_string());
        let tokens = crate::lexer::lex(source, &named).expect("lex source");
        let program = crate::parser::parse(tokens, source.len(), &named).expect("parse source");
        lower_ast(Box::leak(Box::new(program)))
    }

    #[test]
    fn resolves_reference_to_top_task_symbol() {
        let source = "task greet() {}\ntask main() { greet() }\n";
        let hir = lower(source);
        let call_pos = source.rfind("greet").expect("greet call site");
        let call_span = call_pos..(call_pos + "greet".len());
        let call = hir.resolution_at(&call_span);
        assert_eq!(call.kind, NameKind::TopTask);
        let symbol = hir
            .symbol(call.symbol.expect("task symbol"))
            .expect("symbol");
        assert_eq!(symbol.name, "greet");
    }

    #[test]
    fn classifies_string_key_literal_as_map_when_annotation_requires_map() {
        let source = "task main() { values: map[str, int] = {one: 1} }\n";
        let hir = lower(source);
        let pos = source.find("{one: 1}").expect("literal span");
        let span = pos..(pos + "{one: 1}".len());
        assert_eq!(
            hir.literal_kinds.get(&span_key(&span)).copied(),
            Some(LiteralKind::Map(MapKeyKind::Str))
        );
    }

    #[test]
    fn classifies_out_of_order_named_arg_literals_against_matching_params() {
        let source = "type Record = { tag: int }\n\
task collect(record: Record, labels: map[str, int]) {}\n\
task main() { collect(labels: {one: 1}, record: {tag: 2}) }\n";
        let hir = lower(source);
        let kind_for = |literal: &str| {
            let pos = source.find(literal).expect("literal in source");
            hir.literal_kinds
                .get(&span_key(&(pos..(pos + literal.len()))))
                .copied()
        };

        assert_eq!(
            kind_for("{one: 1}"),
            Some(LiteralKind::Map(MapKeyKind::Str))
        );
        assert_eq!(kind_for("{tag: 2}"), Some(LiteralKind::Struct));
    }

    #[test]
    fn cyclic_alias_expected_type_does_not_resolve() {
        let named = NamedSource::new("test.keel", "type A = B\ntype B = A\n".to_string());
        let tokens = crate::lexer::lex(named.inner(), &named).expect("lex source");
        let program =
            crate::parser::parse(tokens, named.inner().len(), &named).expect("parse source");
        let mut lowerer = Lowerer::new(&program);
        lowerer.collect_globals();

        assert!(
            lowerer
                .resolve_expected(Some(&TypeExpr::Named("A".to_string())))
                .is_none()
        );
    }

    #[test]
    fn resolves_self_task_reference_to_agent_method_symbol() {
        let source = "agent Bot {\n task first() { self.second() }\n task second() {}\n}\n";
        let hir = lower(source);
        let field_pos = source.find("self.second").expect("self.second") + "self.".len();
        let field_span = field_pos..(field_pos + "second".len());
        let call = hir.resolution_at(&field_span);
        assert_eq!(call.kind, NameKind::Local);
        let symbol = hir
            .symbol(call.symbol.expect("method symbol"))
            .expect("symbol");
        assert_eq!(symbol.name, "second");
        assert_eq!(symbol.kind, SymbolKind::Method);
    }

    #[test]
    fn resolves_self_field_reference_to_state_symbol() {
        let source = "agent Bot {\n state { count: int = 0 }\n task read() { self.count }\n}\n";
        let hir = lower(source);
        let field_pos = source.find("self.count").expect("self.count") + "self.".len();
        let field_span = field_pos..(field_pos + "count".len());
        let field = hir.resolution_at(&field_span);
        assert_eq!(field.kind, NameKind::Local);
        let symbol = hir
            .symbol(field.symbol.expect("state symbol"))
            .expect("symbol");
        assert_eq!(symbol.name, "count");
        assert_eq!(symbol.kind, SymbolKind::Binding);
    }
}
