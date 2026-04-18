//! VM machine — deferred post-v0.1 (see `compiler.rs`).

use super::bytecode::CompiledProgram;

pub struct VM;

impl VM {
    pub fn new() -> Self {
        VM
    }

    pub fn execute(&mut self, _program: &CompiledProgram) -> Result<(), String> {
        Err("Keel VM is deferred post-v0.1 — use `keel run` (tree-walking interpreter)".to_string())
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}
