//! Exit criterion for issue #145: a program declaring a named struct,
//! building it via a literal (in an annotated `let`), producing an updated
//! copy via spread-update (in `return` position), and reading fields back
//! compiles, links, and matches the interpreter byte-for-byte —
//! `examples/struct_spread_update.keel`'s shape (structs with `str` fields,
//! so this also exercises the heap-struct allocation/field-GEP path, not
//! just an all-scalar aggregate).

use std::process::Command;

use keel_codegen::BuildOptions;

#[path = "support/mod.rs"]
mod support;

fn compile_and_run(source: &str) -> std::process::Output {
    let kir = support::parse_check_and_lower(source);

    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    let bin = keel_codegen::compile(&kir, &opts).expect("compile must succeed");
    Command::new(&bin).output().expect("run compiled binary")
}

const ORDER_SOURCE: &str = r#"
use std/io

type Order {
  id: str
  status: str
  amount: float
  filled_at: str
}

task process_order(o: Order) -> Order {
  return { ...o, status: "filled", filled_at: "2024-01-15T10:30:00Z" }
}

pending: Order = { status: "pending", amount: 250.0, filled_at: "", id: "ord-42" }
filled = process_order(pending)
io.show(filled.id)
io.show(filled.status)
io.show(filled.amount)
io.show(filled.filled_at)
"#;

#[test]
fn struct_literal_field_access_and_spread_update_match_the_interpreter() {
    let compiled = compile_and_run(ORDER_SOURCE);
    let interpreted = support::run_interpreter(ORDER_SOURCE);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(
        compiled.stdout,
        b"  ord-42\n  filled\n  250\n  2024-01-15T10:30:00Z\n"
    );
}

#[test]
fn spread_update_over_multiple_generations_preserves_untouched_fields() {
    let source = r#"
use std/io

type Config {
  host: str
  port: int
  debug: bool
}

base: Config = { host: "localhost", port: 8080, debug: false }
dev = { ...base, debug: true }
prod = { ...dev, host: "api.example.com", debug: false }
io.show(prod.host)
io.show(prod.port)
io.show(prod.debug)
"#;

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  api.example.com\n  8080\n  false\n");
}
