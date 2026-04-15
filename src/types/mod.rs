pub mod checker;

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Type representation (resolved, not AST-level)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Str,
    Bool,
    None,
    Duration,
    Datetime,
    Dynamic,

    /// Nullable wrapper: T?
    Nullable(Box<Type>),

    /// List: list[T]
    List(Box<Type>),
    /// Map: map[K, V]
    Map(Box<Type>, Box<Type>),
    /// Set: set[T]
    Set(Box<Type>),

    /// Struct with named fields (structural)
    Struct(Vec<(String, Type)>),

    /// Tuple: (T, U, ...)
    Tuple(Vec<Type>),

    /// Named enum type
    Enum {
        name: String,
        variants: Vec<String>,
    },

    /// Function type: (params) -> return
    Func {
        params: Vec<Type>,
        ret: Box<Type>,
    },

    /// Agent reference
    Agent(String),

    /// A type variable not yet resolved (for future generic inference)
    Unknown,
}

impl Type {
    /// Make this type nullable. Already-nullable types stay the same.
    pub fn nullable(self) -> Type {
        match self {
            Type::Nullable(_) => self,
            Type::None => Type::None,
            other => Type::Nullable(Box::new(other)),
        }
    }

    /// Unwrap one layer of nullable. Non-nullable types return themselves.
    pub fn unwrap_nullable(&self) -> &Type {
        match self {
            Type::Nullable(inner) => inner,
            other => other,
        }
    }

    pub fn is_nullable(&self) -> bool {
        matches!(self, Type::Nullable(_) | Type::None)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    /// Check structural compatibility: can `self` be assigned where `target` is expected?
    pub fn is_assignable_to(&self, target: &Type) -> bool {
        if self == target {
            return true;
        }
        match (self, target) {
            // None is assignable to any nullable type
            (Type::None, Type::Nullable(_)) => true,
            // T is assignable to T?
            (inner, Type::Nullable(expected)) => inner.is_assignable_to(expected),
            // T? is NOT assignable to T (must unwrap first)
            (Type::Nullable(_), _) => false,
            // Int can be used where Float is expected
            (Type::Int, Type::Float) => true,
            // Struct subtyping: A is assignable to B if A has all fields of B
            (Type::Struct(a_fields), Type::Struct(b_fields)) => {
                b_fields.iter().all(|(name, ty)| {
                    a_fields
                        .iter()
                        .any(|(n, t)| n == name && t.is_assignable_to(ty))
                })
            }
            // List covariance
            (Type::List(a), Type::List(b)) => a.is_assignable_to(b),
            // Unknown matches anything (for partial inference)
            (Type::Unknown, _) | (_, Type::Unknown) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Str => write!(f, "str"),
            Type::Bool => write!(f, "bool"),
            Type::None => write!(f, "none"),
            Type::Duration => write!(f, "duration"),
            Type::Datetime => write!(f, "datetime"),
            Type::Dynamic => write!(f, "dynamic"),
            Type::Nullable(inner) => write!(f, "{inner}?"),
            Type::List(inner) => write!(f, "list[{inner}]"),
            Type::Map(k, v) => write!(f, "map[{k}, {v}]"),
            Type::Set(inner) => write!(f, "set[{inner}]"),
            Type::Struct(fields) => {
                write!(f, "{{")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {ty}")?;
                }
                write!(f, "}}")
            }
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ")")
            }
            Type::Enum { name, .. } => write!(f, "{name}"),
            Type::Func { params, ret } => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")
            }
            Type::Agent(name) => write!(f, "agent {name}"),
            Type::Unknown => write!(f, "unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Type environment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TypeEnv {
    scopes: Vec<HashMap<String, Type>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        TypeEnv {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn define(&mut self, name: String, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}
