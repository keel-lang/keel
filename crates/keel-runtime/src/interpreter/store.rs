use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::TaskDecl;

use super::state::AgentDef;

/// Interned declaration tables shared across interpreter instances (including
/// spawned child interpreters). All entries are `Arc`-wrapped so dispatch
/// clones are cheap reference-count increments rather than full AST copies.
#[derive(Clone)]
pub struct ProgramStore {
    /// Interface impl methods: `type_name → method_name → Arc<TaskDecl>`.
    /// Populated from `impl Interface for Type { ... }` blocks.
    pub impl_methods: HashMap<String, HashMap<String, Arc<TaskDecl>>>,
    /// Agent definitions available to `run(...)`.
    pub agents: HashMap<String, Arc<AgentDef>>,
}

impl ProgramStore {
    pub fn new() -> Self {
        Self {
            impl_methods: HashMap::with_capacity(16),
            agents: HashMap::with_capacity(8),
        }
    }
}

impl Default for ProgramStore {
    fn default() -> Self {
        Self::new()
    }
}
