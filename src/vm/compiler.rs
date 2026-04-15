use std::collections::HashMap;

use crate::ast::*;
use super::bytecode::{Chunk, CompiledProgram, Op};

/// Compile an AST Program into bytecode.
pub fn compile(program: &Program) -> Result<CompiledProgram, String> {
    let mut compiler = Compiler::new();
    compiler.compile_program(program)?;
    Ok(compiler.finish())
}

struct Compiler {
    main: Chunk,
    functions: Vec<Chunk>,
    /// Map task name → function index
    func_map: HashMap<String, u32>,
}

impl Compiler {
    fn new() -> Self {
        Compiler {
            main: Chunk::new("main"),
            functions: Vec::new(),
            func_map: HashMap::new(),
        }
    }

    fn finish(self) -> CompiledProgram {
        CompiledProgram {
            main: self.main,
            functions: self.functions,
            type_names: Vec::new(),
        }
    }

    fn compile_program(&mut self, program: &Program) -> Result<(), String> {
        // First pass: register all tasks as functions
        for (decl, _) in &program.declarations {
            if let Decl::Task(td) = decl {
                let func_idx = self.functions.len() as u32;
                self.func_map.insert(td.name.clone(), func_idx);
                let chunk = self.compile_task(td)?;
                self.functions.push(chunk);
            }
            if let Decl::Agent(ad) = decl {
                for item in &ad.items {
                    if let AgentItem::Task(td) = item {
                        let func_idx = self.functions.len() as u32;
                        self.func_map.insert(td.name.clone(), func_idx);
                        let chunk = self.compile_task(td)?;
                        self.functions.push(chunk);
                    }
                }
            }
        }

        // Second pass: compile agent every blocks into main
        for (decl, _) in &program.declarations {
            if let Decl::Agent(ad) = decl {
                for item in &ad.items {
                    if let AgentItem::Every(every) = item {
                        self.compile_block(&mut self.main.clone(), &every.body)?;
                    }
                }
            }
        }

        self.main.emit(Op::Halt);
        Ok(())
    }

    fn compile_task(&mut self, td: &TaskDecl) -> Result<Chunk, String> {
        let mut chunk = Chunk::new(&td.name);

        // Allocate registers for parameters
        for param in &td.params {
            let reg = chunk.alloc_reg();
            let name_idx = chunk.add_string(&param.name);
            chunk.emit(Op::SetVar { name_idx, src: reg });
        }

        self.compile_block(&mut chunk, &td.body)?;
        chunk.emit(Op::ReturnNone);
        Ok(chunk)
    }

    fn compile_block(&mut self, chunk: &mut Chunk, block: &Block) -> Result<(), String> {
        for (stmt, _) in block {
            self.compile_stmt(chunk, stmt)?;
        }
        Ok(())
    }

    fn compile_stmt(&mut self, chunk: &mut Chunk, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let reg = self.compile_expr(chunk, value)?;
                let name_idx = chunk.add_string(name);
                chunk.emit(Op::SetVar { name_idx, src: reg });
                Ok(())
            }

            Stmt::Expr(expr) => {
                self.compile_expr(chunk, expr)?;
                Ok(())
            }

            Stmt::Return(Some(expr)) => {
                let reg = self.compile_expr(chunk, expr)?;
                chunk.emit(Op::Return { src: reg });
                Ok(())
            }

            Stmt::Return(None) => {
                chunk.emit(Op::ReturnNone);
                Ok(())
            }

            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_reg = self.compile_expr(chunk, cond)?;
                let jump_else = chunk.current_offset();
                chunk.emit(Op::Nop); // placeholder

                self.compile_block(chunk, then_body)?;

