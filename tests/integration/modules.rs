//! Module system end-to-end tests: `use std/<name>`, local file imports,
//! aliasing, symbol imports, implicit main, per-file tests, and the
//! migration tombstones for the removed ambient prelude.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::*;

/// Write `files` into a fresh temp dir and return (dir guard, entry path).
fn write_project(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut entry = None;
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::write(&path, content).expect("write module file");
        if entry.is_none() {
            entry = Some(path);
        }
    }
    (dir, entry.expect("at least one file"))
}

fn keel(subcommand: &str, path: &Path) -> (bool, String, String) {
    let output = Command::new(keel_binary())
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg(subcommand)
        .arg(path)
        .output()
        .expect("run keel binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ── std module imports ──────────────────────────────────────────────────────

#[test]
fn std_module_import_binds_lowercase_namespace() {
    let src = r#"
use std/io
io.show("hello modules")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("hello modules"), "stdout: {stdout}");
}

#[test]
fn std_module_alias_binds_alias_name() {
    let src = r#"
use std/io as console
console.show("aliased")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("aliased"), "stdout: {stdout}");
}

#[test]
fn std_module_call_without_import_is_an_error() {
    let src = r#"
task t() -> str {
  file.read("x.txt")
}
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "uninported module access must fail");
    assert!(
        stderr.contains("undefined") || stderr.contains("not imported"),
        "expected undefined/not-imported error:\n{stderr}"
    );
}

#[test]
fn unknown_std_module_lists_available_modules() {
    let src = "use std/nonexistent\n";
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok);
    assert!(
        stderr.contains("unknown std module"),
        "expected unknown std module error:\n{stderr}"
    );
    assert!(
        stderr.contains("file") && stderr.contains("ai"),
        "expected the list of available modules:\n{stderr}"
    );
}

#[test]
fn std_symbol_import_binds_unqualified_function() {
    let src = r#"
use std/io
use stringify from std/json
data = {name: "keel", version: 2}
io.show(stringify(data))
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("keel"), "stdout: {stdout}");
}

// ── tombstones for the removed ambient prelude ──────────────────────────────

#[test]
fn legacy_pascal_case_namespace_gets_migration_hint() {
    let src = r#"
task t() -> str {
  File.read("x.txt")
}
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok);
    assert!(
        stderr.contains("`File` is not ambient") && stderr.contains("use std/file"),
        "expected tombstone with use hint:\n{stderr}"
    );
}

#[test]
fn legacy_agent_namespace_explains_builtin_verbs() {
    let src = r#"
agent A {
  @on_start {
    Agent.broadcast("team", "hi")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok);
    assert!(
        stderr.contains("built into the language"),
        "expected agent-verb hint:\n{stderr}"
    );
}

#[test]
fn legacy_uuid_constructor_points_to_std_uuid() {
    let src = r#"
task t() {
  id = Uuid.v4()
}
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok);
    assert!(
        stderr.contains("std/uuid") && stderr.contains("uuid.v4"),
        "expected std/uuid hint:\n{stderr}"
    );
}

#[test]
fn uuid_type_annotation_still_works_with_std_uuid_module() {
    let src = r#"
use std/io
use std/uuid

task make_id() -> Uuid {
  uuid.v4()
}

id = make_id()
io.show("version {id.version()}")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("version 4"), "stdout: {stdout}");
}

// ── local file imports ──────────────────────────────────────────────────────

#[test]
fn local_import_namespaces_tasks_by_file_stem() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use std/io
use "./validation.keel"
io.show("valid: {validation.email("ada@example.com")}")
"#,
        ),
        (
            "validation.keel",
            r#"
task email(s: str) -> bool {
  s.contains("@")
}
"#,
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("valid: true"), "stdout: {stdout}");
}

#[test]
fn local_import_alias_overrides_file_stem() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use std/io
use "./validation.keel" as v
io.show("{v.email("a@b.c")}")
"#,
        ),
        (
            "validation.keel",
            "task email(s: str) -> bool { s.contains(\"@\") }\n",
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("true"), "stdout: {stdout}");
}

#[test]
fn implicit_main_skips_imported_top_level_statements() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use std/io
use "./helper.keel"
io.show("from main: {helper.value()}")
"#,
        ),
        (
            "helper.keel",
            r#"
use std/io
task value() -> int { 7 }
io.show("HELPER MAIN RAN")
"#,
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("from main: 7"), "stdout: {stdout}");
    assert!(
        !stdout.contains("HELPER MAIN RAN"),
        "imported top-level statements must not execute:\n{stdout}"
    );
}

#[test]
fn module_file_runs_its_own_implicit_main_when_executed_directly() {
    let (_dir, entry) = write_project(&[(
        "helper.keel",
        r#"
use std/io
task value() -> int { 7 }
io.show("HELPER MAIN RAN")
"#,
    )]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("HELPER MAIN RAN"), "stdout: {stdout}");
}

#[test]
fn symbol_import_binds_task_and_type_unqualified() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use std/io
use classify, Urgency from "./models.keel"

