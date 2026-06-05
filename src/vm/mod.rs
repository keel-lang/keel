//! VM — placeholder module for v0.1.
//!
//! `keel build` / `.keelc` execution is deferred: v0.1 ships with the
//! tree-walking interpreter only. A bytecode compiler and register-based
//! VM will land in a later release.

pub mod bytecode;
pub mod compiler;
pub mod machine;

#[cfg(test)]
mod tests {
    use crate::lexer::lex;
    use crate::parser::parse;
    use crate::vm::{bytecode, compiler, machine};
    use miette::NamedSource;

    #[test]
    fn vm_compiler_and_machine_fail_loudly_while_bytecode_is_deferred() {
        let source = r#"task answer() -> int { 42 }"#;
        let named = NamedSource::new("test.keel", source.to_string());
        let tokens = lex(source, &named).expect("lex source");
        let program = parse(tokens, source.len(), &named).expect("parse source");

        let compile_err = compiler::compile(&program).expect_err("compiler should be deferred");
        let mut machine = machine::VM::new();
        let execute_err = machine
            .execute(&bytecode::CompiledProgram::default())
            .expect_err("machine should be deferred");

        assert!(
            compile_err.contains("keel build is deferred"),
            "unexpected compiler error: {compile_err}"
        );
        assert!(
            execute_err.contains("Keel VM is deferred"),
            "unexpected VM error: {execute_err}"
        );
    }
}
