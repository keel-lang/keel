use std::sync::Arc;

use miette::Result;

use crate::ast::{AgentItem, AttributeBody, Binding, Decl, Expr, TypeDef, TypeExpr};
use crate::types::interface::{self as iface, Signature};

use super::runtime_error;
use super::state::{AgentDef, Interpreter};
use super::value::Value;

impl Interpreter {
    pub fn register_decl(&mut self, decl: &Decl) -> Result<()> {
        match decl {
            Decl::Type(t) => {
                // Bind the type name as a Namespace-like value so that
                // `Mood.neutral` resolves and `as: Mood` finds a
                // defined identifier. For simple enums, also cache
                // the variant list for Ai.classify.
                self.globals
                    .insert(t.name.clone(), Value::Namespace(t.name.clone()));
                match &t.def {
                    TypeDef::SimpleEnum(variants) => {
                        self.enum_types.insert(t.name.clone(), variants.clone());
                    }
                    TypeDef::Struct(fields) => {
                        let schema = fields
                            .iter()
                            .map(|f| (f.name.clone(), type_display_str(&f.ty.kind)))
                            .collect();
                        self.struct_types.insert(t.name.clone(), schema);
                    }
                    TypeDef::Alias(ty_node) => {
                        // If the alias names a struct, remember its canonical
                        // runtime tag so `x: Alias = { ... }` dispatches as T.
                        if let TypeExpr::Named(target) = &ty_node.kind
                            && let Some(canonical) = self
                                .struct_aliases
                                .get(target.as_str())
                                .cloned()
                                .or_else(|| {
                                    self.struct_types
                                        .contains_key(target.as_str())
                                        .then(|| target.clone())
                                })
                        {
                            self.struct_aliases.insert(t.name.clone(), canonical);
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
            Decl::Interface(iface) => {
                const BUILTIN: &[&str] = &[
                    "Stringable",
                    "Comparable",
                    "Equatable",
                    "Serializable",
                    "Iterable",
                ];
                if BUILTIN.contains(&iface.name.as_str()) {
                    return Err(runtime_error(format!(
                        "`{}` is a built-in interface and cannot be redeclared",
                        iface.name
                    )));
                }
                self.interfaces
                    .insert(iface.name.clone(), iface.methods.clone());
                Ok(())
            }
            Decl::Extern(_) | Decl::Use(_) => Ok(()),
            Decl::Impl(impl_decl) => {
                let type_name = &impl_decl.type_name;
                let iface_name = &impl_decl.interface_name;

                // Validate against the known interface definition.
                let required = self.interfaces.get(iface_name).cloned();
                match required {
                    None => {
                        return Err(runtime_error(format!(
                            "impl: unknown interface `{iface_name}` — declare it with `interface {iface_name} {{ ... }}`"
                        )));
                    }
                    Some(sigs) => {
                        let provided: std::collections::HashSet<&str> =
                            impl_decl.methods.iter().map(|m| m.name.as_str()).collect();
                        for sig in &sigs {
                            if !provided.contains(sig.name.as_str()) {
                                return Err(runtime_error(format!(
                                    "impl `{iface_name}` for `{type_name}` is missing required method `{}`",
                                    sig.name
                                )));
                            }
                            // Arity check (excluding the `self` param).
                            let req_params: Vec<_> = sig
                                .params
                                .iter()
                                .filter(|p| {
                                    !matches!(&p.name, crate::ast::Binding::Ident(n) if n == "self")
                                })
                                .collect();
                            let got_method = impl_decl
                                .methods
                                .iter()
                                .find(|m| m.name == sig.name)
                                .unwrap();
                            let got_params: Vec<_> = got_method
                                .params
                                .iter()
                                .filter(|p| {
                                    !matches!(&p.name, crate::ast::Binding::Ident(n) if n == "self")
                                })
                                .collect();
                            if req_params.len() != got_params.len() {
                                return Err(runtime_error(format!(
                                    "impl `{iface_name}` for `{type_name}`: method `{}` expects {} parameter(s) but got {}",
                                    sig.name,
                                    req_params.len(),
                                    got_params.len()
                                )));
                            }
                            // Return-type check — use the shared typed
                            // conformance function so the runtime and the
                            // static checker always agree.
                            let env = &self.type_env;
                            let req_sig = Signature {
                                params: vec![],
                                ret: sig
                                    .return_type
                                    .as_ref()
                                    .map(|n| iface::resolve_type_expr(&n.kind, env))
                                    .unwrap_or(crate::types::checker::Ty::None_),
                            };
                            let got_sig = Signature {
                                params: vec![],
                                ret: got_method
                                    .return_type
                                    .as_ref()
                                    .map(|n| iface::resolve_type_expr(&n.kind, env))
                                    .unwrap_or(crate::types::checker::Ty::None_),
                            };
                            if !iface::signature_satisfies(&req_sig, &got_sig) {
                                // Re-derive display strings for the error message.
                                let req_ret = sig
                                    .return_type
                                    .as_ref()
                                    .map(|n| type_display_str(&n.kind))
                                    .unwrap_or_else(|| "none".to_string());
                                let got_ret = got_method
                                    .return_type
                                    .as_ref()
                                    .map(|n| type_display_str(&n.kind))
                                    .unwrap_or_else(|| "none".to_string());
                                return Err(runtime_error(format!(
                                    "impl `{iface_name}` for `{type_name}`: method `{}` must return `{req_ret}` but returns `{got_ret}`",
                                    sig.name
                                )));
                            }
                        }
                        // Reject extra methods not in the interface.
                        for method in &impl_decl.methods {
                            if !sigs.iter().any(|s| s.name == method.name) {
                                return Err(runtime_error(format!(
                                    "impl `{iface_name}` for `{type_name}`: method `{}` is not part of interface `{iface_name}`",
                                    method.name
                                )));
                            }
                        }
                    }
                }

                for method in &impl_decl.methods {
                    // Replace the SelfType receiver with the concrete implementing type.
                    let mut fixed = method.clone();
                    for param in &mut fixed.params {
                        if let Binding::Ident(n) = &param.name
                            && n == "self"
                        {
                            // Preserve the original span (0..0 for synthetic, or the
                            // `self` keyword span for parsed methods).
                            param.ty.kind = TypeExpr::Named(type_name.clone());
                        }
                    }
                    self.store
                        .impl_methods
                        .entry(type_name.clone())
                        .or_default()
                        .insert(method.name.clone(), Arc::new(fixed));
                }
                Ok(())
            }
            Decl::Task(t) => {
                self.globals.insert(
                    t.name.clone(),
                    Value::Task(t.name.clone(), Arc::new(t.clone())),
                );
                Ok(())
            }
            Decl::Agent(a) => {
                let def = AgentDef {
                    name: a.name.clone(),
                    attributes: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::Attribute(attr) => Some(attr.clone()),
                            _ => None,
                        })
                        .collect(),
                    state_fields: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::State(fields) => Some(fields.clone()),
                            _ => None,
                        })
                        .flatten()
                        .collect(),
                    tasks: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::Task(t) => Some(Arc::new(t.clone())),
                            _ => None,
                        })
                        .collect(),
                    handlers: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::On(h) => Some(h.clone()),
                            _ => None,
                        })
                        .collect(),
                };
                // Validate @limits fields — unimplemented ones are an error.
                for attr in &def.attributes {
                    if attr.name == "limits" {
                        let check_field = |key: &str| -> bool {
                            matches!(key, "timeout" | "max_tokens" | "max_cost")
                        };
                        let unknown_key: Option<String> = match &attr.body {
                            AttributeBody::Expr(node)
                                if matches!(&node.kind, Expr::StructLit(_)) =>
                            {
                                let Expr::StructLit(f) = &node.kind else {
                                    unreachable!()
                                };
                                f.iter()
                                    .filter_map(|(k, _)| k.as_str())
                                    .find(|k| !check_field(k))
                                    .map(|k| k.to_string())
                            }
                            AttributeBody::Expr(node)
                                if matches!(&node.kind, Expr::StructSpreadUpdate { .. }) =>
                            {
                                let Expr::StructSpreadUpdate { overrides, .. } = &node.kind else {
                                    unreachable!()
                                };
                                overrides
                                    .iter()
                                    .map(|(k, _)| k.as_str())
                                    .find(|k| !check_field(k))
                                    .map(|k| k.to_string())
                            }
                            _ => continue,
                        };
                        if let Some(key) = unknown_key {
                            return Err(runtime_error(format!(
                                "@limits: `{key}` is not supported in v0.1 — \
                                 supported fields: `timeout`, `max_tokens`, `max_cost`"
                            )));
                        }
                    }
                }

                self.globals
                    .insert(a.name.clone(), Value::AgentRef(a.name.clone()));
                self.store.agents.insert(a.name.clone(), Arc::new(def));
                Ok(())
            }
            Decl::Test(_) | Decl::Stmt(_) => Ok(()), // executed by their dedicated pass
        }
    }
}

