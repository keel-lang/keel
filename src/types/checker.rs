use std::collections::HashMap;

use crate::ast::*;
use super::{Type, TypeEnv};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TypeError {
    pub message: String,
    pub span: Option<crate::lexer::Span>,
}

impl TypeError {
    fn new(msg: impl Into<String>) -> Self {
        TypeError {
            message: msg.into(),
            span: None,
        }
    }

    #[allow(dead_code)]
    fn with_span(mut self, span: crate::lexer::Span) -> Self {
        self.span = Some(span);
        self
    }
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

type TResult = Result<Type, TypeError>;

fn type_err(msg: impl Into<String>) -> TypeError {
    TypeError::new(msg)
}

// ---------------------------------------------------------------------------
// Checker state
// ---------------------------------------------------------------------------

pub struct TypeChecker {
    env: TypeEnv,
    /// Named type definitions (enums, structs)
    type_defs: HashMap<String, Type>,
    /// Task signatures: name → (param types, return type)
    task_sigs: HashMap<String, (Vec<(String, Type)>, Type)>,
    /// Connection names (for fetch source resolution)
    connections: Vec<String>,
    /// Current agent state field types
    agent_state: HashMap<String, Type>,
    /// Whether we're inside an agent body
    in_agent: bool,
    /// Collected errors
    pub errors: Vec<TypeError>,
    /// Current statement span (for error attribution)
    current_span: Option<crate::lexer::Span>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: TypeEnv::new(),
            type_defs: HashMap::new(),
            task_sigs: HashMap::new(),
            connections: Vec::new(),
            agent_state: HashMap::new(),
            in_agent: false,
            errors: Vec::new(),
            current_span: None,
        }
    }

    fn error(&mut self, msg: impl Into<String>) {
        let mut err = type_err(msg);
        err.span = self.current_span.clone();
        self.errors.push(err);
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn check(program: &Program) -> Vec<TypeError> {
    let mut checker = TypeChecker::new();

    // Pass 1: register all type definitions and task signatures
    for (decl, _span) in &program.declarations {
        register_decl(&mut checker, decl);
    }

    // Pass 2: check all declarations
    for (decl, _span) in &program.declarations {
        check_decl(&mut checker, decl);
    }

    checker.errors
}

// ---------------------------------------------------------------------------
// Pass 1: Register types and signatures
// ---------------------------------------------------------------------------

fn register_decl(checker: &mut TypeChecker, decl: &Decl) {
    match decl {
        Decl::Type(td) => {
            let ty = resolve_type_def(&td.name, &td.def);
            // Register enum variants as names in the environment
            if let Type::Enum { name, variants } = &ty {
                for v in variants {
                    checker.env.define(v.clone(), ty.clone());
                    // Also define Type.variant path
                    checker
                        .env
                        .define(format!("{name}.{v}"), ty.clone());
                }
            }
            checker.type_defs.insert(td.name.clone(), ty);
        }
        Decl::Connect(cd) => {
            checker.connections.push(cd.name.clone());
        }
        Decl::Task(td) => {
            let sig = task_signature(checker, td);
            checker.task_sigs.insert(td.name.clone(), sig.clone());
            let (params, ret) = sig;
            let param_types: Vec<Type> = params.iter().map(|(_, t)| t.clone()).collect();
            checker.env.define(
                td.name.clone(),
                Type::Func {
                    params: param_types,
                    ret: Box::new(ret),
                },
            );
        }
        Decl::Agent(ad) => {
            for item in &ad.items {
                if let AgentItem::Task(td) = item {
                    let sig = task_signature(checker, td);
                    checker.task_sigs.insert(td.name.clone(), sig.clone());
                    let (params, ret) = sig;
                    let param_types: Vec<Type> = params.iter().map(|(_, t)| t.clone()).collect();
                    checker.env.define(
                        td.name.clone(),
                        Type::Func {
                            params: param_types,
                            ret: Box::new(ret),
                        },
                    );
                }
            }
        }
        Decl::Run(_) => {}
    }
}

fn task_signature(checker: &TypeChecker, td: &TaskDecl) -> (Vec<(String, Type)>, Type) {
    let params: Vec<(String, Type)> = td
        .params
        .iter()
        .map(|p| (p.name.clone(), resolve_type_expr(checker, &p.ty)))
        .collect();
    let ret = td
        .return_type
        .as_ref()
        .map(|t| resolve_type_expr(checker, t))
        .unwrap_or(Type::None);
    (params, ret)
}

// ---------------------------------------------------------------------------
// Pass 2: Check declarations
// ---------------------------------------------------------------------------

fn check_decl(checker: &mut TypeChecker, decl: &Decl) {
    match decl {
        Decl::Task(td) => check_task(checker, td),
        Decl::Agent(ad) => check_agent(checker, ad),
        Decl::Run(rs) => {
            if !checker.type_defs.contains_key(&rs.agent)
                && !checker
                    .env
                    .get(&rs.agent)
                    .is_some()
            {
                // Check agent exists (registered in pass 1 via Agent decl)
                // We track agents loosely — just check task_sigs has agent tasks
            }
        }
        _ => {}
    }
}

fn check_task(checker: &mut TypeChecker, td: &TaskDecl) {
    checker.env.push_scope();

    // Bind parameters
    for param in &td.params {
        let ty = resolve_type_expr(checker, &param.ty);
        checker.env.define(param.name.clone(), ty);
    }

    // Check body
    let body_type = check_block(checker, &td.body);

    // Verify return type
    if let Some(declared_ret) = &td.return_type {
        let expected = resolve_type_expr(checker, declared_ret);
        if let Ok(actual) = body_type {
            if !actual.is_assignable_to(&expected) && actual != Type::None {
                checker.error(format!(
                    "Task '{}' declares return type {} but body produces {}",
                    td.name, expected, actual
                ));
            }
        }
    }

    checker.env.pop_scope();
}

fn check_agent(checker: &mut TypeChecker, ad: &AgentDecl) {
    checker.env.push_scope();
    checker.in_agent = true;
    checker.agent_state.clear();

    // Register state fields
    for item in &ad.items {
        if let AgentItem::State(fields) = item {
            for field in fields {
                let ty = resolve_type_expr(checker, &field.ty);
                checker.agent_state.insert(field.name.clone(), ty);
            }
        }
    }

    // Check tasks
    for item in &ad.items {
        match item {
            AgentItem::Task(td) => check_task(checker, td),
            AgentItem::Every(every) => {
                let _ = infer_expr(checker, &every.interval);
                check_block(checker, &every.body).ok();
            }
            AgentItem::On(handler) => {
                checker.env.push_scope();
                if let Some(param) = &handler.param {
                    let ty = resolve_type_expr(checker, &param.ty);
                    checker.env.define(param.name.clone(), ty);
                }
                check_block(checker, &handler.body).ok();
                checker.env.pop_scope();
            }
            _ => {}
        }
    }

    checker.in_agent = false;
    checker.env.pop_scope();
}

// ---------------------------------------------------------------------------
// Block & statement checking
// ---------------------------------------------------------------------------

fn check_block(checker: &mut TypeChecker, block: &Block) -> TResult {
    checker.env.push_scope();
    let mut last_type = Type::None;

    for (stmt, span) in block {
        let prev_span = checker.current_span.clone();
        if !span.is_empty() {
            checker.current_span = Some(span.clone());
        }
        match check_stmt(checker, stmt) {
            Ok(ty) => last_type = ty,
            Err(mut e) => {
                if e.span.is_none() && !span.is_empty() {
                    e.span = Some(span.clone());
                }
                checker.errors.push(e);
            }
        }
        checker.current_span = prev_span;
    }

    checker.env.pop_scope();
    Ok(last_type)
}

fn check_stmt(checker: &mut TypeChecker, stmt: &Stmt) -> TResult {
    match stmt {
        Stmt::Let { name, ty, value } => {
            let inferred = infer_expr(checker, value)?;
            let declared = ty
                .as_ref()
                .map(|t| resolve_type_expr(checker, t));

            if let Some(declared_ty) = &declared {
                if !inferred.is_assignable_to(declared_ty) {
                    checker.error(format!(
                        "Cannot assign {} to variable '{}' of type {}",
                        inferred, name, declared_ty
                    ));
                }
            }

            let final_type = declared.unwrap_or(inferred);
            checker.env.define(name.clone(), final_type.clone());
            Ok(final_type)
        }

        Stmt::SelfAssign { field, value } => {
            if !checker.in_agent {
                return Err(type_err("self can only be used inside an agent"));
            }
            let val_type = infer_expr(checker, value)?;
            if let Some(expected) = checker.agent_state.get(field) {
                if !val_type.is_assignable_to(expected) {
                    checker.error(format!(
                        "Cannot assign {} to state field '{}' of type {}",
                        val_type, field, expected
                    ));
                }
            } else {
                checker.error(format!("Unknown state field: '{field}'"));
            }
            Ok(val_type)
        }

        Stmt::Expr(expr) => infer_expr(checker, expr),

        Stmt::Return(expr) => {
            if let Some(e) = expr {
                infer_expr(checker, e)
            } else {
                Ok(Type::None)
            }
        }

        Stmt::For {
            binding,
            iter,
            filter,
            body,
        } => {
            let iter_type = infer_expr(checker, iter)?;
            let elem_type = match &iter_type {
                Type::List(inner) => *inner.clone(),
                _ => {
                    checker.error(format!("for loop requires a list, got {iter_type}"));
                    Type::Unknown
                }
            };

            checker.env.push_scope();
            checker.env.define(binding.clone(), elem_type);

            if let Some(pred) = filter {
                let pred_type = infer_expr(checker, pred)?;
                if pred_type != Type::Bool && pred_type != Type::Unknown {
                    checker.error(format!("for-where filter must be bool, got {pred_type}"));
                }
            }

            check_block(checker, body)?;
            checker.env.pop_scope();
            Ok(Type::None)
        }

        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let cond_type = infer_expr(checker, cond)?;
            if cond_type != Type::Bool && cond_type != Type::Unknown {
                checker.error(format!("if condition must be bool, got {cond_type}"));
            }
            check_block(checker, then_body)?;
            if let Some(else_b) = else_body {
                check_block(checker, else_b)?;
            }
            Ok(Type::None)
        }

        Stmt::When { subject, arms } => {
            let subject_type = infer_expr(checker, subject)?;
            check_when_exhaustiveness(checker, &subject_type, arms);
            for arm in arms {
                checker.env.push_scope();
                check_block(checker, &arm.body)?;
                checker.env.pop_scope();
            }
            Ok(Type::None)
        }

        Stmt::Notify { message } => {
            infer_expr(checker, message)?;
            Ok(Type::None)
        }

        Stmt::Show { value } => {
            infer_expr(checker, value)?;
            Ok(Type::None)
        }

        Stmt::Send { value, target } => {
            infer_expr(checker, value)?;
            infer_expr(checker, target)?;
            Ok(Type::None)
        }

        Stmt::Archive { value } => {
            infer_expr(checker, value)?;
            Ok(Type::None)
        }

        Stmt::ConfirmThen {
            message,
            then_body,
        } => {
            infer_expr(checker, message)?;
            check_block(checker, then_body)?;
            Ok(Type::None)
        }

        Stmt::Remember { value } => {
            infer_expr(checker, value)?;
            Ok(Type::None)
        }

        Stmt::Wait { duration, condition } => {
            if let Some(dur) = duration {
                let ty = infer_expr(checker, dur)?;
                if ty != Type::Duration && ty != Type::Unknown {
                    checker.error(format!("wait requires a duration, got {ty}"));
                }
            }
            if let Some(cond) = condition {
                let ty = infer_expr(checker, cond)?;
                if ty != Type::Bool && ty != Type::Unknown {
                    checker.error(format!("wait until requires a bool condition, got {ty}"));
                }
            }
            Ok(Type::None)
        }

        Stmt::Retry {
            count, body, ..
        } => {
            let count_type = infer_expr(checker, count)?;
            if count_type != Type::Int && count_type != Type::Unknown {
                checker.error(format!("retry count must be int, got {count_type}"));
            }
            check_block(checker, body)
        }

        Stmt::After { delay, body } => {
            let delay_type = infer_expr(checker, delay)?;
            if delay_type != Type::Duration && delay_type != Type::Unknown {
                checker.error(format!("after requires a duration, got {delay_type}"));
            }
            check_block(checker, body)?;
            Ok(Type::None)
        }

        Stmt::TryCatch { body, catches } => {
            check_block(checker, body)?;
            for clause in catches {
                checker.env.push_scope();
                let ty = resolve_type_expr(checker, &clause.ty);
                checker.env.define(clause.name.clone(), ty);
                check_block(checker, &clause.body)?;
                checker.env.pop_scope();
            }
            Ok(Type::None)
        }
    }
}