u = classify("a very long subject line")
when u {
  low => { io.show("low") }
  high => { io.show("high") }
}
"#,
        ),
        (
            "models.keel",
            r#"
type Urgency = low | high

task classify(s: str) -> Urgency {
  if s.len() > 10 { Urgency.high } else { Urgency.low }
}
"#,
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("high"), "stdout: {stdout}");
}

#[test]
fn symbol_import_alias_renames_task() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use std/io
use email as is_email from "./validation.keel"
io.show("{is_email("a@b.c")}")
"#,
        ),
        (
            "validation.keel",
            "task email(s: str) -> bool { s.contains(\"@\") }\n",
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("true"), "stdout: {stdout}");
}

#[test]
fn imported_agent_starts_via_module_namespace() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./watchers.keel"
run(watchers.Watcher)
send(watchers.Watcher, "ping")
"#,
        ),
        (
            "watchers.keel",
            r#"
use std/io
agent Watcher {
  @tools [io]
  on message(data: str) {
    io.show("got {data}")
    stop(self)
  }
}
"#,
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("got ping"), "stdout: {stdout}");
}

#[test]
fn imported_agent_symbol_form_starts_unqualified() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use Watcher from "./watchers.keel"
run(Watcher)
send(Watcher, "direct")
"#,
        ),
        (
            "watchers.keel",
            r#"
use std/io
agent Watcher {
  @tools [io]
  on message(data: str) {
    io.show("got {data}")
    stop(self)
  }
}
"#,
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("got direct"), "stdout: {stdout}");
}

#[test]
fn interface_impl_crosses_module_boundary() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use std/io
use Point from "./shapes.keel"

p: Point = { x: 3, y: 4 }
io.show(p.to_str())
"#,
        ),
        (
            "shapes.keel",
            r#"
type Point { x: int, y: int }

impl Stringable for Point {
  task to_str(self) -> str {
    "({self.x}, {self.y})"
  }
}
"#,
        ),
    ]);
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("(3, 4)"), "stdout: {stdout}");
}

// ── visibility and collisions ───────────────────────────────────────────────

#[test]
fn foreign_type_annotation_requires_symbol_import() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./models.keel"

task f(c: Classifier) -> str {
  c.name
}
"#,
        ),
        ("models.keel", "type Classifier { name: str }\n"),
    ]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok, "foreign type without import must fail");
    assert!(
        stderr.contains("declared in another module"),
        "expected visibility error with import hint:\n{stderr}"
    );
}

#[test]
fn duplicate_import_binding_is_rejected_with_alias_hint() {
    let (dir, _) = write_project(&[
        ("strings.keel", "task s() -> str { \"local\" }\n"),
        (
            "main.keel",
            r#"
use "./strings.keel"
use "./util/strings.keel"
"#,
        ),
    ]);
    std::fs::create_dir(dir.path().join("util")).expect("mkdir util");
    std::fs::write(
        dir.path().join("util/strings.keel"),
        "task t() -> str { \"util\" }\n",
    )
    .expect("write util module");
    let (ok, _stdout, stderr) = keel("check", &dir.path().join("main.keel"));
    assert!(!ok, "duplicate binding must fail");
    assert!(
        stderr.contains("alias") || stderr.contains("more than one import"),
        "expected duplicate-binding error with alias hint:\n{stderr}"
    );
}

#[test]
fn declaration_colliding_with_import_is_rejected() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./validation.keel"

task validation() -> str { "shadow" }
"#,
        ),
        ("validation.keel", "task email(s: str) -> bool { true }\n"),
    ]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok, "decl/import collision must fail");
    assert!(
        stderr.contains("also bound by an import"),
        "expected decl-vs-import collision error:\n{stderr}"
    );
}

#[test]
fn same_task_name_in_two_modules_is_rejected() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./a.keel"
use "./b.keel"
"#,
        ),
        ("a.keel", "task helper() -> int { 1 }\n"),
        ("b.keel", "task helper() -> int { 2 }\n"),
    ]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok, "cross-module duplicate task name must fail");
    assert!(
        stderr.contains("two different things"),
        "expected flat-namespace conflict error:\n{stderr}"
    );
}

#[test]
fn circular_import_reports_cycle_path() {
    let (_dir, entry) = write_project(&[
        ("main.keel", "use \"./a.keel\"\n"),
        ("a.keel", "use \"./b.keel\"\n"),
        ("b.keel", "use \"./a.keel\"\n"),
    ]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok, "cycle must fail");
    assert!(
        stderr.contains("circular import"),
        "expected circular import error:\n{stderr}"
    );
    assert!(
        stderr.contains("a.keel") && stderr.contains("b.keel"),
        "cycle path should name the files:\n{stderr}"
    );
}

#[test]
fn missing_import_file_reports_path() {
    let (_dir, entry) = write_project(&[("main.keel", "use \"./missing.keel\"\n")]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok);
    assert!(stderr.contains("missing.keel"), "stderr: {stderr}");
}

#[test]
fn module_binding_is_not_callable() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./validation.keel"
validation()
"#,
        ),
        ("validation.keel", "task email(s: str) -> bool { true }\n"),
    ]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok);
    assert!(
        stderr.contains("is a module, not a function"),
        "expected module-not-callable error:\n{stderr}"
    );
}