                if let Some(else_b) = else_body {
                    let jump_end = chunk.current_offset();
                    chunk.emit(Op::Nop); // placeholder

                    let else_start = chunk.current_offset();
                    chunk.ops[jump_else] = Op::JumpIfNot {
                        cond: cond_reg,
                        offset: else_start as i32 - jump_else as i32,
                    };

                    self.compile_block(chunk, else_b)?;

                    let end = chunk.current_offset();
                    chunk.ops[jump_end] = Op::Jump {
                        offset: end as i32 - jump_end as i32,
                    };
                } else {
                    let end = chunk.current_offset();
                    chunk.ops[jump_else] = Op::JumpIfNot {
                        cond: cond_reg,
                        offset: end as i32 - jump_else as i32,
                    };
                }
                Ok(())
            }

            Stmt::For {
                binding,
                iter,
                body,
                ..
            } => {
                let list_reg = self.compile_expr(chunk, iter)?;
                // Simplified: emit a marker for the interpreter to handle
                let name_idx = chunk.add_string(binding);
                chunk.emit(Op::GetVar { dst: list_reg, name_idx });
                self.compile_block(chunk, body)?;
                Ok(())
            }

            Stmt::Notify { message } => {
                let reg = self.compile_expr(chunk, message)?;
                chunk.emit(Op::Notify { src: reg });
                Ok(())
            }

            Stmt::Show { value } => {
                let reg = self.compile_expr(chunk, value)?;
                chunk.emit(Op::Show { src: reg });
                Ok(())
            }

            Stmt::SelfAssign { field, value } => {
                let reg = self.compile_expr(chunk, value)?;
                let name_idx = chunk.add_string(&format!("self.{field}"));
                chunk.emit(Op::SetVar { name_idx, src: reg });
                Ok(())
            }

            // For statements we don't fully compile yet, emit Nop
            _ => {
                chunk.emit(Op::Nop);
                Ok(())
            }
        }
    }

    fn compile_expr(&mut self, chunk: &mut Chunk, expr: &Expr) -> Result<u8, String> {
        match expr {
            Expr::Integer(n) => {
                let dst = chunk.alloc_reg();
                chunk.emit(Op::LoadInt { dst, value: *n });
                Ok(dst)
            }
            Expr::Float(n) => {
                let dst = chunk.alloc_reg();
                chunk.emit(Op::LoadFloat { dst, value: *n });
                Ok(dst)
            }
            Expr::StringLit(parts) => {
                if parts.len() == 1 {
                    if let StringPart::Literal(s) = &parts[0] {
                        let dst = chunk.alloc_reg();
                        let idx = chunk.add_string(s);
                        chunk.emit(Op::LoadStr { dst, idx });
                        return Ok(dst);
                    }
                }
                // String interpolation: compile each part and concatenate
                let dst = chunk.alloc_reg();
                let idx = chunk.add_string("");
                chunk.emit(Op::LoadStr { dst, idx });
                for part in parts {
                    match part {
                        StringPart::Literal(s) => {
                            let tmp = chunk.alloc_reg();
                            let idx = chunk.add_string(s);
                            chunk.emit(Op::LoadStr { dst: tmp, idx });
                            chunk.emit(Op::Concat { dst, a: dst, b: tmp });
                        }
                        StringPart::Interpolation(e) => {
                            let tmp = self.compile_expr(chunk, e)?;
                            chunk.emit(Op::Concat { dst, a: dst, b: tmp });
                        }
                    }
                }
                Ok(dst)
            }
            Expr::Bool(b) => {
                let dst = chunk.alloc_reg();
                chunk.emit(Op::LoadBool { dst, value: *b });
                Ok(dst)
            }
            Expr::None_ => {
                let dst = chunk.alloc_reg();
                chunk.emit(Op::LoadNone { dst });
                Ok(dst)
            }
            Expr::Ident(name) => {
                let dst = chunk.alloc_reg();
                let name_idx = chunk.add_string(name);
                chunk.emit(Op::GetVar { dst, name_idx });
                Ok(dst)
            }
            Expr::BinaryOp { left, op, right } => {
                let a = self.compile_expr(chunk, left)?;
                let b = self.compile_expr(chunk, right)?;
                let dst = chunk.alloc_reg();
                let instruction = match op {
                    BinOp::Add => Op::Add { dst, a, b },
                    BinOp::Sub => Op::Sub { dst, a, b },
                    BinOp::Mul => Op::Mul { dst, a, b },
                    BinOp::Div => Op::Div { dst, a, b },
                    BinOp::Mod => Op::Mod { dst, a, b },
                    BinOp::Eq => Op::Eq { dst, a, b },
                    BinOp::Neq => Op::Neq { dst, a, b },
                    BinOp::Lt => Op::Lt { dst, a, b },
                    BinOp::Gt => Op::Gt { dst, a, b },
                    BinOp::Lte => Op::Lte { dst, a, b },
                    BinOp::Gte => Op::Gte { dst, a, b },
                    BinOp::And => Op::And { dst, a, b },
                    BinOp::Or => Op::Or { dst, a, b },
                };
                chunk.emit(instruction);
                Ok(dst)
            }
            Expr::UnaryOp { op, expr } => {
                let src = self.compile_expr(chunk, expr)?;
                let dst = chunk.alloc_reg();
                match op {
                    UnOp::Neg => chunk.emit(Op::Neg { dst, src }),
                    UnOp::Not => chunk.emit(Op::Not { dst, src }),
                }
                Ok(dst)
            }
            Expr::Call { callee, args } => {
                if let Expr::Ident(name) = callee.as_ref() {
                    let arg_start = chunk.register_count;
                    for arg in args {
                        let reg = self.compile_expr(chunk, &arg.value)?;
                        // Move to sequential registers if needed
                        if reg != chunk.register_count - 1 {
                            let target = chunk.alloc_reg();
                            chunk.emit(Op::Move { dst: target, src: reg });
                        }
                    }

                    let dst = chunk.alloc_reg();
                    if let Some(&func_idx) = self.func_map.get(name) {
                        chunk.emit(Op::Call {
                            dst,
                            func_idx,
                            arg_start,
                            arg_count: args.len() as u8,
                        });
                    } else {
                        // Built-in or unknown function — emit as named call
                        let name_idx = chunk.add_string(name);
                        chunk.emit(Op::Call {
                            dst,
                            func_idx: name_idx | 0x80000000, // flag for named lookup
                            arg_start,
                            arg_count: args.len() as u8,
                        });
                    }
                    Ok(dst)
                } else {
                    Err("Only named function calls supported in bytecode".into())
                }
            }
            Expr::NullCoalesce(left, right) => {
                let src = self.compile_expr(chunk, left)?;
                let alt = self.compile_expr(chunk, right)?;
                let dst = chunk.alloc_reg();
                chunk.emit(Op::Coalesce { dst, src, alt });
                Ok(dst)
            }
            Expr::FieldAccess(obj, field) => {
                let obj_reg = self.compile_expr(chunk, obj)?;
                let dst = chunk.alloc_reg();
                let field_idx = chunk.add_string(field);
                chunk.emit(Op::GetField { dst, obj: obj_reg, field_idx });
                Ok(dst)
            }
            Expr::StructLit(fields) => {
                let dst = chunk.alloc_reg();
                for (key, val) in fields {
                    let val_reg = self.compile_expr(chunk, val)?;
                    let key_idx = chunk.add_string(key);
                    chunk.emit(Op::MapEntry { key_idx, value: val_reg });
                }
                chunk.emit(Op::MakeMap { dst, count: fields.len() as u8 });
                Ok(dst)
            }
            Expr::ListLit(items) => {
                let start = chunk.register_count;
                for item in items {
                    self.compile_expr(chunk, item)?;
                }
                let dst = chunk.alloc_reg();
                chunk.emit(Op::MakeList { dst, start, count: items.len() as u8 });
                Ok(dst)
            }
            Expr::EnvAccess(var) => {
                let dst = chunk.alloc_reg();
                let name = format!("env.{var}");
                let idx = chunk.add_string(&name);
                chunk.emit(Op::GetVar { dst, name_idx: idx });
                Ok(dst)
            }
            Expr::SelfAccess(field) => {
                let dst = chunk.alloc_reg();
                let name = format!("self.{field}");
                let idx = chunk.add_string(&name);
                chunk.emit(Op::GetVar { dst, name_idx: idx });
                Ok(dst)
            }
            // AI primitives and complex expressions — emit placeholder
            _ => {
                let dst = chunk.alloc_reg();
                chunk.emit(Op::LoadNone { dst });
                Ok(dst)
            }
        }
    }
}
