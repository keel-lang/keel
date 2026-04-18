//! Formatter — placeholder for v0.1.
//!
//! `keel fmt` lands in a follow-up change; for now this emits a placeholder
//! message.

use crate::ast::Program;

pub fn format_program(_program: &Program) -> String {
    "# keel fmt is not available in v0.1 alpha yet\n".to_string()
}
