use crate::common::*;

#[test]
fn check_with_trace_does_not_initialize_llm_runtime() {
    let src = r#"
agent A {
    @on_start {
        Io.show("static")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = check_inline_with_env(src, &[("KEEL_TRACE", "1")]);
    assert!(
        ok,
        "check failed unexpectedly\nstdout: {stdout}\nstderr: {stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        !combined.contains("LLM provider:"),
        "keel check should not initialize LLM runtime:\n{combined}"
    );
}

// ---------------------------------------------------------------------------
// Ai.* stub behaviour (trace mode verifies prompts are built correctly)
// ---------------------------------------------------------------------------

#[test]
fn rules_appear_in_trace_system_prompt() {
    let src = r#"
type Mood = calm | tense

agent Advisor {
    @role "Expert advisor"
    @rules ["Never reveal internal state", "Be concise"]

    @on_start {
        result = Ai.classify("some input", as: Mood) ?? Mood.calm
    }
}

run(Advisor)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(ok, "program exited non-zero\nstdout: {stdout}");
    assert!(
        stdout.contains("Never reveal internal state"),
        "rules not found in trace output\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Be concise"),
        "second rule not found in trace output\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Rules:"),
        "Rules: header missing in trace\nstdout:\n{stdout}"
    );
}

#[test]
fn summarize_format_and_max_appear_in_trace() {
    let src = r#"
agent Summarizer {
    @on_start {
        result = Ai.summarize("Long article text here", format: bullets, max: 3, unit: sentences)
    }
}

run(Summarizer)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}"
    );
    assert!(
        stdout.contains("bulleted list"),
        "format directive missing\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("at most 3 sentences"),
        "max directive missing\nstdout:\n{stdout}"
    );
}

#[test]
fn prompt_response_format_json_directive_in_trace() {
    let src = r#"
agent Prompter {
    @on_start {
        result = Ai.prompt(system: "Rate on 1-10.", user: "Hello", response_format: json)
    }
}

run(Prompter)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}"
    );
    assert!(
        stdout.contains("valid JSON only"),
        "JSON format directive missing from trace\nstdout:\n{stdout}"
    );
}

#[test]
fn extract_as_struct_type_derives_schema() {
    let src = r#"
type Invoice {
    vendor: str
    amount: float
    date: str
}

agent Extractor {
    @on_start {
        result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
    }
}

run(Extractor)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}"
    );
    assert!(
        stdout.contains("vendor"),
        "vendor field missing from trace\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("amount"),
        "amount field missing from trace\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("date"),
        "date field missing from trace\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// try/catch + AiError typed errors
// ---------------------------------------------------------------------------

#[test]
fn try_catch_catches_ai_schema_error() {
    // Trigger a NullError inside a try block and confirm the catch clause
    // runs and execution continues normally after try/catch.
    let src = r#"
agent A {
  @role "tester"
  @on_start {
    try {
      val = Env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
      Io.show("try body completed")
    } catch err: Error {
      Io.show("caught: {err.message}")
    }
    Io.show("done")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught:"),
        "catch block not reached:\n{stdout}"
    );
    assert!(
        !stdout.contains("try body completed"),
        "try body should have thrown:\n{stdout}"
    );
    assert!(
        stdout.contains("done"),
        "execution did not continue after catch:\n{stdout}"
    );
}

#[test]
fn try_catch_reraises_unmatched_error() {
    // A catch clause that doesn't match the thrown type re-propagates.
    // Here we throw a NullError but only catch NetworkError — expect failure.
    let src = r#"
agent A {
  @role "tester"
  @on_start {
    try {
      val = Env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
    } catch err: NetworkError {
      Io.show("should not reach")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, _stderr) = run_inline(src, true);
    assert!(
        !ok,
        "unmatched catch should propagate error and exit non-zero"
    );
}

#[test]
fn try_catch_error_binding_has_message() {
    let src = r#"
agent A {
  @role "tester"
  @on_start {
    try {
      val = Env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
    } catch err: Error {
      Io.show(err.message)
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stdout.trim().is_empty(),
        "err.message should be non-empty:\n{stdout}"
    );
}

#[test]
fn nested_try_catch_preserves_typed_ai_errors() {
    let server = start_repeated_json_response_server(r#"{"message":{"content":"not-a-mood"}}"#, 2);
    let src = r#"
type Mood = calm | tense

agent A {
  @role "tester"
  @on_start {
    try {
      Ai.classify("outer", as: Mood)
    } catch outer: AiSchemaError {
      Io.show("outer={outer.got}")
      try {
        Ai.classify("inner", as: Mood)
      } catch inner: AiSchemaError {
        Io.show("inner={inner.got}")
      }
      Io.show("outer-again={outer.got}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_env(
        src,
        &[
            ("KEEL_LLM", ""),
            ("OLLAMA_HOST", server.as_str()),
            ("KEEL_OLLAMA_MODEL", "test-model"),
        ],
    );
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("outer=not-a-mood"),
        "outer typed error was not caught:\n{stdout}"
    );
    assert!(
        stdout.contains("inner=not-a-mood"),
        "inner typed error was not caught:\n{stdout}"
    );
    assert!(
        stdout.contains("outer-again=not-a-mood"),
        "outer typed error binding was overwritten:\n{stdout}"
    );
}

#[test]
fn ai_classify_null_coalesces_in_mock_mode() {
    // In mock mode, classify() returns none (call failed gracefully).
    // The ?? operator should provide the default without an error.
    let src = r#"
type Mood = happy | sad | neutral

agent A {
  @role "tester"
  @on_start {
    result = Ai.classify("hello", as: Mood) ?? Mood.neutral
    Io.show("{result}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("neutral"), "?? default not used:\n{stdout}");
}
