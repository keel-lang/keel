//! Bytecode compiler — placeholder for v0.1.

use crate::ast::Program;
use super::bytecode::CompiledProgram;

pub fn compile(_program: &Program) -> Result<CompiledProgram, String> {
    Err("keel build is not available in v0.1 alpha yet — bytecode compiler \
         migration is pending (see ROADMAP.md)".to_string())
}
