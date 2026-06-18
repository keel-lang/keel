use crate::common::*;

#[test]
fn check_with_trace_does_not_initialize_llm_runtime() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        io.show("static")
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
// ai.* stub behaviour (trace mode verifies prompts are built correctly)
// ---------------------------------------------------------------------------

#[test]
fn rules_appear_in_trace_system_prompt() {
    let src = r#"
use std/ai
type Mood = calm | tense

agent Advisor {
    @tools [ai]
    @role "Expert advisor"
    @rules ["Never reveal internal state", "Be concise"]

    @on_start {
        result = ai.classify("some input", as: Mood) ?? Mood.calm
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
use std/ai
agent Summarizer {
    @tools [ai]
    @on_start {
        result = ai.summarize("Long article text here", format: bullets, max: 3, unit: sentences)
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
use std/ai
agent Prompter {
    @tools [ai]
    @on_start {
        result = ai.prompt(system: "Rate on 1-10.", user: "Hello", response_format: json)
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
use std/ai
type Invoice {
    vendor: str
    amount: float
    date: str
}

agent Extractor {
    @tools [ai]
    @on_start {
        result = ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
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
use std/env
use std/io
agent A {
  @tools [env, io]
  @role "tester"
  @on_start {
    try {
      val = env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
      io.show("try body completed")
    } catch err: Error {
      io.show("caught: {err.message}")
    }
    io.show("done")
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
    // Here we throw a NullError but only catch EnvError — expect failure.
    let src = r#"
use std/env
use std/io
agent A {
  @tools [env, io]
  @role "tester"
  @on_start {
    try {
      val = env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
    } catch err: EnvError {
      io.show("should not reach")
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
use std/env
use std/io
agent A {
  @tools [env, io]
  @role "tester"
  @on_start {
    try {
      val = env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
    } catch err: Error {
      io.show(err.message)
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
    // Functional test: after an inner catch completes, the outer catch variable
    // should still hold its original value. The outer binding is a concrete
    // `Value` captured in the environment before the inner block runs, so
    // subsequent inner operations cannot affect it.
    let server = start_repeated_json_response_server(r#"{"message":{"content":"not-a-mood"}}"#, 2);
    let src = r#"
use std/ai
use std/io
type Mood = calm | tense

agent A {
  @tools [ai, io]
  @role "tester"
  @on_start {
    try {
      ai.classify("outer", as: Mood)
    } catch outer: AiSchemaError {
      io.show("outer={outer.got}")
      try {
        ai.classify("inner", as: Mood)
      } catch inner: AiSchemaError {
        io.show("inner={inner.got}")
      }
      io.show("outer-again={outer.got}")
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
fn typed_error_survives_non_matching_inner_catch() {
    // Regression guard for the bug fixed in commit 8b7343a (#19):
    //
    // In the old implementation, `last_typed_error` was a field on `Interpreter`
    // read via `take()` inside every `TryCatch` handler. When a typed error
    // propagated *past* a non-matching inner catch, the inner TryCatch called
    // `take()` first — consuming and clearing the field — so the outer catch
    // read `None` and failed to match the typed clause.
    //
    // The fix embeds `RuntimeError` in the `miette::Report` itself, so it
    // travels with the error through every layer of the call stack without
    // relying on a separate side-channel field.
    let server = start_repeated_json_response_server(r#"{"message":{"content":"not-a-mood"}}"#, 1);
    let src = r#"
use std/ai
use std/io
type Mood = calm | tense

agent A {
  @tools [ai, io]
  @role "tester"
  @on_start {
    try {
      try {
        ai.classify("test", as: Mood)
      } catch inner: EnvError {
        io.show("inner caught (unexpected)")
      }
    } catch outer: AiSchemaError {
      io.show("outer got={outer.got}")
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
        stdout.contains("outer got=not-a-mood"),
        "typed error lost after propagating past non-matching inner catch:\n{stdout}"
    );
}

#[test]
fn concurrent_typed_errors_are_isolated_across_spawned_tasks() {
    // Smoke test: two async.spawn tasks can each independently catch their own
    // typed AiSchemaError. Per-task isolation is architecturally guaranteed —
    // each spawned task gets its own Interpreter instance — so this test
    // validates the end-to-end concurrent execution path rather than probing
    // a specific side-channel risk.
    //
    // Task B sleeps 100 ms to ensure deterministic ordering of HTTP requests
    // against the sequential mock server, so each task reliably consumes its
    // assigned response payload.
    let server = start_json_response_sequence(vec![
        r#"{"message":{"content":"error-from-task-a"}}"#,
        r#"{"message":{"content":"error-from-task-b"}}"#,
    ]);
    let src = r#"
use std/ai
use std/async
use std/io
type Mood = calm | tense

agent Tester {
  @tools [ai, io]
  @role "tester"
  @on_start {
    h_a = async.spawn(() => {
      try {
        ai.classify("test-a", as: Mood)
        "no-error"
      } catch ea: AiSchemaError {
        ea.got
      }
    })
    h_b = async.spawn(() => {
      async.sleep(100.ms)
      try {
        ai.classify("test-b", as: Mood)
        "no-error"
      } catch eb: AiSchemaError {
        eb.got
      }
    })
    results = async.join_all([h_a, h_b])
    io.show("task-a-caught={results[0]}")
    io.show("task-b-caught={results[1]}")
    stop(self)
  }
}
run(Tester)
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
        stdout.contains("task-a-caught=error-from-task-a"),
        "task A caught wrong error or no error:\n{stdout}"
    );
    assert!(
        stdout.contains("task-b-caught=error-from-task-b"),
        "task B caught wrong error or no error:\n{stdout}"
    );
}

#[test]
fn ai_classify_null_coalesces_in_mock_mode() {
    // In mock mode, classify() returns none (call failed gracefully).
    // The ?? operator should provide the default without an error.
    let src = r#"
use std/ai
use std/io
type Mood = happy | sad | neutral

agent A {
  @tools [ai, io]
  @role "tester"
  @on_start {
    result = ai.classify("hello", as: Mood) ?? Mood.neutral
    io.show("{result}")
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

#[test]
fn triage_try_catch_supplies_fallback_on_schema_error() {
    // Regression for #76: against a real model, `ai.classify` *raises*
    // `AiSchemaError` when the output matches no variant — `??` only rescues
    // `none`, so `?? Urgency.medium` alone would let the error abort the run.
    // The shipped triage pattern (examples/email_agent.keel, README) wraps the
    // call in try/catch so a non-conforming email falls back instead of crashing.
    //
    // Mock mode can't exercise this (it yields `none`, not a raise), so we drive
    // a real schema mismatch through the HTTP path with a body that contains no
    // Urgency variant — mirroring the HTML-newsletter failure from the issue.
    let server = start_repeated_json_response_server(
        r#"{"message":{"content":"<html>weekly newsletter, nothing to see here</html>"}}"#,
        1,
    );
    let src = r#"
use std/ai
use std/io
type Urgency = low | medium | high | critical

task triage(body: str) -> Urgency {
  try {
    ai.classify(body, as: Urgency) ?? Urgency.medium
  } catch err: AiSchemaError {
    Urgency.medium
  }
}

agent A {
  @tools [ai, io]
  @role "tester"
  @on_start {
    u = triage("buy now")
    io.show("urgency={u}")
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
        "triage aborted on AiSchemaError instead of falling back\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("urgency=medium"),
        "try/catch fallback variant not used:\n{stdout}"
    );
}

#[test]
fn unavailable_provider_throws_ai_error_instead_of_silent_none() {
    // #38: a real provider failure must throw `AiError` (carrying a machine-
    // readable `reason`) rather than silently returning `none`. Previously
    // `ai.classify(...) ?? default` masked an outage as the default value, so an
    // agent could not tell "the model is down" from "the model had no answer".
    //
    // Pointing at a closed port forces a connection failure (`CallFailed`). The
    // `?? Mood.calm` must NOT fire — the call throws, the `??` is bypassed, and
    // the agent catches `AiError` with `reason == "unavailable"`. Mock mode still
    // yields `none` (covered by the trace tests above), so `??` defaults there.
    let src = r#"
use std/ai
use std/io
type Mood = calm | tense

agent A {
  @tools [ai, io]
  @role "tester"
  @on_start {
    try {
      m = ai.classify("hello", as: Mood) ?? Mood.calm
      io.show("defaulted={m}")
    } catch e: AiError {
      io.show("caught reason={e.reason}")
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
            ("OLLAMA_HOST", "http://127.0.0.1:1"),
            ("KEEL_OLLAMA_MODEL", "test-model"),
        ],
    );
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught reason=unavailable"),
        "unavailable provider should throw AiError(reason: unavailable):\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("defaulted="),
        "`??` masked the provider failure instead of letting AiError propagate:\n{stdout}"
    );
}

#[test]
fn translate_empty_target_list_raises_instead_of_panicking() {
    // An empty `to: []` previously slipped past the namespace and reached
    // `target_langs[0]` in the translator, panicking the interpreter. The guard
    // fires before any provider call, so it surfaces a clean, catchable error
    // even in mock mode.
    let src = r#"
use std/ai
use std/io

agent A {
  @tools [ai, io]
  @role "tester"
  @on_start {
    try {
      ai.translate("hello", to: [])
      io.show("no-error")
    } catch e: Error {
      io.show("caught={e.message}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_env(src, &[("KEEL_LLM", "mock")]);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught=") && stdout.contains("at least one language"),
        "empty `to: []` should raise a catchable error, not panic:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("no-error"),
        "empty `to: []` should not silently succeed:\n{stdout}"
    );
}