// ---------------------------------------------------------------------------
// Expression type inference
// ---------------------------------------------------------------------------

fn infer_expr(checker: &mut TypeChecker, expr: &Expr) -> TResult {
    match expr {
        Expr::Integer(_) => Ok(Type::Int),
        Expr::Float(_) => Ok(Type::Float),
        Expr::Bool(_) => Ok(Type::Bool),
        Expr::None_ => Ok(Type::None),
        Expr::Now => Ok(Type::Datetime),
        Expr::StringLit(_) => Ok(Type::Str),

        Expr::Ident(name) => checker
            .env
            .get(name)
            .cloned()
            .or_else(|| {
                if checker.connections.contains(name) {
                    Some(Type::Str)
                } else {
                    None
                }
            })
            .ok_or_else(|| type_err(format!("Undefined variable: '{name}'"))),

        Expr::FieldAccess(obj, field) => {
            let obj_type = infer_expr(checker, obj)?;
            infer_field_access(&obj_type, field)
        }

        Expr::NullFieldAccess(obj, field) => {
            let obj_type = infer_expr(checker, obj)?;
            let inner = obj_type.unwrap_nullable();
            infer_field_access(inner, field).map(|t| t.nullable())
        }

        Expr::NullAssert(inner) => {
            let ty = infer_expr(checker, inner)?;
            Ok(ty.unwrap_nullable().clone())
        }

        Expr::EnvAccess(_) => Ok(Type::Str),

        Expr::SelfAccess(field) => {
            if !checker.in_agent {
                return Err(type_err("self can only be used inside an agent"));
            }
            checker
                .agent_state
                .get(field)
                .cloned()
                .ok_or_else(|| type_err(format!("Unknown state field: '{field}'")))
        }

        Expr::StructLit(fields) => {
            let typed_fields: Vec<(String, Type)> = fields
                .iter()
                .map(|(name, expr)| {
                    let ty = infer_expr(checker, expr).unwrap_or(Type::Unknown);
                    (name.clone(), ty)
                })
                .collect();
            Ok(Type::Struct(typed_fields))
        }

        Expr::ListLit(items) => {
            if items.is_empty() {
                return Ok(Type::List(Box::new(Type::Unknown)));
            }
            let first_type = infer_expr(checker, &items[0])?;
            for item in &items[1..] {
                let item_type = infer_expr(checker, item)?;
                if !item_type.is_assignable_to(&first_type) && item_type != Type::Unknown {
                    checker.error(format!(
                        "List element type mismatch: expected {first_type}, got {item_type}"
                    ));
                }
            }
            Ok(Type::List(Box::new(first_type)))
        }

        Expr::BinaryOp { left, op, right } => {
            let l = infer_expr(checker, left)?;
            let r = infer_expr(checker, right)?;
            infer_binop(&l, *op, &r, checker)
        }

        Expr::UnaryOp { op, expr } => {
            let ty = infer_expr(checker, expr)?;
            match op {
                UnOp::Neg => {
                    if !ty.is_numeric() && ty != Type::Unknown {
                        checker.error(format!("Cannot negate {ty}"));
                    }
                    Ok(ty)
                }
                UnOp::Not => Ok(Type::Bool),
            }
        }

        Expr::NullCoalesce(left, right) => {
            let l = infer_expr(checker, left)?;
            let r = infer_expr(checker, right)?;
            // none ?? X → type of X
            if l == Type::None {
                return Ok(r);
            }
            // T? ?? X → T (check X is compatible)
            let inner = l.unwrap_nullable().clone();
            if !r.is_assignable_to(&inner) && r != Type::Unknown && inner != Type::Unknown {
                checker.error(format!(
                    "Null coalescing type mismatch: {inner} ?? {r}"
                ));
            }
            Ok(inner)
        }

        Expr::Pipeline(left, _right) => {
            // Type checking pipelines fully requires resolving the right-side function
            // For now, infer left and return Unknown for the pipeline result
            infer_expr(checker, left)?;
            Ok(Type::Unknown)
        }

        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref() {
                if let Some((params, ret)) = checker.task_sigs.get(name).cloned() {
                    // Check argument count
                    if args.len() > params.len() {
                        checker.error(format!(
                            "Task '{}' expects {} arguments, got {}",
                            name,
                            params.len(),
                            args.len()
                        ));
                    }
                    // Check argument types
                    for (i, arg) in args.iter().enumerate() {
                        if let Some((param_name, param_type)) = params.get(i) {
                            let arg_type = infer_expr(checker, &arg.value)?;
                            if !arg_type.is_assignable_to(param_type) && arg_type != Type::Unknown {
                                checker.error(format!(
                                    "Argument '{}' of task '{}': expected {}, got {}",
                                    param_name, name, param_type, arg_type
                                ));
                            }
                        }
                    }
                    return Ok(ret);
                }
            }
            // Unknown callee — infer args but return Unknown
            for arg in args {
                infer_expr(checker, &arg.value)?;
            }
            Ok(Type::Unknown)
        }

        Expr::MethodCall { object, method, args } => {
            let obj_type = infer_expr(checker, object)?;
            for arg in args {
                infer_expr(checker, &arg.value)?;
            }
            infer_method_return(&obj_type, method)
        }

        // ── AI primitives ────────────────────────────────────────
        Expr::Classify {
            input,
            target,
            fallback,
            ..
        } => {
            infer_expr(checker, input)?;
            let target_type = match target {
                ClassifyTarget::Named(name) => {
                    checker
                        .type_defs
                        .get(name)
                        .cloned()
                        .unwrap_or(Type::Unknown)
                }
                ClassifyTarget::Inline(variants) => Type::Enum {
                    name: "inline".to_string(),
                    variants: variants.clone(),
                },
            };
            if let Some(fb) = fallback {
                infer_expr(checker, fb)?;
                Ok(target_type) // With fallback → non-nullable
            } else {
                Ok(target_type.nullable()) // Without fallback → nullable
            }
        }

        Expr::Summarize {
            input, fallback, ..
        } => {
            infer_expr(checker, input)?;
            if let Some(fb) = fallback {
                infer_expr(checker, fb)?;
                Ok(Type::Str)
            } else {
                Ok(Type::Str.nullable())
            }
        }

        Expr::Draft {
            description,
            options,
            ..
        } => {
            infer_expr(checker, description)?;
            for (_, val) in options {
                infer_expr(checker, val)?;
            }
            Ok(Type::Str.nullable())
        }

        Expr::Extract { .. } => Ok(Type::Unknown), // TODO
        Expr::Translate { .. } => Ok(Type::Str.nullable()),
        Expr::Decide { .. } => Ok(Type::Unknown), // TODO

        Expr::Prompt { target_type, .. } => {
            match target_type.as_str() {
                "dynamic" => Ok(Type::Dynamic),
                "str" => Ok(Type::Str.nullable()),
                _ => Ok(Type::Unknown), // Struct type — resolved at runtime
            }
        }

        Expr::Ask { prompt, .. } => {
            infer_expr(checker, prompt)?;
            Ok(Type::Str)
        }

        Expr::Confirm { message } => {
            infer_expr(checker, message)?;
            Ok(Type::Bool)
        }

        Expr::Fetch { source, .. } => {
            let src_type = infer_expr(checker, source)?;
            // Connection-sourced fetch (identifier) returns non-nullable list
            // URL fetch (string) returns nullable
            match src_type {
                Type::Str => {
                    // Could be a connection name or a URL — return non-nullable list
                    // (connections always return a list, possibly empty)
                    Ok(Type::List(Box::new(Type::Unknown)))
                }
                _ => Ok(Type::List(Box::new(Type::Unknown)).nullable()),
            }
        }

        Expr::Recall { query, .. } => {
            infer_expr(checker, query)?;
            Ok(Type::List(Box::new(Type::Unknown)))
        }

        Expr::Delegate { task_call, .. } => {
            let inner = infer_expr(checker, task_call)?;
            Ok(inner.nullable())
        }

        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            let cond_type = infer_expr(checker, cond)?;
            if cond_type != Type::Bool && cond_type != Type::Unknown {
                checker.error(format!("if condition must be bool, got {cond_type}"));
            }
            let then_type = check_block(checker, then_body)?;
            let else_type = check_block(checker, else_body)?;
            // Both branches must produce compatible types
            if then_type != else_type
                && then_type != Type::Unknown
                && else_type != Type::Unknown
                && then_type != Type::None
                && else_type != Type::None
            {
                checker.error(format!(
                    "if/else branches have different types: {} vs {}",
                    then_type, else_type
                ));
            }
            Ok(then_type)
        }

        Expr::WhenExpr { subject, arms } => {
            let subject_type = infer_expr(checker, subject)?;
            check_when_exhaustiveness(checker, &subject_type, arms);
            // Infer type from first arm
            if let Some(arm) = arms.first() {
                check_block(checker, &arm.body)
            } else {
                Ok(Type::None)
            }
        }

        Expr::Lambda { .. } => Ok(Type::Unknown), // TODO
        Expr::Duration { .. } => Ok(Type::Duration),
        Expr::EnumVariant(type_name, _) => {
            checker
                .type_defs
                .get(type_name)
                .cloned()
                .ok_or_else(|| type_err(format!("Unknown type: '{type_name}'")))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn infer_field_access(obj_type: &Type, field: &str) -> TResult {
    match obj_type {
        Type::Struct(fields) => {
            fields
                .iter()
                .find(|(n, _)| n == field)
                .map(|(_, t)| t.clone())
                .ok_or_else(|| type_err(format!("No field '{field}' on {obj_type}")))
        }
        Type::List(_) => match field {
            "count" => Ok(Type::Int),
            "first" | "last" => Ok(obj_type.clone()), // simplified
            "is_empty" => Ok(Type::Bool),
            _ => Err(type_err(format!("No property '{field}' on list"))),
        },
        Type::Str => match field {
            "length" => Ok(Type::Int),
            "is_empty" => Ok(Type::Bool),
            _ => Err(type_err(format!("No property '{field}' on str"))),
        },
        Type::Map(_, v) => Ok(*v.clone()), // field access on map returns value type
        Type::Unknown => Ok(Type::Unknown),
        _ => Err(type_err(format!("Cannot access field '{field}' on {obj_type}"))),
    }
}

fn infer_method_return(obj_type: &Type, method: &str) -> TResult {
    match obj_type {
        Type::Str => match method {
            "contains" | "starts_with" | "ends_with" | "is_empty" => Ok(Type::Bool),
            "trim" | "upper" | "lower" | "replace" => Ok(Type::Str),
            "split" => Ok(Type::List(Box::new(Type::Str))),
            "to_int" => Ok(Type::Int.nullable()),
            "to_float" => Ok(Type::Float.nullable()),
            "slice" => Ok(Type::Str),
            _ => Ok(Type::Unknown),
        },
        Type::List(elem) => match method {
            "map" => Ok(Type::List(Box::new(Type::Unknown))),
            "filter" => Ok(Type::List(elem.clone())),
            "find" => Ok(elem.as_ref().clone().nullable()),
            "any" | "all" => Ok(Type::Bool),
            "sort_by" => Ok(obj_type.clone()),
            "group_by" => Ok(Type::Map(Box::new(Type::Unknown), Box::new(obj_type.clone()))),
            _ => Ok(Type::Unknown),
        },
        Type::Int => match method {
            "to_str" => Ok(Type::Str),
            "to_float" => Ok(Type::Float),
            _ => Ok(Type::Unknown),
        },
        _ => Ok(Type::Unknown),
    }
}

fn infer_binop(left: &Type, op: BinOp, right: &Type, checker: &mut TypeChecker) -> TResult {
    match op {
        BinOp::Add => match (left, right) {
            (Type::Int, Type::Int) => Ok(Type::Int),
            (Type::Float, Type::Float) | (Type::Int, Type::Float) | (Type::Float, Type::Int) => {
                Ok(Type::Float)
            }
            (Type::Str, Type::Str) => Ok(Type::Str),
            (Type::Unknown, _) | (_, Type::Unknown) => Ok(Type::Unknown),
            _ => {
                checker.error(format!("Cannot add {left} and {right}"));
                Ok(Type::Unknown)
            }
        },
        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => match (left, right) {
            (Type::Int, Type::Int) => Ok(Type::Int),
            (Type::Float, _) | (_, Type::Float) => Ok(Type::Float),
            (Type::Duration, Type::Int) if matches!(op, BinOp::Mul) => Ok(Type::Duration),
            (Type::Unknown, _) | (_, Type::Unknown) => Ok(Type::Unknown),
            _ => {
                checker.error(format!("Cannot apply {:?} to {left} and {right}", op));
                Ok(Type::Unknown)
            }
        },
        BinOp::Eq | BinOp::Neq => Ok(Type::Bool),
        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
            if !left.is_numeric() && *left != Type::Str && *left != Type::Unknown {
                checker.error(format!("Cannot compare {left}"));
            }
            Ok(Type::Bool)
        }
        BinOp::And | BinOp::Or => Ok(Type::Bool),
    }
}

