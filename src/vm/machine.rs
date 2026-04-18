//! VM machine — placeholder for v0.1.

use super::bytecode::CompiledProgram;

pub struct VM;

impl VM {
    pub fn new() -> Self {
        VM
    }

    pub fn execute(&mut self, _program: &CompiledProgram) -> Result<(), String> {
        Err("Keel VM is not available in v0.1 alpha yet".to_string())
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}
