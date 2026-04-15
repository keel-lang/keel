use std::collections::HashMap;

use crate::interpreter::value::Value;
use crate::runtime::human;
use super::bytecode::{Chunk, CompiledProgram, Op};

/// Register-based virtual machine for executing Keel bytecode.
pub struct VM {
    registers: Vec<Value>,
    variables: HashMap<String, Value>,
    /// Pending map entries for MakeMap
    map_buffer: Vec<(String, Value)>,
}

impl VM {
    pub fn new() -> Self {
        VM {
            registers: vec![Value::None; 256],
            variables: HashMap::new(),
            map_buffer: Vec::new(),
        }
    }

    /// Execute a compiled program.
    pub fn execute(&mut self, program: &CompiledProgram) -> Result<Value, String> {
        self.run_chunk(&program.main, &program.functions)
    }

    fn run_chunk(&mut self, chunk: &Chunk, functions: &[Chunk]) -> Result<Value, String> {
        let mut ip = 0usize;

        while ip < chunk.ops.len() {
            let op = &chunk.ops[ip];
            ip += 1;

            match op {
                // ── Constants ─────────────────────────────────────
                Op::LoadInt { dst, value } => {
                    self.registers[*dst as usize] = Value::Integer(*value);
                }
                Op::LoadFloat { dst, value } => {
                    self.registers[*dst as usize] = Value::Float(*value);
                }
                Op::LoadStr { dst, idx } => {
                    let s = chunk.strings.get(*idx as usize).cloned().unwrap_or_default();
                    self.registers[*dst as usize] = Value::String(s);
                }
                Op::LoadBool { dst, value } => {
                    self.registers[*dst as usize] = Value::Bool(*value);
                }
                Op::LoadNone { dst } => {
                    self.registers[*dst as usize] = Value::None;
                }

                // ── Variables ─────────────────────────────────────
                Op::GetVar { dst, name_idx } => {
                    let name = chunk.strings.get(*name_idx as usize).cloned().unwrap_or_default();
                    // Handle env.VAR
                    if let Some(var) = name.strip_prefix("env.") {
                        let val = std::env::var(var).unwrap_or_default();
                        self.registers[*dst as usize] = Value::String(val);
                    } else {
                        let val = self.variables.get(&name).cloned().unwrap_or(Value::None);
                        self.registers[*dst as usize] = val;
                    }
                }
                Op::SetVar { name_idx, src } => {
                    let name = chunk.strings.get(*name_idx as usize).cloned().unwrap_or_default();
                    self.variables.insert(name, self.registers[*src as usize].clone());
                }
                Op::GetField { dst, obj, field_idx } => {
                    let field = chunk.strings.get(*field_idx as usize).cloned().unwrap_or_default();
                    let val = match &self.registers[*obj as usize] {
                        Value::Map(fields) => fields.get(&field).cloned().unwrap_or(Value::None),
                        Value::List(items) => match field.as_str() {
                            "count" => Value::Integer(items.len() as i64),
                            "first" => items.first().cloned().unwrap_or(Value::None),
                            "last" => items.last().cloned().unwrap_or(Value::None),
                            _ => Value::None,
                        },
                        _ => Value::None,
                    };
                    self.registers[*dst as usize] = val;
                }
                Op::SetField { obj, field_idx, src } => {
                    let field = chunk.strings.get(*field_idx as usize).cloned().unwrap_or_default();
                    let val = self.registers[*src as usize].clone();
                    if let Value::Map(ref mut fields) = self.registers[*obj as usize] {
                        fields.insert(field, val);
                    }
                }

                // ── Arithmetic ────────────────────────────────────
                Op::Add { dst, a, b } => {
                    let result = binop_add(&self.registers[*a as usize], &self.registers[*b as usize]);
                    self.registers[*dst as usize] = result;
                }
                Op::Sub { dst, a, b } => {
                    let result = binop_num(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| x - y, |x, y| x - y);
                    self.registers[*dst as usize] = result;
                }
                Op::Mul { dst, a, b } => {
                    let result = binop_num(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| x * y, |x, y| x * y);
                    self.registers[*dst as usize] = result;
                }
                Op::Div { dst, a, b } => {
                    let result = binop_num(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| if y != 0 { x / y } else { 0 }, |x, y| if y != 0.0 { x / y } else { 0.0 });
                    self.registers[*dst as usize] = result;
                }
                Op::Mod { dst, a, b } => {
                    let result = binop_num(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| if y != 0 { x % y } else { 0 }, |x, y| if y != 0.0 { x % y } else { 0.0 });
                    self.registers[*dst as usize] = result;
                }
                Op::Neg { dst, src } => {
                    self.registers[*dst as usize] = match &self.registers[*src as usize] {
                        Value::Integer(n) => Value::Integer(-n),
                        Value::Float(n) => Value::Float(-n),
                        _ => Value::None,
                    };
                }

                // ── Comparison ────────────────────────────────────
                Op::Eq { dst, a, b } => {
                    self.registers[*dst as usize] = Value::Bool(self.registers[*a as usize] == self.registers[*b as usize]);
                }
                Op::Neq { dst, a, b } => {
                    self.registers[*dst as usize] = Value::Bool(self.registers[*a as usize] != self.registers[*b as usize]);
                }
                Op::Lt { dst, a, b } => {
                    self.registers[*dst as usize] = cmp_op(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| x < y);
                }
                Op::Gt { dst, a, b } => {
                    self.registers[*dst as usize] = cmp_op(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| x > y);
                }
                Op::Lte { dst, a, b } => {
                    self.registers[*dst as usize] = cmp_op(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| x <= y);
                }
                Op::Gte { dst, a, b } => {
                    self.registers[*dst as usize] = cmp_op(&self.registers[*a as usize], &self.registers[*b as usize], |x, y| x >= y);
                }

                // ── Logic ─────────────────────────────────────────
                Op::Not { dst, src } => {
                    self.registers[*dst as usize] = Value::Bool(!self.registers[*src as usize].is_truthy());
                }
                Op::And { dst, a, b } => {
                    self.registers[*dst as usize] = Value::Bool(
                        self.registers[*a as usize].is_truthy() && self.registers[*b as usize].is_truthy()
                    );
                }
                Op::Or { dst, a, b } => {
                    self.registers[*dst as usize] = Value::Bool(
                        self.registers[*a as usize].is_truthy() || self.registers[*b as usize].is_truthy()
                    );
                }

                // ── Control flow ──────────────────────────────────
                Op::Jump { offset } => {
                    ip = (ip as i32 + offset - 1) as usize;
                }
                Op::JumpIf { cond, offset } => {
                    if self.registers[*cond as usize].is_truthy() {
                        ip = (ip as i32 + offset - 1) as usize;
                    }
                }
                Op::JumpIfNot { cond, offset } => {
                    if !self.registers[*cond as usize].is_truthy() {
                        ip = (ip as i32 + offset - 1) as usize;
                    }
                }

                // ── Functions ─────────────────────────────────────
                Op::Call { dst, func_idx, arg_start, arg_count } => {
                    let idx = *func_idx as usize;
                    if idx < functions.len() {
                        // Save and set up args
                        let mut sub_vm = VM::new();
                        sub_vm.variables = self.variables.clone();
                        for i in 0..*arg_count {
                            let reg = (*arg_start + i) as usize;
                            if let Some(param_name) = functions[idx].strings.get(i as usize) {
                                sub_vm.variables.insert(param_name.clone(), self.registers[reg].clone());
                            }
                        }
                        let result = sub_vm.run_chunk(&functions[idx], functions)?;
                        self.registers[*dst as usize] = result;
                        // Merge variable changes back
                        for (k, v) in sub_vm.variables {
                            self.variables.insert(k, v);
                        }
                    }
                }
                Op::Return { src } => {
                    return Ok(self.registers[*src as usize].clone());
                }
                Op::ReturnNone => {
                    return Ok(Value::None);
                }

                // ── Data structures ───────────────────────────────
                Op::MakeList { dst, start, count } => {
                    let items: Vec<Value> = (0..*count)
                        .map(|i| self.registers[(*start + i) as usize].clone())
                        .collect();
                    self.registers[*dst as usize] = Value::List(items);
                }
                Op::MapEntry { key_idx, value } => {
                    let key = chunk.strings.get(*key_idx as usize).cloned().unwrap_or_default();
                    self.map_buffer.push((key, self.registers[*value as usize].clone()));
                }
                Op::MakeMap { dst, count: _ } => {
                    let entries: HashMap<String, Value> = self.map_buffer.drain(..).collect();
                    self.registers[*dst as usize] = Value::Map(entries);
                }

                // ── Null safety ───────────────────────────────────
                Op::IsNone { dst, src } => {
                    self.registers[*dst as usize] = Value::Bool(
                        matches!(self.registers[*src as usize], Value::None)
                    );
                }
                Op::Coalesce { dst, src, alt } => {
                    self.registers[*dst as usize] = if matches!(self.registers[*src as usize], Value::None) {
                        self.registers[*alt as usize].clone()
                    } else {
                        self.registers[*src as usize].clone()
                    };
                }

                // ── String ────────────────────────────────────────
                Op::Concat { dst, a, b } => {
                    let s = format!(
                        "{}{}",
                        self.registers[*a as usize].as_string(),
                        self.registers[*b as usize].as_string()
                    );
                    self.registers[*dst as usize] = Value::String(s);
                }
                Op::Interpolate { dst, part_count: _ } => {
                    // Handled by Concat chains
                    let _ = dst;
                }

                // ── AI primitives (delegate to runtime) ───────────
                Op::Classify { dst, .. } | Op::Summarize { dst, .. } | Op::Draft { dst, .. } => {
                    // AI ops require async — bytecode VM dispatches to tree-walker for these
                    self.registers[*dst as usize] = Value::None;
                }

                // ── Human interaction ─────────────────────────────
                Op::Notify { src } => {
                    human::notify(&self.registers[*src as usize].as_string());
                }
                Op::Show { src } => {
                    human::show(&self.registers[*src as usize]);
                }
                Op::Ask { dst, prompt } => {
                    let answer = human::ask(&self.registers[*prompt as usize].as_string());
                    self.registers[*dst as usize] = Value::String(answer);
                }
                Op::Confirm { dst, msg } => {
                    let confirmed = human::confirm(&self.registers[*msg as usize].as_string());
                    self.registers[*dst as usize] = Value::Bool(confirmed);
                }

                // ── Misc ──────────────────────────────────────────
                Op::Move { dst, src } => {
                    self.registers[*dst as usize] = self.registers[*src as usize].clone();
                }
                Op::Nop => {}
                Op::Halt => break,
            }
        }

        Ok(Value::None)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn binop_add(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Value::Integer(x + y),
        (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
        (Value::Integer(x), Value::Float(y)) => Value::Float(*x as f64 + y),
        (Value::Float(x), Value::Integer(y)) => Value::Float(x + *y as f64),
        (Value::String(x), Value::String(y)) => Value::String(format!("{x}{y}")),
        _ => Value::String(format!("{}{}", a.as_string(), b.as_string())),
    }
}

fn binop_num(a: &Value, b: &Value, int_op: fn(i64, i64) -> i64, float_op: fn(f64, f64) -> f64) -> Value {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Value::Integer(int_op(*x, *y)),
        (Value::Float(x), Value::Float(y)) => Value::Float(float_op(*x, *y)),
        (Value::Integer(x), Value::Float(y)) => Value::Float(float_op(*x as f64, *y)),
        (Value::Float(x), Value::Integer(y)) => Value::Float(float_op(*x, *y as f64)),
        _ => Value::None,
    }
}

fn cmp_op(a: &Value, b: &Value, f: fn(f64, f64) -> bool) -> Value {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Value::Bool(f(*x as f64, *y as f64)),
        (Value::Float(x), Value::Float(y)) => Value::Bool(f(*x, *y)),
        (Value::Integer(x), Value::Float(y)) => Value::Bool(f(*x as f64, *y)),
        (Value::Float(x), Value::Integer(y)) => Value::Bool(f(*x, *y as f64)),
        _ => Value::Bool(false),
    }
}
