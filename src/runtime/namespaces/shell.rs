use std::collections::HashMap;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::{MapKey, Value};
use crate::runtime::args::{expect_str, expect_str_named};
use crate::runtime::namespace::ns;

pub(crate) const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "Shell",
    name: "run",
    params: &[BuiltinParam {
        name: "cmd",
        ty: TySpec::Str,
        optional: false,
    }],
    result: BuiltinResult::Fixed(TySpec::Str),
    doc: "Run a shell command and return its combined stdout.",
}];

pub(crate) fn namespace() -> Namespace {
    ns!("Shell", {
        "run" => |_i, args| Box::pin(async move {
            let cmd = expect_str(&args, 0, "Shell.run")?.to_owned();

            let stdin_input = expect_str_named(&args, "stdin", "Shell.run")?.map(str::to_owned);
            let cwd = expect_str_named(&args, "cwd", "Shell.run")?.map(str::to_owned);

            // Start with a clean environment to avoid leaking secrets from the
            // keel process (e.g. DATABASE_URL, API keys). Only restore a minimal
            // set of variables that most shell commands need to function normally.
            const SAFE_ENV_VARS: &[&str] = &["PATH", "HOME", "SHELL", "TMPDIR", "USER", "LANG"];

            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(&cmd)
                .env_clear()
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            for var in SAFE_ENV_VARS {
                if let Ok(val) = std::env::var(var) {
                    command.env(var, val);
                }
            }

            if let Some(ref dir) = cwd {
                command.current_dir(dir);
            }

            if stdin_input.is_some() {
                command.stdin(std::process::Stdio::piped());
            } else {
                command.stdin(std::process::Stdio::null());
            }

            let mut child = command
                .spawn()
                .map_err(|e| miette::miette!("Shell.run: failed to spawn `/bin/sh`: {e}"))?;

            if let Some(input) = stdin_input
                && let Some(mut stdin_pipe) = child.stdin.take()
            {
                stdin_pipe
                    .write_all(input.as_bytes())
                    .await
                    .map_err(|e| miette::miette!("Shell.run: failed to write stdin: {e}"))?;
            }

            let output = child
                .wait_with_output()
                .await
                .map_err(|e| miette::miette!("Shell.run: process wait failed: {e}"))?;

            let stdout_str = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit_code = output.status.code().unwrap_or(-1) as i64;

            let mut result = HashMap::new();
            result.insert(MapKey::Str("stdout".into()), Value::String(stdout_str));
            result.insert(MapKey::Str("stderr".into()), Value::String(stderr_str));
            result.insert(MapKey::Str("exit_code".into()), Value::Integer(exit_code));

            Ok(Value::Map(result))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{CallArgValue, Interpreter};

    #[test]
    fn namespace_has_run_method() {
        let ns = namespace();
        assert_eq!(ns.name, "Shell");
        assert!(ns.methods.contains_key("run"));
    }

    #[tokio::test]
    async fn run_echo_returns_stdout() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("run").expect("run method");
        let args = vec![CallArgValue {
            name: None,
            value: Value::String("echo hello".to_string()),
        }];
        let result = method(&mut interp, args).await.expect("run should succeed");
        match result {
            Value::Map(m) => {
                assert_eq!(
                    m.get(&MapKey::Str("stdout".into()))
                        .map(|v| v.to_display_string()),
                    Some("hello\n".to_string())
                );
                assert_eq!(
                    m.get(&MapKey::Str("exit_code".into())),
                    Some(&Value::Integer(0))
                );
            }
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_captures_nonzero_exit_code() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("run").expect("run method");
        let args = vec![CallArgValue {
            name: None,
            value: Value::String("exit 42".to_string()),
        }];
        let result = method(&mut interp, args).await.expect("run should succeed");
        match result {
            Value::Map(m) => {
                assert_eq!(
                    m.get(&MapKey::Str("exit_code".into())),
                    Some(&Value::Integer(42))
                );
            }
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_missing_cmd_errors() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("run").expect("run method");
        let err = method(&mut interp, vec![]).await.unwrap_err();
        assert!(
            err.to_string().contains("missing argument at position 0"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn run_cwd_changes_working_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dir_path = dir.path().to_str().expect("utf8 path").to_string();
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("run").expect("run method");
        let args = vec![
            CallArgValue {
                name: None,
                value: Value::String("pwd".to_string()),
            },
            CallArgValue {
                name: Some("cwd".to_string()),
                value: Value::String(dir_path.clone()),
            },
        ];
        let result = method(&mut interp, args).await.expect("run should succeed");
        match result {
            Value::Map(m) => {
                let stdout = m
                    .get(&MapKey::Str("stdout".into()))
                    .map(|v| v.to_display_string())
                    .unwrap_or_default();
                // macOS resolves /var/folders/... symlinks; compare canonicalized paths.
                let got = std::fs::canonicalize(stdout.trim()).expect("canonicalize stdout");
                let want = std::fs::canonicalize(&dir_path).expect("canonicalize dir");
                assert_eq!(got, want, "subprocess cwd should match requested dir");
                assert_eq!(
                    m.get(&MapKey::Str("exit_code".into())),
                    Some(&Value::Integer(0))
                );
            }
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_stdin_is_passed_to_process() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("run").expect("run method");
        let args = vec![
            CallArgValue {
                name: None,
                value: Value::String("cat".to_string()),
            },
            CallArgValue {
                name: Some("stdin".to_string()),
                value: Value::String("hello from stdin".to_string()),
            },
        ];
        let result = method(&mut interp, args).await.expect("run should succeed");
        match result {
            Value::Map(m) => {
                assert_eq!(
                    m.get(&MapKey::Str("stdout".into()))
                        .map(|v| v.to_display_string()),
                    Some("hello from stdin".to_string())
                );
            }
            other => panic!("expected map, got {other:?}"),
        }
    }
}
