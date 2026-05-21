use std::sync::Arc;

use miette::Result;

use crate::ast::{AgentItem, AttributeBody, Binding, Decl, Expr, TypeDef, TypeExpr};

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
                            .map(|f| (f.name.clone(), type_expr_to_string(&f.ty)))
                            .collect();
                        self.struct_types.insert(t.name.clone(), schema);
                    }
                    _ => {}
                }
                Ok(())
            }
            Decl::Interface(iface) => {
                const BUILTIN: &[&str] =
                    &["Stringable", "Comparable", "Equatable", "Serializable", "Iterable"];
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
                            // Return-type check.
                            let req_ret = sig
                                .return_type
                                .as_ref()
                                .map(type_expr_to_string)
                                .unwrap_or_else(|| "none".to_string());
                            let got_ret = got_method
                                .return_type
                                .as_ref()
                                .map(type_expr_to_string)
                                .unwrap_or_else(|| "none".to_string());
                            if !return_types_match(&req_ret, &got_ret) {
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
                    // Fix up __impl_self__ placeholder with the concrete type name.
                    let mut fixed = method.clone();
                    for param in &mut fixed.params {
                        if let Binding::Ident(n) = &param.name {
                            if n == "self" {
                                param.ty = TypeExpr::Named(type_name.clone());
                            }
                        }
                    }
                    self.impl_methods
                        .entry(type_name.clone())
                        .or_default()
                        .insert(method.name.clone(), fixed);
                }
                Ok(())
            }
            Decl::Task(t) => {
                self.globals.insert(
                    t.name.clone(),
                    Value::Task(t.name.clone(), Box::new(t.clone())),
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
                            AgentItem::Task(t) => Some(t.clone()),
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
                    if attr.name == "limits"
                        && let AttributeBody::Expr(Expr::StructLit(fields)) = &attr.body
                    {
                        for (key, _) in fields {
                            match key.as_str() {
                                "timeout" | "max_tokens" | "max_cost" => {}
                                other => {
                                    return Err(runtime_error(format!(
                                        "@limits: `{other}` is not supported in v0.1 — \
                                         supported fields: `timeout`, `max_tokens`, `max_cost`"
                                    )));
                                }
                            }
                        }
                    }
                }

                self.globals
                    .insert(a.name.clone(), Value::AgentRef(a.name.clone()));
                self.agents.insert(a.name.clone(), Arc::new(def));
                Ok(())
            }
            Decl::Stmt(_) => Ok(()), // executed in pass 2
        }
    }
}

fn type_expr_to_string(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", type_expr_to_string(inner)),
        TypeExpr::List(inner) => format!("[{}]", type_expr_to_string(inner)),
        TypeExpr::Map(k, v) => format!(
            "map[{}, {}]",
            type_expr_to_string(k),
            type_expr_to_string(v)
        ),
        TypeExpr::Set(inner) => format!("set[{}]", type_expr_to_string(inner)),
        TypeExpr::Tuple(items) => {
            let parts: Vec<_> = items.iter().map(type_expr_to_string).collect();
            format!("({})", parts.join(", "))
        }
        TypeExpr::Func(params, ret) => {
            let ps: Vec<_> = params.iter().map(type_expr_to_string).collect();
            format!("({}) -> {}", ps.join(", "), type_expr_to_string(ret))
        }
        TypeExpr::Generic(name, args) => {
            let as_: Vec<_> = args.iter().map(type_expr_to_string).collect();
            format!("{}[{}]", name, as_.join(", "))
        }
        TypeExpr::Struct(fields) => {
            let fs: Vec<_> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_expr_to_string(&f.ty)))
                .collect();
            format!("{{{}}}", fs.join(", "))
        }
        TypeExpr::Dynamic => "dynamic".to_string(),
    }
}

