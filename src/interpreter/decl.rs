use std::sync::Arc;

use miette::Result;

use crate::ast::{AgentItem, AttributeBody, Decl, Expr, TypeDef, TypeExpr};

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
            Decl::Interface(_) | Decl::Extern(_) | Decl::Use(_) => Ok(()),
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