#[test]
fn unknown_module_member_is_rejected_statically() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./validation.keel"
task t() -> bool {
  validation.nope("x")
}
"#,
        ),
        ("validation.keel", "task email(s: str) -> bool { true }\n"),
    ]);
    let (ok, _stdout, stderr) = keel("check", &entry);
    assert!(!ok);
    assert!(
        stderr.contains("no member `nope`"),
        "expected unknown-member error:\n{stderr}"
    );
}

// ── tests and modules ───────────────────────────────────────────────────────

#[test]
fn keel_test_runs_only_the_target_files_tests() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use "./helper.keel"

test "entry test" {
  assert helper.double(3) == 6
}
"#,
        ),
        (
            "helper.keel",
            r#"
task double(x: int) -> int { x * 2 }

test "helper test" {
  assert double(1) == 2
}
"#,
        ),
    ]);
    let (ok, _stdout, stderr) = keel("test", &entry);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("entry test"), "stderr: {stderr}");
    assert!(
        !stderr.contains("helper test"),
        "imported module tests must not run:\n{stderr}"
    );
    assert!(stderr.contains("1 test passed"), "stderr: {stderr}");
}

#[test]
fn imported_module_provides_test_helpers_as_plain_tasks() {
    let (_dir, entry) = write_project(&[
        (
            "main.keel",
            r#"
use make_fixture from "./fixtures.keel"

test "uses imported helper" {
  f = make_fixture()
  assert f.name == "fixture"
}
"#,
        ),
        (
            "fixtures.keel",
            r#"
type Fixture { name: str }

task make_fixture() -> Fixture {
  { name: "fixture" }
}
"#,
        ),
    ]);
    let (ok, _stdout, stderr) = keel("test", &entry);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("1 test passed"), "stderr: {stderr}");
}

#[test]
fn testing_module_still_mocks_std_methods() {
    let src = r#"
use std/testing
use std/file

task load() -> str {
  file.read("config.json")
}

test "mocked read" {
  setup {
    testing.mock(file.read).returns("mocked!")
  }
  assert load() == "mocked!"
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("1 test passed"), "stderr: {stderr}");
}

// ── formatter round-trip ────────────────────────────────────────────────────

#[test]
fn formatter_preserves_all_use_forms() {
    let src = r#"use std/io
use std/file as f
use "./validation.keel"
use "./util.keel" as u
use email, helper as h from "./validation.keel"
use parse from std/json

task t() {
}
"#;
    let formatted = keel_lang::session::fmt_source(src, "t.keel").expect("format");
    assert!(formatted.contains("use std/io\n"), "got:\n{formatted}");
    assert!(
        formatted.contains("use std/file as f\n"),
        "got:\n{formatted}"
    );
    assert!(
        formatted.contains("use \"./validation.keel\"\n"),
        "got:\n{formatted}"
    );
    assert!(
        formatted.contains("use \"./util.keel\" as u\n"),
        "got:\n{formatted}"
    );
    assert!(
        formatted.contains("use email, helper as h from \"./validation.keel\"\n"),
        "got:\n{formatted}"
    );
    assert!(
        formatted.contains("use parse from std/json\n"),
        "got:\n{formatted}"
    );
}

// ── examples carry their own test blocks ────────────────────────────────────

#[test]
fn every_example_with_test_blocks_passes_keel_test() {
    let examples = project_root().join("examples");
    let mut files: Vec<PathBuf> = Vec::new();
    collect_keel_files(&examples, &mut files);
    files.sort();
    let mut ran = 0_usize;
    for file in files {
        let source = std::fs::read_to_string(&file).expect("read example");
        if !source.contains("test \"") {
            continue;
        }
        let (ok, _stdout, stderr) = keel("test", &file);
        assert!(ok, "keel test {} failed:\n{stderr}", file.display());
        ran += 1;
    }
    assert!(
        ran >= 30,
        "expected ≥30 examples with test blocks, found {ran}"
    );
}

fn collect_keel_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read examples dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_keel_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("keel") {
            out.push(path);
        }
    }
}

// ── multi-file example ──────────────────────────────────────────────────────

#[test]
fn inbox_modules_example_tests_run_per_file() {
    let entry = project_root().join("examples/inbox_modules/validation_test.keel");
    let (ok, _stdout, stderr) = keel("test", &entry);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("5 tests passed"), "stderr: {stderr}");
    // validation.keel's own test blocks must not run transitively.
    assert!(
        !stderr.contains("accepts well-formed addresses"),
        "imported module tests must not run:\n{stderr}"
    );

    let module = project_root().join("examples/inbox_modules/validation.keel");
    let (ok, _stdout, stderr) = keel("test", &module);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("2 tests passed"), "stderr: {stderr}");
}

#[test]
fn inbox_modules_example_runs() {
    let entry = project_root().join("examples/inbox_modules/main.keel");
    let (ok, stdout, stderr) = keel("run", &entry);
    assert!(ok, "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("2 of 3 addresses are valid"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("valid: ada@example.com"),
        "stdout: {stdout}"
    );
}