/// Return true when the required interface return type `req` is satisfied by
/// the concrete return type `got`.  Plain equality is the common case; the
/// extra arms handle built-in covariant wildcards used by Iterable et al.
fn return_types_match(req: &str, got: &str) -> bool {
    // Exact match.
    if req == got {
        return true;
    }
    // `dynamic` in the interface sig accepts anything.
    if req == "dynamic" {
        return true;
    }
    // `[dynamic]` (i.e. `list[dynamic]`) in the interface sig accepts any list.
    if req == "[dynamic]" && got.starts_with('[') {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        AgentDecl, AgentItem, AttributeBody, AttributeDecl, Decl, Expr, Field, InterfaceDecl,
        OnHandler, Param, StateField, TaskDecl, TypeDecl, TypeDef, TypeExpr, UseDecl, UseKind,
    };

    // ── helpers ──────────────────────────────────────────────────────────

    fn new_interp() -> Interpreter {
        Interpreter::new()
    }

    fn struct_lit(fields: Vec<(&str, Expr)>) -> Expr {
        Expr::StructLit(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    fn named_ty(name: &str) -> TypeExpr {
        TypeExpr::Named(name.to_string())
    }

    // ── register_decl: Type ─────────────────────────────────────────────

    #[test]
    fn register_simple_enum() {
        let mut interp = new_interp();
        let decl = Decl::Type(TypeDecl {
            name: "Urgency".into(),
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
            type_params: vec![],
            params: vec![],
            return_type: Some(named_ty("str")),
            body: vec![],
        };
        let decl = Decl::Task(task_decl.clone());
        interp.register_decl(&decl).unwrap();

        match interp.globals.get("do_thing") {
            Some(Value::Task(name, boxed)) => {
                assert_eq!(name, "do_thing");
                assert_eq!(boxed.name, "do_thing");
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
            items: vec![],
        });
        interp.register_decl(&decl).unwrap();

        // globals has AgentRef
        assert!(matches!(
            interp.globals.get("Bot"),
            Some(Value::AgentRef(n)) if n == "Bot"
        ));

        // agents map has the definition
        let def = interp.agents.get("Bot").unwrap();
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
            items: vec![
                AgentItem::Attribute(AttributeDecl {
                    name: "role".into(),
                    body: AttributeBody::Expr(Expr::StringLit(vec![
                        crate::ast::StringPart::Literal("assistant".into()),
                    ])),
                }),
                AgentItem::State(vec![StateField {
                    name: "counter".into(),
                    ty: named_ty("int"),
                    default: Expr::Integer(0),
                    readonly: false,
                }]),
                AgentItem::Task(TaskDecl {
                    name: "tick".into(),
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

        let def = interp.agents.get("FullBot").unwrap();
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
            items: vec![AgentItem::State(vec![
                StateField {
                    name: "a".into(),
                    ty: named_ty("str"),
                    default: Expr::StringLit(vec![crate::ast::StringPart::Literal("".into())]),
                    readonly: false,
                },
                StateField {
                    name: "b".into(),
                    ty: named_ty("int"),
                    default: Expr::Integer(1),
                    readonly: true,
                },
            ])],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.agents.get("StateBot").unwrap();
        assert_eq!(def.state_fields.len(), 2);
        assert!(def.tasks.is_empty());
        assert!(def.handlers.is_empty());
    }

    #[test]
    fn register_agent_with_only_tasks() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "TaskBot".into(),
            items: vec![
                AgentItem::Task(TaskDecl {
                    name: "a".into(),
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    body: vec![],
                }),
                AgentItem::Task(TaskDecl {
                    name: "b".into(),
                    type_params: vec![],
                    params: vec![],
                    return_type: None,
                    body: vec![],
                }),
            ],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.agents.get("TaskBot").unwrap();
        assert_eq!(def.tasks.len(), 2);
        assert!(def.state_fields.is_empty());
        assert!(def.handlers.is_empty());
    }

    #[test]
    fn register_agent_with_only_handlers() {
        let mut interp = new_interp();
        let decl = Decl::Agent(AgentDecl {
            name: "HandlerBot".into(),
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
                        ty: named_ty("int"),
                        default: None,
                        variadic: false,
                    }),
                    body: vec![],
                }),
            ],
        });
        interp.register_decl(&decl).unwrap();

        let def = interp.agents.get("HandlerBot").unwrap();
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
            items: vec![AgentItem::Attribute(AttributeDecl {
                name: "limits".into(),
                body: AttributeBody::Expr(Expr::Integer(42)),
            })],
        });
        assert!(interp.register_decl(&decl).is_ok());
    }

    // ── register_decl: Stmt ─────────────────────────────────────────────

    #[test]
    fn register_stmt_is_noop() {
        let mut interp = new_interp();
        let decl = Decl::Stmt((crate::ast::Stmt::Expr(Expr::Integer(1)), 0..1));
        let prev_count = interp.globals.len();
        interp.register_decl(&decl).unwrap();
        assert_eq!(interp.globals.len(), prev_count);
    }

    // ── type_expr_to_string ─────────────────────────────────────────────

    #[test]
    fn type_named() {
        assert_eq!(type_expr_to_string(&named_ty("str")), "str");
        assert_eq!(type_expr_to_string(&named_ty("Urgency")), "Urgency");
    }

    #[test]
    fn type_nullable() {
        let te = TypeExpr::Nullable(Box::new(named_ty("str")));
        assert_eq!(type_expr_to_string(&te), "str?");
    }

    #[test]
    fn type_list() {
        let te = TypeExpr::List(Box::new(named_ty("int")));
        assert_eq!(type_expr_to_string(&te), "[int]");
    }

    #[test]
    fn type_map() {
        let te = TypeExpr::Map(Box::new(named_ty("str")), Box::new(named_ty("int")));
        assert_eq!(type_expr_to_string(&te), "map[str, int]");
    }

    #[test]
    fn type_set() {
        let te = TypeExpr::Set(Box::new(named_ty("str")));
        assert_eq!(type_expr_to_string(&te), "set[str]");
    }

    #[test]
    fn type_tuple() {
        let te = TypeExpr::Tuple(vec![named_ty("str"), named_ty("int"), named_ty("bool")]);
        assert_eq!(type_expr_to_string(&te), "(str, int, bool)");
    }

    #[test]
    fn type_tuple_single_element() {
        let te = TypeExpr::Tuple(vec![named_ty("str")]);
        assert_eq!(type_expr_to_string(&te), "(str)");
    }

    #[test]
    fn type_func() {
        let te = TypeExpr::Func(
            vec![named_ty("str"), named_ty("int")],
            Box::new(named_ty("bool")),
        );
        assert_eq!(type_expr_to_string(&te), "(str, int) -> bool");
    }

    #[test]
    fn type_func_no_params() {
        let te = TypeExpr::Func(vec![], Box::new(named_ty("str")));
        assert_eq!(type_expr_to_string(&te), "() -> str");
    }

    #[test]
    fn type_generic() {
        let te = TypeExpr::Generic("Result".into(), vec![named_ty("str"), named_ty("int")]);
        assert_eq!(type_expr_to_string(&te), "Result[str, int]");
    }

    #[test]
    fn type_generic_no_args() {
        let te = TypeExpr::Generic("List".into(), vec![]);
        assert_eq!(type_expr_to_string(&te), "List[]");
    }

    #[test]
    fn type_struct() {
        let te = TypeExpr::Struct(vec![
            Field {
                name: "body".into(),
                ty: named_ty("str"),
            },
            Field {
                name: "from".into(),
                ty: named_ty("str"),
            },
        ]);
        assert_eq!(type_expr_to_string(&te), "{body: str, from: str}");
    }

    #[test]
    fn type_struct_empty() {
        let te = TypeExpr::Struct(vec![]);
        assert_eq!(type_expr_to_string(&te), "{}");
    }

    #[test]
    fn type_dynamic() {
        assert_eq!(type_expr_to_string(&TypeExpr::Dynamic), "dynamic");
    }

    // ── type_expr_to_string: nested/composed ────────────────────────────

    #[test]
    fn type_nested_nullable_list() {
        let te = TypeExpr::List(Box::new(TypeExpr::Nullable(Box::new(named_ty("str")))));
        assert_eq!(type_expr_to_string(&te), "[str?]");
    }

    #[test]
    fn type_nested_map_of_lists() {
        let te = TypeExpr::Map(
            Box::new(named_ty("str")),
            Box::new(TypeExpr::List(Box::new(named_ty("int")))),
        );
        assert_eq!(type_expr_to_string(&te), "map[str, [int]]");
    }

    #[test]
    fn type_nested_func_returning_func() {
        let ret = TypeExpr::Func(vec![named_ty("int")], Box::new(named_ty("bool")));
        let te = TypeExpr::Func(vec![named_ty("str")], Box::new(ret));
        assert_eq!(type_expr_to_string(&te), "(str) -> (int) -> bool");
    }
}
