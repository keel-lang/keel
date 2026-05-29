//! Declaration-collection pass for the type checker.
//!
//! Walks all top-level declarations **before** the body-checking pass so that
//! every task, agent, type alias, struct, and enum is registered in the
//! checker's lookup tables regardless of source order.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::types::ty::Ty;

use super::{AgentInfo, Checker, TaskSig};

impl Checker {
    /// First-pass scan: register all declared types, tasks, agents, and
    /// interface implementations into the checker's lookup tables.
    ///
    /// Must be called before any call to [`Checker::check_task`] or
    /// [`Checker::infer_expr`] so that forward references resolve correctly.
    pub(crate) fn collect(&mut self, program: &Program) {
        // First pass: register all interface declarations so impl blocks can
        // reference them regardless of source order.
        const BUILTIN_IFACES: &[&str] = &[
            "Stringable",
            "Comparable",
            "Equatable",
            "Serializable",
            "Iterable",
        ];
        for node in &program.declarations {
            if let Decl::Interface(iface) = &node.kind {
                if BUILTIN_IFACES.contains(&iface.name.as_str()) {
                    self.err(format!(
                        "`{}` is a built-in interface and cannot be redeclared",
                        iface.name
                    ));
                    continue;
                }
                self.interfaces
                    .insert(iface.name.clone(), iface.methods.clone());
            }
        }

        for node in &program.declarations {
            match &node.kind {
                Decl::Type(t) => self.collect_type_decl(t),
                Decl::Task(t) => {
                    if !t.type_params.is_empty() {
                        self.generic_task_decls.insert(t.name.clone(), t.clone());
                    }
                    let sig = self.task_sig(t);
                    self.top_tasks.insert(t.name.clone(), sig);
                }
                Decl::Agent(a) => {
                    let info = self.agent_info(a);
                    self.agents.insert(a.name.clone(), info);
                }
                Decl::Impl(impl_decl) => {
                    self.check_impl_conformance(impl_decl);
                    if impl_decl.interface_name == "Iterable" {
                        self.iterable_types.insert(impl_decl.type_name.clone());
                    }
                }
                _ => {}
            }
        }
    }

    /// Register a single type declaration into the checker tables.
    ///
    /// Generic types are stored in `generic_decls` for deferred instantiation;
    /// concrete types are fully resolved and stored immediately.
    fn collect_type_decl(&mut self, t: &TypeDecl) {
        if !t.type_params.is_empty() {
            // Generic type — defer body resolution until instantiation.
            // For enum types, still register variant names for exhaustiveness checking.
            match &t.def {
                TypeDef::SimpleEnum(vs) => {
                    self.enum_variants.insert(t.name.clone(), vs.clone());
                }
                TypeDef::RichEnum(vs) => {
                    self.enum_variants
                        .insert(t.name.clone(), vs.iter().map(|v| v.name.clone()).collect());
                }
                _ => {}
            }
            self.generic_decls
                .insert(t.name.clone(), (t.type_params.clone(), t.def.clone()));
            return;
        }
        match &t.def {
            TypeDef::SimpleEnum(vs) => {
                self.enum_variants.insert(t.name.clone(), vs.clone());
            }
            TypeDef::RichEnum(vs) => {
                self.enum_variants
                    .insert(t.name.clone(), vs.iter().map(|v| v.name.clone()).collect());
            }
            TypeDef::Struct(fields) => {
                let mut f = Vec::with_capacity(fields.len());
                for field in fields {
                    let ty = self.resolve_and_check_type(&field.ty.kind);
                    f.push((field.name.clone(), ty));
                }
                self.structs.insert(t.name.clone(), f);
            }
            TypeDef::Alias(ty_node) => {
                let resolved = self.resolve_and_check_type(&ty_node.kind);
                self.aliases.insert(t.name.clone(), resolved);
            }
        }
    }

    /// Build the [`TaskSig`] for a task declaration.
    ///
    /// Variadic parameters store the element type (not `list[T]`) so that
    /// call-site arity checks compare each argument against `T` directly.
    fn task_sig(&self, t: &TaskDecl) -> TaskSig {
        let variadic = t.params.last().is_some_and(|p| p.variadic);
        let params = t
            .params
            .iter()
            .map(|p| {
                let name = match &p.name {
                    Binding::Ident(n) => n.clone(),
                    Binding::Destruct(_) => "_".to_string(),
                };
                // Variadic params are `list[T]` inside the body but `T` at call sites.
                // The sig stores the element type so call-site checks compare each arg to T.
                (name, self.resolve_type(&p.ty.kind))
            })
            .collect();
        let return_type = t
            .return_type
            .as_ref()
            .map(|ty| self.resolve_type(&ty.kind))
            .unwrap_or(Ty::None_);
        TaskSig {
            params,
            return_type,
            variadic,
        }
    }

    /// Summarise an agent declaration into the lightweight [`AgentInfo`] record
    /// used for scope checking within agent bodies.
    fn agent_info(&self, a: &AgentDecl) -> AgentInfo {
        let mut state_fields = HashMap::new();
        let mut readonly_fields = HashSet::new();
        let mut tasks = HashMap::new();
        let mut handlers = HashMap::new();
        for item in &a.items {
            match item {
                AgentItem::State(fields) => {
                    for f in fields {
                        state_fields.insert(f.name.clone(), self.resolve_type(&f.ty.kind));
                        if f.readonly {
                            readonly_fields.insert(f.name.clone());
                        }
                    }
                }
                AgentItem::Task(t) => {
                    tasks.insert(t.name.clone(), self.task_sig(t));
                }
                AgentItem::On(h) => {
                    let param_ty = h.param.as_ref().map(|p| self.resolve_type(&p.ty.kind));
                    handlers.insert(h.event.clone(), param_ty);
                }
                AgentItem::Attribute(_) => {}
            }
        }
        AgentInfo {
            state_fields,
            readonly_fields,
            tasks,
            handlers,
        }
    }
}