/// Produce a human-readable display string for a [`TypeExpr`] — used for
/// `struct_types` field schema storage and for error messages.  The conformance
/// decision itself is made by [`crate::types::interface::signature_satisfies`],
/// not by comparing these strings.
fn type_display_str(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", type_display_str(inner)),
        TypeExpr::List(inner) => format!("[{}]", type_display_str(inner)),
        TypeExpr::Map(k, v) => format!("map[{}, {}]", type_display_str(k), type_display_str(v)),
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
    use super::*;
    use crate::ast::{
        AgentDecl, AgentItem, AttributeBody, AttributeDecl, Decl, Expr, Field, InterfaceDecl, Node,
        OnHandler, Param, SpannedExpr, StateField, TaskDecl, TypeDecl, TypeDef, TypeExpr, UseDecl,
        UseKind,
    };

    // ── helpers ──────────────────────────────────────────────────────────

    fn new_interp() -> Interpreter {
        Interpreter::new()
    }

    fn struct_lit(fields: Vec<(&str, Expr)>) -> SpannedExpr {
        Node::synthetic(Expr::StructLit(
            fields
                .into_iter()
                .map(|(k, v)| {
                    (
                        crate::ast::MapLitKey::Ident(k.to_string()),
                        Node::synthetic(v),
                    )
                })
                .collect(),
        ))
    }

    /// Construct a synthetic named-type annotation with a 0..0 sentinel span.
    /// Use for AST node fields that expect `Node<TypeExpr>` (e.g. `Field.ty`, `Param.ty`).
    fn named_ty(name: &str) -> Node<TypeExpr> {
        Node::synthetic(TypeExpr::Named(name.to_string()))
    }

    /// Construct a bare `TypeExpr::Named` for use inside `TypeExpr::*` constructors
    /// that take plain `TypeExpr` children (e.g. `Nullable`, `List`, `Map`).
    fn te(name: &str) -> TypeExpr {
        TypeExpr::Named(name.to_string())
    }

    // ── register_decl: Type ─────────────────────────────────────────────

    #[test]
    fn register_simple_enum() {
        let mut interp = new_interp();
        let decl = Decl::Type(TypeDecl {
            name: "Urgency".into(),
            name_span: 0..0,
            type_params: vec![],
            def: TypeDef::SimpleEnum(vec!["low".into(), "medium".into(), "high".into()]),
        });
        interp.register_decl(&decl).unwrap();

        // globals has a Namespace entry
        assert!(matches!(
            interp.globals.get("Urgency"),
            Some(Value::Namespace(n)) if n == "Urgency"
        ));

        // enum_types populated
        let variants = interp.enum_types.get("Urgency").unwrap();
        assert_eq!(variants, &vec!["low", "medium", "high"]);
    }

    #[test]
    fn register_struct_type() {
        let mut interp = new_interp();
        let decl = Decl::Type(TypeDecl {
            name: "EmailInfo".into(),
            name_span: 0..0,
            type_params: vec![],
            def: TypeDef::Struct(vec![
                Field {
                    name: "sender".into(),
                    ty: named_ty("str"),
                },
                Field {
                    name: "subject".into(),
                    ty: named_ty("str"),
                },
            ]),
        });
        interp.register_decl(&decl).unwrap();

        assert!(matches!(
            interp.globals.get("EmailInfo"),
            Some(Value::Namespace(n)) if n == "EmailInfo"
        ));

        let schema = interp.struct_types.get("EmailInfo").unwrap();
        assert_eq!(
            schema,
            &vec![
                ("sender".into(), "str".into()),
                ("subject".into(), "str".into()),
            ]
        );
    }

    #[test]
    fn register_type_alias() {
        let mut interp = new_interp();
        let decl = Decl::Type(TypeDecl {
            name: "Timestamp".into(),
            name_span: 0..0,
            type_params: vec![],
            def: TypeDef::Alias(named_ty("datetime")),
        });
        interp.register_decl(&decl).unwrap();

        assert!(matches!(
            interp.globals.get("Timestamp"),
            Some(Value::Namespace(n)) if n == "Timestamp"
        ));
        // enum_types and struct_types should NOT be populated
        assert!(interp.enum_types.is_empty());
        assert!(interp.struct_types.is_empty());
    }

    #[test]
    fn register_rich_enum() {
        let mut interp = new_interp();
        let decl = Decl::Type(TypeDecl {
            name: "Action".into(),
            name_span: 0..0,
            type_params: vec![],
            def: TypeDef::RichEnum(vec![]), // empty rich enum
        });
        interp.register_decl(&decl).unwrap();

        assert!(matches!(
            interp.globals.get("Action"),
            Some(Value::Namespace(n)) if n == "Action"
        ));
        assert!(interp.enum_types.is_empty());
        assert!(interp.struct_types.is_empty());
    }

    // ── register_decl: Interface, Extern, Use ───────────────────────────

    #[test]
    fn register_interface_is_noop() {
        let mut interp = new_interp();
        let decl = Decl::Interface(InterfaceDecl {
            name: "MyInterface".into(),
            name_span: 0..0,
            methods: vec![],
        });
        let prev_count = interp.globals.len();
        interp.register_decl(&decl).unwrap();
        assert_eq!(interp.globals.len(), prev_count);
    }

    #[test]
    fn register_extern_is_noop() {
        let mut interp = new_interp();
        let decl = Decl::Extern(crate::ast::ExternDecl {
            name: "my_extern".into(),
            name_span: 0..0,
            params: vec![],
            return_type: named_ty("str"),
            source: "python".into(),
        });
        let prev_count = interp.globals.len();
        interp.register_decl(&decl).unwrap();
        assert_eq!(interp.globals.len(), prev_count);
    }

    #[test]
    fn register_use_is_noop() {
        let mut interp = new_interp();
        let decl = Decl::Use(UseDecl {
            kind: UseKind::File("./lib.keel".into()),
        });
        let prev_count = interp.globals.len();
        interp.register_decl(&decl).unwrap();
        assert_eq!(interp.globals.len(), prev_count);
    }

    // ── register_decl: Task ─────────────────────────────────────────────

    #[test]
    fn register_task() {
        let mut interp = new_interp();
        let task_decl = TaskDecl {
            name: "do_thing".into(),
            name_span: 0..0,
            type_params: vec![],
            params: vec![],
            return_type: Some(named_ty("str")),
            body: vec![],
        };
        let decl = Decl::Task(task_decl.clone());
        interp.register_decl(&decl).unwrap();

        match interp.globals.get("do_thing") {
            Some(Value::Task(name, decl)) => {
                assert_eq!(name, "do_thing");
                assert_eq!(decl.name, "do_thing");
            }
            other => panic!("expected Value::Task, got {other:?}"),
        }
    }

    // ── register_decl: Agent ───────────────────────────────────────────

    #[test]
    fn register_agent_minimal() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![],
        });
        interp.register_decl(&decl).unwrap();

        // globals has AgentRef
        assert!(matches!(
            interp.globals.get("Bot"),
            Some(Value::AgentRef(n)) if n == "Bot"
        ));

        // agents map has the definition
        let def = interp.store.agents.get("Bot").unwrap();
        assert_eq!(def.name, "Bot");
        assert!(def.attributes.is_empty());
        assert!(def.state_fields.is_empty());
        assert!(def.tasks.is_empty());
        assert!(def.handlers.is_empty());
    }

    #[test]
    fn register_agent_with_all_items() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "FullBot".into(),
            name_span: 0..0,
            items: vec![
                AgentItem::Attribute(AttributeDecl {
                    name: "role".into(),
                    body: AttributeBody::Expr(Node::synthetic(Expr::StringLit(vec![
                        crate::ast::StringPart::Literal("assistant".into()),
                    ]))),
                }),
                AgentItem::State(vec![StateField {
                    name: "counter".into(),
                    name_span: 0..0,
                    ty: named_ty("int"),
                    default: Node::synthetic(Expr::Integer(0)),
                    readonly: false,
                }]),
                AgentItem::Task(TaskDecl {
                    name: "tick".into(),
                    name_span: 0..0,
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    body: vec![],
                }),
                AgentItem::On(OnHandler {
                    event: "greet".into(),
                    param: None,
                    body: vec![],
                }),
            ],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.store.agents.get("FullBot").unwrap();
        assert_eq!(def.attributes.len(), 1);
        assert_eq!(def.attributes[0].name, "role");
        assert_eq!(def.state_fields.len(), 1);
        assert_eq!(def.state_fields[0].name, "counter");
        assert_eq!(def.tasks.len(), 1);
        assert_eq!(def.tasks[0].name, "tick");
        assert_eq!(def.handlers.len(), 1);
        assert_eq!(def.handlers[0].event, "greet");
    }

    #[test]
    fn register_agent_with_only_state() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "StateBot".into(),
            name_span: 0..0,
            items: vec![AgentItem::State(vec![
                StateField {
                    name: "a".into(),
                    name_span: 0..0,
                    ty: named_ty("str"),
                    default: Node::synthetic(Expr::StringLit(vec![
                        crate::ast::StringPart::Literal("".into()),
                    ])),
                    readonly: false,
                },
                StateField {
                    name: "b".into(),
                    name_span: 0..0,
                    ty: named_ty("int"),
                    default: Node::synthetic(Expr::Integer(1)),
                    readonly: true,
                },
            ])],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.store.agents.get("StateBot").unwrap();
        assert_eq!(def.state_fields.len(), 2);
        assert!(def.tasks.is_empty());
        assert!(def.handlers.is_empty());
    }

    #[test]
    fn register_agent_with_only_tasks() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "TaskBot".into(),
            name_span: 0..0,
            items: vec![
                AgentItem::Task(TaskDecl {
                    name: "a".into(),
                    name_span: 0..0,
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    body: vec![],
                }),
                AgentItem::Task(TaskDecl {
                    name: "b".into(),
                    name_span: 0..0,
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    body: vec![],
                }),
            ],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.store.agents.get("TaskBot").unwrap();
        assert_eq!(def.tasks.len(), 2);
        assert!(def.state_fields.is_empty());
        assert!(def.handlers.is_empty());
    }

    #[test]
    fn register_agent_with_only_handlers() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "HandlerBot".into(),
            name_span: 0..0,
            items: vec![
                AgentItem::On(OnHandler {
                    event: "msg".into(),
                    param: None,
                    body: vec![],
                }),
                AgentItem::On(OnHandler {
                    event: "tick".into(),
                    param: Some(Param {
                        name: crate::ast::Binding::Ident("x".into()),
                        name_span: 0..0,
                        ty: named_ty("int"),
                        default: None,
                        variadic: false,
                    }),
                    body: vec![],
                }),
            ],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.store.agents.get("HandlerBot").unwrap();
        assert_eq!(def.handlers.len(), 2);
        assert!(def.tasks.is_empty());
        assert!(def.state_fields.is_empty());
    }

    // ── register_decl: Agent @limits validation ────────────────────────

    #[test]
    fn agent_limits_supported_fields_ok() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(struct_lit(vec![
                    ("timeout", Expr::Integer(30)),
                    ("max_tokens", Expr::Integer(4096)),
                    ("max_cost", Expr::Float(0.05)),
                ])),
            })],
        });
        assert!(interp.register_decl(&decl).is_ok());
    }

    #[test]
    fn agent_limits_unsupported_field_is_error() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(struct_lit(vec![
                    ("timeout", Expr::Integer(30)),
                    ("retry_count", Expr::Integer(3)), // unsupported
                ])),
            })],
        });
        let err = interp.register_decl(&decl).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("retry_count"),
            "expected error mentioning retry_count, got: {msg}"
        );
    }

    #[test]
    fn agent_limits_empty_is_ok() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(struct_lit(vec![])),
            })],
        });
        assert!(interp.register_decl(&decl).is_ok());
    }

    #[test]
    fn agent_non_limits_attribute_is_not_validated() {
        // @model with a struct body should not trigger limit validation
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "model".into(),
                body: AttributeBody::Expr(struct_lit(vec![(
                    "bogus_field",
                    Expr::StringLit(vec![crate::ast::StringPart::Literal("x".into())]),
                )])),
            })],
        });
        assert!(interp.register_decl(&decl).is_ok());
    }

    #[test]
    fn agent_limits_not_a_struct_lit_is_ok() {
        // @limits with a non-StructLit expression should not crash
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(Node::synthetic(Expr::Integer(42))),
            })],
        });
        assert!(interp.register_decl(&decl).is_ok());
    }

    #[test]
    fn agent_limits_spread_update_supported_overrides_ok() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(Node::synthetic(Expr::StructSpreadUpdate {
                    base: Box::new(Node::synthetic(Expr::Ident("base_limits".into()))),
                    overrides: vec![("max_tokens".into(), Node::synthetic(Expr::Integer(2048)))],
                })),
            })],
        });
        assert!(interp.register_decl(&decl).is_ok());
    }

    #[test]
    fn agent_limits_spread_update_unsupported_override_is_error() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "Bot".into(),
            name_span: 0..0,
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(Node::synthetic(Expr::StructSpreadUpdate {
                    base: Box::new(Node::synthetic(Expr::Ident("base_limits".into()))),
                    overrides: vec![("retry_count".into(), Node::synthetic(Expr::Integer(3)))],
                })),
            })],
        });
        let err = interp.register_decl(&decl).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("retry_count"),
            "expected error mentioning retry_count, got: {msg}"
        );
    }

    // ── register_decl: Stmt ─────────────────────────────────────────────

    #[test]
    fn register_stmt_is_noop() {
        let mut interp = new_interp();
        let decl = Decl::Stmt(Node::new(
            crate::ast::Stmt::Expr(Node::synthetic(Expr::Integer(1))),
            0..1,
        ));
        let prev_count = interp.globals.len();
        interp.register_decl(&decl).unwrap();
        assert_eq!(interp.globals.len(), prev_count);
    }

    // ── type_display_str ─────────────────────────────────────────────

    #[test]
    fn type_named() {
        assert_eq!(type_display_str(&te("str")), "str");
        assert_eq!(type_display_str(&te("Urgency")), "Urgency");
    }

    #[test]
    fn type_nullable() {
        let ty = TypeExpr::Nullable(Box::new(te("str")));
        assert_eq!(type_display_str(&ty), "str?");
    }

    #[test]
    fn type_list() {
        let ty = TypeExpr::List(Box::new(te("int")));
        assert_eq!(type_display_str(&ty), "[int]");
    }

    #[test]
    fn type_map() {
        let ty = TypeExpr::Map(Box::new(te("str")), Box::new(te("int")));
        assert_eq!(type_display_str(&ty), "map[str, int]");
    }

    #[test]
    fn type_set() {
        let ty = TypeExpr::Set(Box::new(te("str")));
        assert_eq!(type_display_str(&ty), "set[str]");
    }

    #[test]
    fn type_tuple() {
        let ty = TypeExpr::Tuple(vec![te("str"), te("int"), te("bool")]);
        assert_eq!(type_display_str(&ty), "(str, int, bool)");
    }

    #[test]
    fn type_tuple_single_element() {
        let ty = TypeExpr::Tuple(vec![te("str")]);
        assert_eq!(type_display_str(&ty), "(str)");
    }

    #[test]
    fn type_func() {
        let ty = TypeExpr::Func(vec![te("str"), te("int")], Box::new(te("bool")));
        assert_eq!(type_display_str(&ty), "(str, int) -> bool");
    }

    #[test]
    fn type_func_no_params() {
        let ty = TypeExpr::Func(vec![], Box::new(te("str")));
        assert_eq!(type_display_str(&ty), "() -> str");
    }

    #[test]
    fn type_generic() {
        let ty = TypeExpr::Generic("Result".into(), vec![te("str"), te("int")]);
        assert_eq!(type_display_str(&ty), "Result[str, int]");
    }

    #[test]
    fn type_generic_no_args() {
        let ty = TypeExpr::Generic("List".into(), vec![]);
        assert_eq!(type_display_str(&ty), "List[]");
    }

    #[test]
    fn type_struct() {
        let ty = TypeExpr::Struct(vec![
            Field {
                name: "body".into(),
                ty: named_ty("str"),
            },
            Field {
                name: "from".into(),
                ty: named_ty("str"),
            },
        ]);
        assert_eq!(type_display_str(&ty), "{body: str, from: str}");
    }

    #[test]
    fn type_struct_empty() {
        let ty = TypeExpr::Struct(vec![]);
        assert_eq!(type_display_str(&ty), "{}");
    }

    #[test]
    fn type_dynamic() {
        assert_eq!(type_display_str(&TypeExpr::Dynamic), "dynamic");
    }

    // ── type_display_str: nested/composed ────────────────────────────

    #[test]
    fn type_nested_nullable_list() {
        let ty = TypeExpr::List(Box::new(TypeExpr::Nullable(Box::new(te("str")))));
        assert_eq!(type_display_str(&ty), "[str?]");
    }

    #[test]
    fn type_nested_map_of_lists() {
        let ty = TypeExpr::Map(
            Box::new(te("str")),
            Box::new(TypeExpr::List(Box::new(te("int")))),
        );
        assert_eq!(type_display_str(&ty), "map[str, [int]]");
    }

    #[test]
    fn type_nested_func_returning_func() {
        let ret = TypeExpr::Func(vec![te("int")], Box::new(te("bool")));
        let ty = TypeExpr::Func(vec![te("str")], Box::new(ret));
        assert_eq!(type_display_str(&ty), "(str) -> (int) -> bool");
    }
}