// ---------------------------------------------------------------------------
// When exhaustiveness checking
// ---------------------------------------------------------------------------

fn check_when_exhaustiveness(checker: &mut TypeChecker, subject_type: &Type, arms: &[WhenArm]) {
    if let Type::Enum { name, variants } = subject_type {
        let has_wildcard = arms
            .iter()
            .any(|arm| arm.patterns.iter().any(|p| matches!(p, Pattern::Wildcard)));

        if !has_wildcard {
            let covered: Vec<&str> = arms
                .iter()
                .flat_map(|arm| {
                    arm.patterns.iter().filter_map(|p| match p {
                        Pattern::Ident(s) => Some(s.as_str()),
                        Pattern::Variant { name, .. } => Some(name.as_str()),
                        _ => None,
                    })
                })
                .collect();

            let missing: Vec<&str> = variants
                .iter()
                .filter(|v| !covered.contains(&v.as_str()))
                .map(|v| v.as_str())
                .collect();

            if !missing.is_empty() {
                checker.error(format!(
                    "Non-exhaustive match on {name}: missing {}",
                    missing.join(", ")
                ));
            }
        }
    } else {
        // Non-enum types require a wildcard
        let has_wildcard = arms
            .iter()
            .any(|arm| arm.patterns.iter().any(|p| matches!(p, Pattern::Wildcard)));

        if !has_wildcard {
            checker.error("when on non-enum type requires a wildcard '_' arm".to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// AST TypeExpr → resolved Type
// ---------------------------------------------------------------------------

fn resolve_type_expr(checker: &TypeChecker, ty: &TypeExpr) -> Type {
    match ty {
        TypeExpr::Named(name) => match name.as_str() {
            "int" => Type::Int,
            "float" => Type::Float,
            "str" => Type::Str,
            "bool" => Type::Bool,
            "none" => Type::None,
            "duration" => Type::Duration,
            "datetime" => Type::Datetime,
            "dynamic" => Type::Dynamic,
            _ => checker
                .type_defs
                .get(name)
                .cloned()
                .unwrap_or(Type::Unknown),
        },
        TypeExpr::Nullable(inner) => resolve_type_expr(checker, inner).nullable(),
        TypeExpr::List(inner) => Type::List(Box::new(resolve_type_expr(checker, inner))),
        TypeExpr::Map(k, v) => Type::Map(
            Box::new(resolve_type_expr(checker, k)),
            Box::new(resolve_type_expr(checker, v)),
        ),
        TypeExpr::Set(inner) => Type::Set(Box::new(resolve_type_expr(checker, inner))),
        TypeExpr::Struct(fields) => Type::Struct(
            fields
                .iter()
                .map(|f| (f.name.clone(), resolve_type_expr(checker, &f.ty)))
                .collect(),
        ),
        TypeExpr::Tuple(types) => {
            Type::Tuple(types.iter().map(|t| resolve_type_expr(checker, t)).collect())
        }
        TypeExpr::Func(params, ret) => Type::Func {
            params: params.iter().map(|t| resolve_type_expr(checker, t)).collect(),
            ret: Box::new(resolve_type_expr(checker, ret)),
        },
    }
}

fn resolve_type_def(name: &str, def: &TypeDef) -> Type {
    match def {
        TypeDef::SimpleEnum(variants) => Type::Enum {
            name: name.to_string(),
            variants: variants.clone(),
        },
        TypeDef::RichEnum(variants) => Type::Enum {
            name: name.to_string(),
            variants: variants.iter().map(|v| v.name.clone()).collect(),
        },
        TypeDef::Struct(fields) => Type::Struct(
            fields
                .iter()
                .map(|f| {
                    let ty = match &f.ty {
                        TypeExpr::Named(n) => match n.as_str() {
                            "int" => Type::Int,
                            "float" => Type::Float,
                            "str" => Type::Str,
                            "bool" => Type::Bool,
                            _ => Type::Unknown,
                        },
                        _ => Type::Unknown,
                    };
                    (f.name.clone(), ty)
                })
                .collect(),
        ),
    }
}
