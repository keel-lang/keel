use crate::common::*;

// ---------------------------------------------------------------------------
// v0.1.9 — Tooling: keel lint + keel check error quality
// ---------------------------------------------------------------------------

#[test]
fn lint_unused_variable_warns() {
    let src = r#"
agent A {
  @on_start {
    unused = "hello"
    Io.show("done")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "lint should exit non-zero when there are warnings");
    assert!(
        stderr.contains("unused"),
        "expected unused-variable warning:\n{stderr}"
    );
}

#[test]
fn lint_underscore_prefix_suppresses_unused_warning() {
    let src = r#"
agent A {
  @on_start {
    _ignored = "hello"
    Io.show("done")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(
        ok,
        "underscore-prefixed binding should suppress lint warning:\n{stderr}"
    );
}

#[test]
fn lint_uncalled_task_warns() {
    let src = r#"
task unused_helper() {
  Io.show("never")
}
agent A {
  @on_start {
    Io.show("start")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "lint should exit non-zero for uncalled task");
    assert!(
        stderr.contains("unused_helper"),
        "expected uncalled-task warning:\n{stderr}"
    );
}

#[test]
fn lint_called_task_no_warning() {
    let src = r#"
task greet() {
  Io.show("hi")
}
agent A {
  @on_start {
    greet()
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(ok, "called task should not produce warnings:\n{stderr}");
}

#[test]
fn lint_ai_call_outside_agent_warns() {
    let src = r#"
task process(text: str) -> str {
  result = Ai.summarize(text)
  result ?? "none"
}
agent A {
  @on_start {
    Io.show(process("hi"))
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "Ai.* outside agent should produce a warning");
    assert!(
        stderr.contains("outside an agent"),
        "expected outside-agent warning:\n{stderr}"
    );
}

#[test]
fn lint_ai_call_inside_agent_no_warning() {
    let src = r#"
agent Assistant {
  @role "helper"
  @model "ollama:llama3.2"

  @on_start {
    result = Ai.summarize("some text")
    Io.show(result ?? "none")
  }
}
run(Assistant)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(ok, "Ai.* inside agent should not warn:\n{stderr}");
}

#[test]
fn lint_state_written_not_read_warns() {
    let src = r#"
agent Sink {
  state {
    events: int = 0
  }
  on tick(n: int) {
    self.events = 42
    Io.show("ticked")
  }
}
run(Sink)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "write-only state field should produce a warning");
    assert!(
        stderr.contains("events"),
        "expected state-field warning:\n{stderr}"
    );
}

#[test]
fn lint_state_written_and_read_no_warning() {
    let src = r#"
agent Counter {
  state {
    count: int = 0
  }
  @on_start {
    self.count = self.count + 1
    Io.show("count ok")
  }
}
run(Counter)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(
        ok,
        "state field written and read should not warn:\n{stderr}"
    );
}

#[test]
fn lint_clean_program_exits_zero() {
    let (ok, _stdout, stderr) = lint_inline(
        r#"
task greet(name: str) -> str {
  msg = "Hello, " + name + "!"
  msg
}

agent Greeter {
  state {
    call_count: int = 0
  }

  @on_start {
    result = greet("World")
    Io.show(result)
    self.call_count = self.call_count + 1
    total = self.call_count
    Io.show(total)
  }
}

run(Greeter)
"#,
    );
    assert!(
        ok,
        "clean program should produce no lint warnings:\n{stderr}"
    );
}
