use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::ast::{DurationUnit, LambdaBody, LambdaParam, TaskDecl};
use crate::runtime::db_provider::DbConnectionHandle;

/// Hashable map key — valid key types for `map[K, V]`.
/// `float` is intentionally excluded: NaN violates the Hash/Eq contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MapKey {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl MapKey {
    /// Convert a runtime `Value` to a `MapKey`, returning `None` if the value
    /// cannot be used as a map key (e.g. float, list, none).
    pub fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::String(s) => Some(MapKey::Str(s.clone())),
            Value::Integer(n) => Some(MapKey::Int(*n)),
            Value::Bool(b) => Some(MapKey::Bool(*b)),
            _ => None,
        }
    }

    /// Convert this key back into the corresponding `Value`.
    pub fn to_value(&self) -> Value {
        match self {
            MapKey::Str(s) => Value::String(s.clone()),
            MapKey::Int(n) => Value::Integer(*n),
            MapKey::Bool(b) => Value::Bool(*b),
        }
    }
}

impl fmt::Display for MapKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapKey::Str(s) => write!(f, "{s}"),
            MapKey::Int(n) => write!(f, "{n}"),
            MapKey::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Runtime value representation for the Keel interpreter.
#[derive(Debug, Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    None,

    /// List of values
    List(Vec<Value>),
    /// Deduplicated collection of values, in insertion order.
    ///
    /// Backed by a `Vec` rather than a `HashSet` because `Value` has no `Hash`
    /// impl and cannot get one: `set[T]` accepts any `T` the checker accepts
    /// (structs, floats, nested lists), whereas hashing would force `MapKey`'s
    /// str/int/bool restriction onto sets that don't need it. Membership uses
    /// the hand-written [`PartialEq`] below, so dedup follows exactly the same
    /// equality rules as `==` everywhere else in the language — including the
    /// consequence that `float` NaN never dedups against itself.
    ///
    /// Insertion order is preserved, which is what makes set output
    /// deterministic without the sort-on-observation that `Map` needs.
    /// Construct and extend it through [`set_insert`] so the interpreter and
    /// `keel-rt-ffi`'s `keel_set_insert` share one definition of dedup.
    Set(Vec<Value>),
    /// Lazy inclusive integer range — stores only lo and hi, never materializes.
    Range(i64, i64),
    /// Anonymous map literal — keys are str, int, or bool (`MapKey`).
    Map(HashMap<MapKey, Value>),
    /// Type-tagged struct instance: (declared_type_name, fields).
    /// Created when a Map literal is bound with a known struct type annotation,
    /// passed to/from a typed task param, or returned from Ai.extract.
    /// Enables O(1) impl-method dispatch instead of field-set subset matching.
    /// Struct fields are always string-keyed.
    Struct(String, HashMap<String, Value>),

    /// An enum variant: (type_name, variant_name, optional rich fields).
    /// Simple variants (`Urgency.high`) use `None`; rich variants
    /// (`Action.reply { to: "x" }`) carry their constructed fields.
    EnumVariant(String, String, Option<HashMap<String, Value>>),

    /// Duration in seconds
    Duration(f64),

    /// UUID value stored in canonical lowercase hyphenated form.
    Uuid(String),

    /// A callable task (name, Arc-wrapped declaration).
    Task(String, Arc<TaskDecl>),

    /// Reference to an agent by name
    AgentRef(String),

    /// Typed reference to a specific handler on an agent: `Foo.handle`.
    /// Produced by `FieldAccess(Ident("Foo"), "handle")` when `Foo` is a
    /// registered agent. Consumed by `Agent.delegate` as the 2-arg symbol form.
    AgentHandlerRef(String, String),

    /// A closure: (params, boxed body).
    ///
    /// Carries no module identity of its own — when a debugger attributes a
    /// paused frame to a source file, an invoked closure inherits whichever
    /// module was already current at its call site rather than being tagged
    /// with the module it was written in. This is correct for the
    /// overwhelmingly common case (a lambda invoked right where it's
    /// defined) and only misattributes a closure literal that crosses a
    /// module boundary before being invoked (e.g. passed into `.map()`,
    /// `Schedule.every()`, or `Http.serve()` from a different file than
    /// where it was written) — a documented D0 limitation, not a bug.
    Closure(Vec<LambdaParam>, Box<LambdaBody>),

    /// Prelude namespace (by name). Method dispatch goes through the
    /// interpreter's namespace registry.
    Namespace(String),

    /// Test-only reference to a mockable namespace method.
    MockTarget {
        namespace: String,
        method: String,
    },

    /// Test-only handle returned by `testing.mock(target)`.
    MockHandle {
        namespace: String,
        method: String,
    },

    /// An open database connection.
    ///
    /// Stores the original URL for display and a shared backend handle.
    /// Cloning the value shares the same underlying connection (Arc).
    /// Two `DbConnection` values are equal only when they share the same
    /// `Arc` allocation (i.e., are the same connection object).
    DbConnection(String, Arc<dyn DbConnectionHandle>),

    /// A top-level built-in (by name) — `run`, `stop`, etc.
    BuiltinFn(String),

    /// A local module namespace bound by `use "./x.keel"`.
    /// Member access resolves against the flat global table.
    Module(Arc<ModuleValue>),
}

