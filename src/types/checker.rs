//! Type checker — placeholder for v0.1.
//!
//! Returns no errors for now, so programs can flow through the pipeline
//! unchecked. The full type checker will be rewritten in a follow-up
//! commit to use the new AST (interfaces, attributes, unified call shape).

use crate::ast::Program;

#[derive(Debug)]
pub struct TypeError {
    pub message: String,
    pub span: Option<crate::lexer::Span>,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn check(_program: &Program) -> Vec<TypeError> {
    Vec::new()
}
