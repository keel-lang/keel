//! Statement, binding, and pattern nodes.

use super::{Expr, Spanned, TypeExpr};

pub type Block = Vec<Spanned<Stmt>>;

/// Left-hand side of a destructuring assignment or parameter.
/// Distinct from `Pattern` (used in `when` arms) — destructuring binds, not matches.
#[derive(Debug, Clone)]
pub enum DestructPat {
    /// `{field}` shorthand or `{field: rename, ...}`.
    /// Each entry is `(source_field, local_name)`.
    /// For shorthand `{field}`, both strings are identical.
    Struct(Vec<(String, String)>),
    /// `(a, b, c)` — positional tuple bind.
    Tuple(Vec<String>),
}

/// Left-hand side of a `let` binding, `for` loop variable, or task parameter name.
#[derive(Debug, Clone)]
pub enum Binding {
    /// Simple identifier: `x = expr`
    Ident(String),
    /// Destructuring: `{a, b} = expr` or `(a, b) = expr`
    Destruct(DestructPat),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `x = expr`, `x: Type = expr`, `{a, b} = expr`, or `(a, b) = expr`
    Let {
        binding: Binding,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    /// `self.field = expr`
    SelfAssign { field: String, value: Expr },
    /// `return expr`
    Return(Option<Expr>),
    /// `for x in expr { ... }`, `for {a, b} in expr { ... }`, or with `where pred`
    For {
        binding: Binding,
        iter: Expr,
        filter: Option<Expr>,
        body: Block,
    },
    /// `if cond { ... } else { ... }` — statement form, used when the value
    /// isn't consumed (for branching side effects).
    If {
        cond: Expr,
        then_body: Block,
        else_body: Option<Block>,
    },
    /// `when expr { arms }` — statement form.
    When { subject: Expr, arms: Vec<WhenArm> },
    /// `try { ... } catch err: Type { ... }`
    TryCatch {
        body: Block,
        catches: Vec<CatchClause>,
    },
    /// `x += rhs`, `x -= rhs`, etc. — mutates an existing binding in the
    /// nearest enclosing scope; does not create a shadow. Distinct from
    /// `Stmt::Let` (which uses define-in-current-scope semantics) so that
    /// accumulation inside `for` loops updates the outer variable correctly.
    AugAssign {
        name: String,
        op: crate::ast::expr::BinOp,
        rhs: Expr,
    },
    /// `raise expr` — throws an error; caught by `catch err: Error`.
    Raise(Expr),
    /// `while cond { ... }` — repeat body until condition is false.
    While { cond: Expr, body: Block },
    /// `break` — exits the nearest enclosing loop.
    Break,
    /// `continue` — skips the rest of the current iteration.
    Continue,
    /// Expression used as a statement — covers `Io.notify(...)`,
    /// `Email.send(...)`, `run(MyAgent)`, bare calls, etc.
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct WhenArm {
    pub patterns: Vec<Pattern>,
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    /// Identifier: matches an enum variant by name or binds a variable.
    Ident(String),
    /// Wildcard: `_`
    Wildcard,
    /// Literal value.
    Literal(Expr),
    /// Rich enum variant destructure: `reply { to, tone }`.
    Variant { name: String, bindings: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub name: String,
    pub ty: TypeExpr,
    pub body: Block,
}
