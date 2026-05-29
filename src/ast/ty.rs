//! Type declarations and type-expression nodes.

use super::Node;

#[derive(Debug, Clone)]
pub enum TypeDef {
    /// `type Urgency = low | medium | high | critical`
    SimpleEnum(Vec<String>),
    /// `type Action = | reply { to: str } | archive`
    RichEnum(Vec<EnumVariant>),
    /// `type EmailInfo { sender: str, subject: str }`
    Struct(Vec<Field>),
    /// `type Timestamp = datetime`
    Alias(Node<TypeExpr>),
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Option<Vec<Field>>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    /// The type annotation for this field, together with its source span.
    pub ty: Node<TypeExpr>,
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// Named type: `str`, `int`, `Urgency`
    Named(String),
    /// Nullable: `str?`
    Nullable(Box<TypeExpr>),
    /// List: `list[str]`
    List(Box<TypeExpr>),
    /// Map: `map[str, int]`
    Map(Box<TypeExpr>, Box<TypeExpr>),
    /// Set: `set[int]`
    Set(Box<TypeExpr>),
    /// Inline struct: `{body: str, from: str}`
    Struct(Vec<Field>),
    /// Tuple: `(str, int)`
    Tuple(Vec<TypeExpr>),
    /// Function type: `(str) -> bool`
    Func(Vec<TypeExpr>, Box<TypeExpr>),
    /// Generic application: `Result[T, E]`
    Generic(String, Vec<TypeExpr>),
    /// Dynamic (FFI escape hatch)
    Dynamic,
}
