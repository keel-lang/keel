// Shared helpers for integration tests.
// Cargo treats tests/common/ as a special path — it's available to all test
// binaries as `mod common` without becoming a test binary itself.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn keel_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_keel"))
}

pub fn run_example(name: &str) -> (bool, String, String) {
    let bin = keel_binary();
    let example = project_root().join("examples").join(format!("{name}.keel"));
    let output = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&example)
        .output()
        .expect("failed to run keel binary");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

#[allow(dead_code)]
pub fn check_example(name: &str) -> bool {
    let bin = keel_binary();
    let example = project_root().join("examples").join(format!("{name}.keel"));
    Command::new(&bin)
        .arg("check")
        .arg(&example)
        .status()
        .expect("failed to run keel check")
        .success()
}

pub fn lint_inline(src: &str) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let output = Command::new(&bin)
        .arg("lint")
        .arg(&path)
        .output()
        .expect("run keel lint");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

pub fn check_inline_output(src: &str) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let output = Command::new(&bin)
        .arg("check")
        .arg(&path)
        .output()
        .expect("run keel check");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

pub fn check_inline_with_env(src: &str, envs: &[(&str, &str)]) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let mut cmd = Command::new(&bin);
    cmd.arg("check").arg(&path);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run keel check");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

pub fn run_inline(src: &str, trace: bool) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let mut cmd = Command::new(&bin);
    cmd.env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&path);
    if trace {
        cmd.env("KEEL_TRACE", "1");
    }
    let output = cmd.output().expect("run keel");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

pub fn run_inline_with_env(src: &str, envs: &[(&str, &str)]) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let mut cmd = Command::new(&bin);
    cmd.env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&path);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run keel");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

pub fn run_inline_with_stdin(src: &str, stdin_text: &str) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let mut child = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run keel with stdin");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin_text.as_bytes())
        .expect("write child stdin");
    let output = child.wait_with_output().expect("wait for keel");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

pub fn keel_string_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn start_single_response_server(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
    let address = listener.local_addr().expect("read test HTTP address");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test HTTP request");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("read test HTTP request");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\nx-keel-test: yes\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write test HTTP response");
    });
    format!("http://{address}")
}

pub fn start_repeated_json_response_server(body: &'static str, request_count: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
    let address = listener.local_addr().expect("read test HTTP address");
    thread::spawn(move || {
        for _ in 0..request_count {
            let (mut stream, _) = listener.accept().expect("accept test HTTP request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read test HTTP request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write test HTTP response");
        }
    });
    format!("http://{address}")
}