/// Appends `value` to a set's backing vector unless an equal element is
/// already present, and reports whether it was added.
///
/// The single definition of set dedup. Both engines route through it — the
/// interpreter for `set[...]` literals and `.add(v)`, `keel-rt-ffi` for
/// `keel_set_new`/`keel_set_insert` — so compiled and interpreted programs
/// agree on set membership by construction rather than by differential test.
///
/// Equality is `Value`'s own [`PartialEq`], so a set of structs dedups on
/// field values and a set of floats inherits IEEE NaN semantics.
pub fn set_insert(items: &mut Vec<Value>, value: Value) -> bool {
    if items.contains(&value) {
        return false;
    }
    items.push(value);
    true
}

/// Runtime view of a local module: its name and exposed member names.
#[derive(Debug)]
pub struct ModuleValue {
    pub name: String,
    /// Top-level task names (including externs).
    pub tasks: std::collections::HashSet<String>,
    /// Agent declaration names.
    pub agents: std::collections::HashSet<String>,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "str",
            Value::Bool(_) => "bool",
            Value::None => "none",
            Value::List(_) | Value::Range(_, _) => "list",
            Value::Set(_) => "set",
            Value::Map(_) => "map",
            Value::Struct(_, _) => "struct",
            Value::EnumVariant(_, _, _) => "enum",
            Value::Duration(_) => "duration",
            Value::Uuid(_) => "Uuid",
            Value::Task(_, _) => "task",
            Value::AgentRef(_) => "agent",
            Value::AgentHandlerRef(_, _) => "agent_handler",
            Value::Closure(_, _) => "closure",
            Value::Namespace(_) => "namespace",
            Value::MockTarget { .. } => "mock_target",
            Value::MockHandle { .. } => "mock",
            Value::DbConnection(_, _) => "DbConnection",
            Value::BuiltinFn(_) => "builtin",
            Value::Module(_) => "module",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::None => false,
            Value::Integer(0) => false,
            Value::String(s) if s.is_empty() => false,
            Value::List(l) if l.is_empty() => false,
            Value::Set(s) if s.is_empty() => false,
            Value::Range(lo, hi) if lo > hi => false,
            Value::DbConnection(_, _) | Value::MockTarget { .. } | Value::MockHandle { .. } => true,
            _ => true,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// Look up a string-named field on either a `Map` (by `MapKey::Str`) or a
    /// `Struct` (by its string field name). Returns `None` for any other variant.
    pub fn get_str_field(&self, field: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.get(&MapKey::Str(field.to_string())),
            Value::Struct(_, fields) => fields.get(field),
            _ => None,
        }
    }

    /// Convert this `Map` into a `HashMap<String, Value>` by extracting only the
    /// `MapKey::Str` entries. Returns `None` if any key is not a string (in
    /// practice this only arises when a non-string-key map is accidentally
    /// promoted — the type checker prevents this in well-typed programs).
    pub fn into_str_map(self) -> Option<HashMap<String, Value>> {
        match self {
            Value::Map(m) => {
                let mut out = HashMap::with_capacity(m.len());
                for (k, v) in m {
                    match k {
                        MapKey::Str(s) => {
                            out.insert(s, v);
                        }
                        _ => return None,
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }

    pub fn duration_seconds(value: i64, unit: DurationUnit) -> f64 {
        match unit {
            DurationUnit::Milliseconds => value as f64 / 1000.0,
            DurationUnit::Seconds => value as f64,
            DurationUnit::Minutes => value as f64 * 60.0,
            DurationUnit::Hours => value as f64 * 3600.0,
            DurationUnit::Days => value as f64 * 86400.0,
            DurationUnit::Weeks => value as f64 * 604800.0,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::None => write!(f, "none"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            // Rendered as the literal that would rebuild it, so a set is
            // never mistaken for a list (`[…]`) or a map (`{…}`) in hover
            // text, error messages, or interpolation.
            Value::Set(items) => {
                write!(f, "set[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Range(lo, hi) => write!(f, "{lo}..{hi}"),
            Value::Map(fields) => {
                write!(f, "{{")?;
                let mut pairs: Vec<(&MapKey, &Value)> = fields.iter().collect();
                pairs.sort_by_key(|(k, _)| *k);
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Struct(_, fields) => {
                write!(f, "{{")?;
                let mut keys: Vec<&str> = fields.keys().map(|s| s.as_str()).collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {}", fields[*k])?;
                }
                write!(f, "}}")
            }
            Value::EnumVariant(ty, variant, fields) => {
                write!(f, "{ty}.{variant}")?;
                if let Some(fields) = fields
                    && !fields.is_empty()
                {
                    write!(f, " {{")?;
                    for (i, (k, v)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{k}: {v}")?;
                    }
                    write!(f, "}}")?;
                }
                Ok(())
            }
            Value::Duration(secs) => {
                if *secs >= 86400.0 {
                    write!(f, "{} days", secs / 86400.0)
                } else if *secs >= 3600.0 {
                    write!(f, "{} hours", secs / 3600.0)
                } else if *secs >= 60.0 {
                    write!(f, "{} minutes", secs / 60.0)
                } else {
                    write!(f, "{secs} seconds")
                }
            }
            Value::Uuid(id) => write!(f, "{id}"),
            Value::Task(name, _) => write!(f, "<task {name}>"),
            Value::AgentRef(name) => write!(f, "<agent {name}>"),
            Value::AgentHandlerRef(agent, handler) => write!(f, "<handler {agent}.{handler}>"),
            Value::Closure(params, _) => {
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                write!(f, "<closure ({})>", names.join(", "))
            }
            Value::Namespace(name) => write!(f, "<namespace {name}>"),
            Value::MockTarget { namespace, method } => {
                write!(f, "<mock target {namespace}.{method}>")
            }
            Value::MockHandle { namespace, method } => write!(f, "<mock {namespace}.{method}>"),
            Value::DbConnection(url, _) => write!(f, "<DbConnection {url}>"),
            Value::BuiltinFn(name) => write!(f, "<builtin {name}>"),
            Value::Module(module) => write!(f, "<module {}>", module.name),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::None, Value::None) => true,
            (Value::List(a), Value::List(b)) => a == b,
            // Order-independent: two sets built from the same elements in
            // different orders are equal, even though each preserves its own
            // insertion order for display and iteration. Both sides are
            // deduplicated, so equal lengths plus one-way containment suffice.
            (Value::Set(a), Value::Set(b)) => a.len() == b.len() && a.iter().all(|x| b.contains(x)),
            (Value::Range(a1, a2), Value::Range(b1, b2)) => a1 == b1 && a2 == b2,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Struct(_, a), Value::Struct(_, b)) => a == b,
            // Cross-comparison: a string-keyed Map equals a Struct with the same fields.
            (Value::Map(a), Value::Struct(_, b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        if let MapKey::Str(s) = k {
                            b.get(s) == Some(v)
                        } else {
                            false
                        }
                    })
            }
            (Value::Struct(_, a), Value::Map(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(&MapKey::Str(k.clone())) == Some(v))
            }
            (Value::EnumVariant(t1, v1, _), Value::EnumVariant(t2, v2, _)) => t1 == t2 && v1 == v2,
            (Value::Duration(a), Value::Duration(b)) => {
                let tol = f64::EPSILON * a.abs().max(b.abs()).max(1.0);
                (a - b).abs() < tol
            }
            (Value::Uuid(a), Value::Uuid(b)) => a == b,
            (Value::Task(a, _), Value::Task(b, _)) => a == b,
            (Value::AgentRef(a), Value::AgentRef(b)) => a == b,
            (Value::AgentHandlerRef(a1, b1), Value::AgentHandlerRef(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (Value::Namespace(a), Value::Namespace(b)) => a == b,
            (
                Value::MockTarget {
                    namespace: a_ns,
                    method: a_method,
                },
                Value::MockTarget {
                    namespace: b_ns,
                    method: b_method,
                },
            )
            | (
                Value::MockHandle {
                    namespace: a_ns,
                    method: a_method,
                },
                Value::MockHandle {
                    namespace: b_ns,
                    method: b_method,
                },
            ) => a_ns == b_ns && a_method == b_method,
            (Value::DbConnection(_, a), Value::DbConnection(_, b)) => Arc::ptr_eq(a, b),
            (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DurationUnit, Expr, LambdaBody, LambdaParam, TaskDecl};

    // ── type_name ─────────────────────────────────────────────────────

    #[test]
    fn type_name_all_variants() {
        assert_eq!(Value::Integer(1).type_name(), "int");
        assert_eq!(Value::Float(1.0).type_name(), "float");
        assert_eq!(Value::String("s".into()).type_name(), "str");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::None.type_name(), "none");
        assert_eq!(Value::List(vec![]).type_name(), "list");
        assert_eq!(Value::Range(1, 5).type_name(), "list");
        assert_eq!(
            Value::Map(std::collections::HashMap::new()).type_name(),
            "map"
        );
        assert_eq!(
            Value::EnumVariant("T".into(), "v".into(), None).type_name(),
            "enum"
        );
        assert_eq!(Value::Duration(1.0).type_name(), "duration");
        assert_eq!(
            Value::Uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479".into()).type_name(),
            "Uuid"
        );
        let td = TaskDecl {
            name: "t".into(),
            name_span: 0..0,
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: vec![],
        };
        assert_eq!(Value::Task("t".into(), Arc::new(td)).type_name(), "task");
        assert_eq!(Value::AgentRef("a".into()).type_name(), "agent");
        let p = LambdaParam {
            name: "x".into(),
            ty: None,
        };
        let b = LambdaBody::Expr(Box::new(crate::ast::Node::synthetic(Expr::Integer(0))));
        assert_eq!(Value::Closure(vec![p], Box::new(b)).type_name(), "closure");
        assert_eq!(Value::Namespace("ns".into()).type_name(), "namespace");
        assert_eq!(Value::BuiltinFn("f".into()).type_name(), "builtin");
    }

    // ── is_truthy ─────────────────────────────────────────────────────

    #[test]
    fn is_truthy_scalars() {
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(!Value::None.is_truthy());
        assert!(!Value::Integer(0).is_truthy());
        assert!(Value::Integer(1).is_truthy());
        assert!(!Value::String(String::new()).is_truthy());
        assert!(Value::String("hi".into()).is_truthy());
    }

    #[test]
    fn is_truthy_collections() {
        assert!(!Value::List(vec![]).is_truthy());
        assert!(Value::List(vec![Value::Integer(1)]).is_truthy());
        assert!(!Value::Range(5, 3).is_truthy());
        assert!(Value::Range(1, 5).is_truthy());
    }

    #[test]
    fn is_truthy_wildcard_always_true() {
        // _ => true arm: Map, Duration, EnumVariant, etc.
        assert!(Value::Map(std::collections::HashMap::new()).is_truthy());
        assert!(Value::Duration(0.0).is_truthy());
        assert!(Value::EnumVariant("T".into(), "v".into(), None).is_truthy());
    }

    // ── as_int ────────────────────────────────────────────────────────

    #[test]
    fn as_int_returns_some_for_integer() {
        assert_eq!(Value::Integer(42).as_int(), Some(42));
        assert_eq!(Value::Integer(-1).as_int(), Some(-1));
    }

    #[test]
    fn as_int_returns_none_for_non_integer() {
        assert_eq!(Value::String("42".into()).as_int(), None);
        assert_eq!(Value::None.as_int(), None);
        assert_eq!(Value::Float(1.0).as_int(), None);
    }

    #[test]
    fn display_scalars() {
        assert_eq!(format!("{}", Value::String("hello".into())), "hello");
        assert_eq!(format!("{}", Value::Integer(42)), "42");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::None), "none");
    }

    #[test]
    fn display_list_and_range() {
        let l = Value::List(vec![Value::Integer(1), Value::Integer(2)]);
        assert_eq!(format!("{l}"), "[1, 2]");
        assert_eq!(format!("{}", Value::Range(1, 5)), "1..5");
    }

    #[test]
    fn partial_eq_same_type() {
        assert_eq!(Value::Integer(1), Value::Integer(1));
        assert_ne!(Value::Integer(1), Value::Integer(2));
        assert_eq!(Value::None, Value::None);
        assert_ne!(Value::Bool(true), Value::Bool(false));
    }

    #[test]
    fn partial_eq_cross_type() {
        assert_ne!(Value::Integer(1), Value::Bool(true));
        assert_ne!(Value::None, Value::Bool(false));
    }

    #[test]
    fn partial_eq_duration() {
        assert_eq!(Value::Duration(60.0), Value::Duration(60.0));
        assert_ne!(Value::Duration(60.0), Value::Duration(61.0));
        // cross-type always false
        assert_ne!(Value::Duration(1.0), Value::Float(1.0));
    }

    #[test]
    fn partial_eq_uuid() {
        assert_eq!(
            Value::Uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479".into()),
            Value::Uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479".into())
        );
        assert_ne!(
            Value::Uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479".into()),
            Value::Uuid("00000000-0000-0000-0000-000000000000".into())
        );
    }

    #[test]
    fn partial_eq_list() {
        assert_eq!(
            Value::List(vec![Value::Integer(1), Value::Integer(2)]),
            Value::List(vec![Value::Integer(1), Value::Integer(2)]),
        );
        assert_ne!(
            Value::List(vec![Value::Integer(1)]),
            Value::List(vec![Value::Integer(2)]),
        );
        assert_eq!(Value::List(vec![]), Value::List(vec![]));
        assert_ne!(Value::List(vec![]), Value::None);
    }

    #[test]
    fn partial_eq_map() {
        let mut m1 = HashMap::new();
        m1.insert(MapKey::Str("k".into()), Value::Integer(1));
        let mut m2 = HashMap::new();
        m2.insert(MapKey::Str("k".into()), Value::Integer(1));
        let mut m3 = HashMap::new();
        m3.insert(MapKey::Str("k".into()), Value::Integer(2));
        assert_eq!(Value::Map(m1), Value::Map(m2.clone()));
        assert_ne!(Value::Map(m2), Value::Map(m3));
        assert_eq!(Value::Map(HashMap::new()), Value::Map(HashMap::new()));
    }

    #[test]
    fn partial_eq_name_based_variants() {
        assert_eq!(Value::AgentRef("A".into()), Value::AgentRef("A".into()));
        assert_ne!(Value::AgentRef("A".into()), Value::AgentRef("B".into()));
        assert_eq!(Value::Namespace("Io".into()), Value::Namespace("Io".into()));
        assert_ne!(Value::Namespace("Io".into()), Value::Namespace("Ai".into()));
        assert_eq!(
            Value::BuiltinFn("run".into()),
            Value::BuiltinFn("run".into())
        );
        assert_ne!(
            Value::BuiltinFn("run".into()),
            Value::BuiltinFn("stop".into())
        );
    }

    #[test]
    fn duration_seconds_conversion() {
        assert_eq!(Value::duration_seconds(2, DurationUnit::Minutes), 120.0);
        assert_eq!(
            Value::duration_seconds(500, DurationUnit::Milliseconds),
            0.5
        );
        assert_eq!(Value::duration_seconds(1, DurationUnit::Hours), 3600.0);
    }

    #[test]
    fn duration_seconds_remaining_units() {
        assert_eq!(Value::duration_seconds(5, DurationUnit::Seconds), 5.0);
        assert_eq!(Value::duration_seconds(1, DurationUnit::Days), 86400.0);
        assert_eq!(Value::duration_seconds(1, DurationUnit::Weeks), 604800.0);
    }

    // ── Display ───────────────────────────────────────────────────────

    #[test]
    fn display_map() {
        let mut m = HashMap::new();
        m.insert(MapKey::Str("k".into()), Value::Integer(1));
        assert_eq!(format!("{}", Value::Map(m)), "{k: 1}");
    }

    #[test]
    fn display_enum_variant_simple() {
        let v = Value::EnumVariant("Urgency".into(), "high".into(), None);
        assert_eq!(format!("{v}"), "Urgency.high");
    }

    #[test]
    fn display_enum_variant_rich() {
        let mut fields = HashMap::new();
        fields.insert("to".into(), Value::String("x".into()));
        let v = Value::EnumVariant("Action".into(), "reply".into(), Some(fields));
        assert_eq!(format!("{v}"), "Action.reply {to: x}");
    }

    #[test]
    fn display_duration_seconds() {
        assert_eq!(format!("{}", Value::Duration(5.0)), "5 seconds");
    }

    #[test]
    fn display_duration_minutes() {
        assert_eq!(format!("{}", Value::Duration(120.0)), "2 minutes");
    }

    #[test]
    fn display_duration_hours() {
        assert_eq!(format!("{}", Value::Duration(7200.0)), "2 hours");
    }

    #[test]
    fn display_duration_days() {
        assert_eq!(format!("{}", Value::Duration(172800.0)), "2 days");
    }

    #[test]
    fn display_uuid() {
        assert_eq!(
            format!(
                "{}",
                Value::Uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479".into())
            ),
            "f47ac10b-58cc-4372-a567-0e02b2c3d479"
        );
    }

    #[test]
    fn display_task() {
        let td = TaskDecl {
            name: "my_task".into(),
            name_span: 0..0,
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: vec![],
        };
        let v = Value::Task("my_task".into(), Arc::new(td));
        assert_eq!(format!("{v}"), "<task my_task>");
    }

    #[test]
    fn display_agent_ref() {
        assert_eq!(format!("{}", Value::AgentRef("Bot".into())), "<agent Bot>");
    }

    #[test]
    fn display_closure() {
        let p = LambdaParam {
            name: "x".into(),
            ty: None,
        };
        let b = LambdaBody::Expr(Box::new(crate::ast::Node::synthetic(Expr::Integer(0))));
        let v = Value::Closure(vec![p], Box::new(b));
        assert_eq!(format!("{v}"), "<closure (x)>");
    }

    #[test]
    fn display_namespace() {
        assert_eq!(
            format!("{}", Value::Namespace("Io".into())),
            "<namespace Io>"
        );
    }

    #[test]
    fn display_builtin_fn() {
        assert_eq!(
            format!("{}", Value::BuiltinFn("run".into())),
            "<builtin run>"
        );
    }
}
