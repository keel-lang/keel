//! Bytecode — placeholder for v0.1.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub ops: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    pub main: Chunk,
    pub functions: Vec<Chunk>,
}
