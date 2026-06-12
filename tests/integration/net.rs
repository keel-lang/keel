use crate::common::*;

#[test]
fn http_get_returns_status_body_headers_and_ok_flag() {
    let url = start_single_response_server("hello http");
    let src = format!(
        r#"
use std/http
use std/io
agent A {{
    @tools [http, io]
    @on_start {{
        resp = http.get("{url}")
        header = resp.headers.get("x-keel-test") ?? "missing"
        io.show("status={{resp.status}} ok={{resp.is_ok}} body={{resp.body}} header={{header}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "http.get program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("status=200"), "{stdout}");
    assert!(stdout.contains("ok=true"), "{stdout}");
    assert!(stdout.contains("body=hello http"), "{stdout}");
    assert!(stdout.contains("header=yes"), "{stdout}");
}

#[test]
fn http_post_sends_json_body_and_reports_success() {
    let url = start_single_response_server("posted");
    let src = format!(
        r#"
use std/http
use std/io
agent A {{
    @tools [http, io]
    @on_start {{
        resp = http.post("{url}", json: {{name: "Ada", count: 2}})
        io.show("status={{resp.status}} ok={{resp.is_ok}} body={{resp.body}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "http.post program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("status=200"), "{stdout}");
    assert!(stdout.contains("ok=true"), "{stdout}");
    assert!(stdout.contains("body=posted"), "{stdout}");
}

#[test]
fn http_request_accepts_named_args_and_body_string() {
    let url = start_single_response_server("patched");
    let src = format!(
        r#"
use std/http
use std/io
agent A {{
    @tools [http, io]
    @on_start {{
        resp = http.request(method: "PATCH", url: "{url}", body: "payload")
        io.show("status={{resp.status}} body={{resp.body}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "http.request program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("status=200"), "{stdout}");
    assert!(stdout.contains("body=patched"), "{stdout}");
}

#[test]
fn http_request_rejects_unsupported_method_before_network_call() {
    let src = r#"
use std/http
agent A {
    @tools [http]
    @on_start {
        http.request(method: "TRACE", url: "http://127.0.0.1:1")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "unsupported http.request method should fail");
    assert!(
        stderr.contains("Http: unsupported method `TRACE`"),
        "expected unsupported method diagnostic:\n{stderr}"
    );
}

#[test]
fn http_request_requires_url() {
    let src = r#"
use std/http
agent A {
    @tools [http]
    @on_start {
        http.request(method: "GET")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "http.request without URL should fail");
    assert!(
        stderr.contains("http.request: missing `url`"),
        "expected missing URL diagnostic:\n{stderr}"
    );
}

#[test]
fn http_request_map_form_accepts_method_and_url() {
    let url = start_single_response_server("map-form");
    let src = format!(
        r#"
use std/http
use std/io
agent A {{
    @tools [http, io]
    @on_start {{
        resp = http.request({{method: "POST", url: "{url}", body: "map-body"}})
        io.show("status={{resp.status}} body={{resp.body}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "http.request map form failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("status=200"), "{stdout}");
    assert!(stdout.contains("body=map-form"), "{stdout}");
}

#[test]
fn http_request_map_form_requires_url() {
    let src = r#"
use std/http
agent A {
    @tools [http]
    @on_start {
        http.request({method: "GET"})
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "http.request map form without URL should fail");
    assert!(
        stderr.contains("http.request: missing `url`"),
        "expected missing URL diagnostic:\n{stderr}"
    );
}

#[test]
fn http_get_missing_url() {
    let src = r#"
use std/http
agent A {
    @tools [http]
    @on_start {
        http.get()
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "http.get without URL should fail");
    assert!(
        stderr.contains("http.get: missing argument at position 0"),
        "expected missing URL diagnostic:\n{stderr}"
    );
}

#[test]
fn http_post_missing_url() {
    let src = r#"
use std/http
agent A {
    @tools [http]
    @on_start {
        http.post(body: "hello")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "http.post without URL should fail");
    assert!(
        stderr.contains("http.post: missing argument at position 0"),
        "expected missing URL diagnostic:\n{stderr}"
    );
}

#[test]
fn http_post_body_arg_sends_string_body() {
    let url = start_single_response_server("body-arg");
    let src = format!(
        r#"
use std/http
use std/io
agent A {{
    @tools [http, io]
    @on_start {{
        resp = http.post("{url}", body: "string-body")
        io.show("status={{resp.status}} body={{resp.body}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "http.post body: arg failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("status=200"), "{stdout}");
    assert!(stdout.contains("body=body-arg"), "{stdout}");
}

#[test]
fn email_archive_without_config_is_graceful() {
    let src = r#"
use std/email
use std/io
agent A {
    @tools [email, io]
    @on_start {
        msg = {uid: 42, body: "hi", subject: "x", from: "y"}
        email.archive(msg)
        io.show("archived")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("archived"),
        "expected archive call to no-op silently:\n{stdout}"
    );
}

#[test]
fn email_send_with_config_validates_missing_body_before_transport() {
    let src = r#"
use std/email
agent A {
    @tools [email]
    @on_start {
        email.send(to: "ops@example.com")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline_with_env(
        src,
        &[
            ("IMAP_HOST", "imap.example.test"),
            ("EMAIL_USER", "bot@example.test"),
            ("EMAIL_PASS", "secret"),
        ],
    );
    assert!(!ok, "email.send without body should fail");
    assert!(
        stderr.contains("email.send: missing message body"),
        "expected missing body diagnostic:\n{stderr}"
    );
}

#[test]
fn email_send_with_config_validates_missing_recipient_before_transport() {
    let src = r#"
use std/email
agent A {
    @tools [email]
    @on_start {
        email.send("hello")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline_with_env(
        src,
        &[
            ("IMAP_HOST", "imap.example.test"),
            ("EMAIL_USER", "bot@example.test"),
            ("EMAIL_PASS", "secret"),
        ],
    );
    assert!(!ok, "email.send without recipient should fail");
    assert!(
        stderr.contains("email.send: missing `to:` argument"),
        "expected missing recipient diagnostic:\n{stderr}"
    );
}

#[test]
fn email_archive_with_config_validates_uid_before_transport() {
    let src = r#"
use std/email
agent A {
    @tools [email]
    @on_start {
        email.archive({body: "hello"})
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline_with_env(
        src,
        &[
            ("IMAP_HOST", "imap.example.test"),
            ("EMAIL_USER", "bot@example.test"),
            ("EMAIL_PASS", "secret"),
        ],
    );
    assert!(!ok, "email.archive without UID should fail");
    assert!(
        stderr.contains("email.archive: message has no UID"),
        "expected missing UID diagnostic:\n{stderr}"
    );
}
