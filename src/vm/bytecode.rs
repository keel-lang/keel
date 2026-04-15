use serde::{Deserialize, Serialize};

/// Bytecode instruction set for the Keel VM.
///
/// Register-based: instructions reference registers (r0..r255) to avoid
/// stack manipulation overhead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    // ── Constants ─────────────────────────────────────────────────
    /// Load integer constant into register
    LoadInt { dst: u8, value: i64 },
    /// Load float constant into register
    LoadFloat { dst: u8, value: f64 },
    /// Load string constant (index into string pool) into register
    LoadStr { dst: u8, idx: u32 },
    /// Load bool into register
    LoadBool { dst: u8, value: bool },
    /// Load none into register
    LoadNone { dst: u8 },

    // ── Variables ─────────────────────────────────────────────────
    /// Load variable from environment into register
    GetVar { dst: u8, name_idx: u32 },
    /// Store register value into environment variable
    SetVar { name_idx: u32, src: u8 },
    /// Load field from register (struct/map) into register
    GetField { dst: u8, obj: u8, field_idx: u32 },
    /// Set field on struct/map in register
    SetField { obj: u8, field_idx: u32, src: u8 },

    // ── Arithmetic ────────────────────────────────────────────────
    Add { dst: u8, a: u8, b: u8 },
    Sub { dst: u8, a: u8, b: u8 },
    Mul { dst: u8, a: u8, b: u8 },
    Div { dst: u8, a: u8, b: u8 },
    Mod { dst: u8, a: u8, b: u8 },
    Neg { dst: u8, src: u8 },

    // ── Comparison ────────────────────────────────────────────────
    Eq { dst: u8, a: u8, b: u8 },
    Neq { dst: u8, a: u8, b: u8 },
    Lt { dst: u8, a: u8, b: u8 },
    Gt { dst: u8, a: u8, b: u8 },
    Lte { dst: u8, a: u8, b: u8 },
    Gte { dst: u8, a: u8, b: u8 },

    // ── Logic ─────────────────────────────────────────────────────
    Not { dst: u8, src: u8 },
    And { dst: u8, a: u8, b: u8 },
    Or { dst: u8, a: u8, b: u8 },

    // ── Control flow ──────────────────────────────────────────────
    /// Unconditional jump
    Jump { offset: i32 },
    /// Jump if register is truthy
    JumpIf { cond: u8, offset: i32 },
    /// Jump if register is falsy
    JumpIfNot { cond: u8, offset: i32 },

    // ── Functions ─────────────────────────────────────────────────
    /// Call function with args from registers, store result
    Call { dst: u8, func_idx: u32, arg_start: u8, arg_count: u8 },
    /// Return value from register
    Return { src: u8 },
    /// Return none
    ReturnNone,

    // ── Data structures ───────────────────────────────────────────
    /// Create list from register range
    MakeList { dst: u8, start: u8, count: u8 },
    /// Create map/struct from key-value pairs
    MakeMap { dst: u8, count: u8 },
    /// Push key-value pair for MakeMap (key from string pool)
    MapEntry { key_idx: u32, value: u8 },

    // ── Null safety ───────────────────────────────────────────────
    /// Check if register is none, store bool
    IsNone { dst: u8, src: u8 },
    /// Null coalesce: dst = src if not none, else alt
    Coalesce { dst: u8, src: u8, alt: u8 },

    // ── String operations ─────────────────────────────────────────
    /// Concatenate registers as strings
    Concat { dst: u8, a: u8, b: u8 },
    /// String interpolation: build string from parts
    Interpolate { dst: u8, part_count: u8 },

    // ── AI primitives (high-level ops) ────────────────────────────
    /// Classify: takes input register, variant list, stores result
    Classify { dst: u8, input: u8, type_idx: u32 },
    /// Summarize: takes input register, stores result
    Summarize { dst: u8, input: u8 },
    /// Draft: takes description register, stores result
    Draft { dst: u8, desc: u8 },

    // ── Human interaction ─────────────────────────────────────────
    Notify { src: u8 },
    Show { src: u8 },
    Ask { dst: u8, prompt: u8 },
    Confirm { dst: u8, msg: u8 },

    // ── Agent operations ──────────────────────────────────────────
    /// Move register to register
    Move { dst: u8, src: u8 },
    /// No operation
    Nop,
    /// Halt VM
    Halt,
}

/// Compiled bytecode chunk for a function/task/block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub name: String,
    pub ops: Vec<Op>,
    /// String constant pool
    pub strings: Vec<String>,
    /// Number of registers needed
    pub register_count: u8,
}

impl Chunk {
    pub fn new(name: impl Into<String>) -> Self {
        Chunk {
            name: name.into(),
            ops: Vec::new(),
            strings: Vec::new(),
            register_count: 0,
        }
    }

    /// Add a string to the constant pool, return its index.
    pub fn add_string(&mut self, s: &str) -> u32 {
        if let Some(idx) = self.strings.iter().position(|existing| existing == s) {
            return idx as u32;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        idx
    }

    /// Allocate a new register, return its index.
    pub fn alloc_reg(&mut self) -> u8 {
        let reg = self.register_count;
        self.register_count = self.register_count.saturating_add(1);
        reg
    }

    pub fn emit(&mut self, op: Op) {
        self.ops.push(op);
    }

    pub fn current_offset(&self) -> usize {
        self.ops.len()
    }
}

/// A compiled Keel program — multiple chunks plus metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    /// Main chunk (top-level code)
    pub main: Chunk,
    /// Named function/task chunks
    pub functions: Vec<Chunk>,
    /// Type definitions (for runtime use)
    pub type_names: Vec<String>,
}
