use sha2::{Digest, Sha256};

use crate::interpreter::value::Value;
use crate::interpreter::{Host, Namespace};
use crate::runtime::args::expect_str;
use crate::runtime::context;
use crate::runtime::namespace::{ns, positional};

// Memory.* is only valid inside an agent body; calls from top-level code or
// plain tasks raise an error ("requires an agent context").
//
// Identity uniqueness: the directory name is <sanitized_stem>_<SHA-256[:12]> where
// the hash is computed over the OsStr bytes of the canonicalized file path.
// REPL / stdin / inline sources that have no stable on-disk path use ephemeral
// namespace names (__repl__, __stdin__, __inline__) and share storage across all
// sessions of that kind.
//
// Cross-process safety: `with_locked` uses advisory flock (fs2) on a sidecar
// <agent>.lock file. The sidecar is never renamed, so the lock target is stable
// even while the data file is being atomically replaced.
//
// Atomic writes: write to <agent>.json.tmp → fsync → rename → fsync(parent dir).
// Stale .tmp from a previous crash is removed at lock-acquisition time.
//
// Corrupt-file safety: if the JSON file fails to parse it is renamed to
// <agent>.json.bak so the user can inspect/recover the data, and an error is
// returned rather than silently starting fresh.
// ---------------------------------------------------------------------------

/// Derive the memory directory name from a raw source path string.
///
/// For real files: `<sanitized_stem>_<sha256[:12]>` where the hash is over the
/// canonical path bytes. For REPL / stdin / inline: `__repl__`, `__stdin__`,
/// `__inline__`.
#[cfg(test)]
pub(crate) fn derive_program_name(raw_source_name: &str) -> String {
    derive_program_name_with_fs(raw_source_name, &context::NativeFileSystem)
}

pub(crate) fn derive_program_name_with_fs(
    raw_source_name: &str,
    file_system: &dyn context::FileSystem,
) -> String {
    let path = std::path::Path::new(raw_source_name);
    let resolved: Option<std::path::PathBuf> = file_system.canonicalize(path).ok();
    match resolved {
        Some(p) if file_system.exists(&p) || p.is_symlink() => {
            let bytes = p.as_os_str().as_encoded_bytes();
            let mut h = Sha256::new();
            h.update(bytes);
            let hex = format!("{:x}", h.finalize());
            let hash12 = &hex[..12];
            let basename = p.file_stem().and_then(|s| s.to_str()).unwrap_or("program");
            let safe_basename = sanitize_memory_basename(basename);
            format!("{safe_basename}_{hash12}")
        }
        _ => classify_special_source(raw_source_name),
    }
}

fn sanitize_memory_basename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "program".into()
    } else {
        cleaned
    }
}

fn classify_special_source(raw: &str) -> String {
    match raw {
        "<repl>" | "repl" => "__repl__".into(),
        "<stdin>" | "stdin" => "__stdin__".into(),
        "<inline>" | "inline" => "__inline__".into(),
        other => {
            let safe = sanitize_memory_basename(other);
            format!("__{safe}__")
        }
    }
}

/// Decode the `@memory` attribute for the current agent.
/// Returns `"session"` (the default), `"persistent"`, or `"none"`.
/// Errors on any other value so programmers see a clear diagnostic.
fn memory_mode(host: &dyn Host) -> miette::Result<&'static str> {
    match host.current_memory_attr().as_deref() {
        None | Some("session") => Ok("session"),
        Some("persistent") => Ok("persistent"),
        Some("none") => Ok("none"),
        Some(other) => Err(miette::miette!(
            "Memory: unrecognized @memory value `{other}` — expected `session`, `persistent`, or `none`"
        )),
    }
}

/// Validate that a value can be round-tripped through JSON before storing it.
/// Closures, agent refs, and namespace values serialize to null and would
/// silently return `none` on recall — surface that as an error instead.
fn assert_memory_serializable(value: &Value) -> miette::Result<()> {
    match value {
        Value::Closure(..) => Err(miette::miette!(
            "Memory: cannot store a closure — only scalars, lists, and maps are supported"
        )),
        Value::Namespace(n) => Err(miette::miette!(
            "Memory: cannot store namespace `{n}` — only scalars, lists, and maps are supported"
        )),
        Value::BuiltinFn(n) => Err(miette::miette!(
            "Memory: cannot store built-in function `{n}` — only scalars, lists, and maps are supported"
        )),
        Value::AgentRef(_) => Err(miette::miette!(
            "Memory: cannot store an agent reference — only scalars, lists, and maps are supported"
        )),
        Value::Task(n, _) => Err(miette::miette!(
            "Memory: cannot store task `{n}` — only scalars, lists, and maps are supported"
        )),
        Value::Duration(_) => Err(miette::miette!(
            "Memory: cannot store a duration literal — store the numeric value (e.g. seconds as int) instead"
        )),
        _ => Ok(()),
    }
}

pub(crate) fn namespace() -> Namespace {
    ns!("Memory", {
        "remember" => |host, args| Box::pin(async move {
            let (program_name, agent_name) = host.require_agent_context("Memory.remember")?;
            let mode = memory_mode(host)?;

            let key = expect_str(&args, 0, "Memory.remember")?.to_owned();
            let value = positional(&args, 1)
                .cloned()
                .ok_or_else(|| miette::miette!("Memory.remember: missing value argument"))?;

            match mode {
                "none" => Err(miette::miette!(
                    "CapabilityError: Memory.remember is not allowed — agent has @memory none"
                )),
                "persistent" => {
                    assert_memory_serializable(&value)?;
                    let pm = host.runtime().persistent_memory.clone();
                    tokio::task::spawn_blocking(move || {
                        pm.remember(&program_name, &agent_name, key, value)
                    })
                    .await
                    .map_err(|e| miette::miette!("Memory.remember: {e}"))?
                    .map(|_| Value::None)
                }
                _ => {
                    assert_memory_serializable(&value)?;
                    host.runtime().session_memory.remember(&agent_name, key, value);
                    Ok(Value::None)
                }
            }
        }),

        "recall" => |host, args| Box::pin(async move {
            let (program_name, agent_name) = host.require_agent_context("Memory.recall")?;
            let mode = memory_mode(host)?;

            let key = expect_str(&args, 0, "Memory.recall")?.to_owned();

            match mode {
                "none" => Err(miette::miette!(
                    "CapabilityError: Memory.recall is not allowed — agent has @memory none"
                )),
                "persistent" => {
                    let pm = host.runtime().persistent_memory.clone();
                    tokio::task::spawn_blocking(move || {
                        pm.recall(&program_name, &agent_name, &key)
                    })
                    .await
                    .map_err(|e| miette::miette!("Memory.recall: {e}"))?
                }
                _ => {
                    let result = host.runtime().session_memory.recall(&agent_name, &key);
                    Ok(result)
                }
            }
        }),

        "forget" => |host, args| Box::pin(async move {
            let (program_name, agent_name) = host.require_agent_context("Memory.forget")?;
            let mode = memory_mode(host)?;

            let key = expect_str(&args, 0, "Memory.forget")?.to_owned();

            match mode {
                "none" => Err(miette::miette!(
                    "CapabilityError: Memory.forget is not allowed — agent has @memory none"
                )),
                "persistent" => {
                    let pm = host.runtime().persistent_memory.clone();
                    tokio::task::spawn_blocking(move || {
                        pm.forget(&program_name, &agent_name, &key)
                    })
                    .await
                    .map_err(|e| miette::miette!("Memory.forget: {e}"))?
                    .map(|_| Value::None)
                }
                _ => {
                    host.runtime().session_memory.forget(&agent_name, &key);
                    Ok(Value::None)
                }
            }
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_invalid_agent_name_rejected() {
        let store = context::InMemoryPersistentMemoryStore::default();
        let result = context::PersistentMemoryStore::recall(&store, "__inline__", "../evil", "key");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid agent name"),
            "expected invalid agent name error, got: {msg}"
        );
    }

    #[test]
    fn derive_program_name_repl() {
        assert_eq!(derive_program_name("<repl>"), "__repl__");
        assert_eq!(derive_program_name("repl"), "__repl__");
    }

    #[test]
    fn derive_program_name_inline() {
        assert_eq!(derive_program_name("<inline>"), "__inline__");
        assert_eq!(derive_program_name("inline"), "__inline__");
    }

    #[test]
    fn derive_program_name_nonexistent_file() {
        let result = derive_program_name("/nonexistent/path/foo.keel");
        assert!(result.starts_with("__"), "got: {result}");
    }

    #[test]
    fn derive_program_name_real_file() {
        let tmp = tempfile::NamedTempFile::new().expect("create tempfile");
        let path = tmp.path().to_str().expect("temp path is valid utf-8");
        let name = derive_program_name(path);
        let parts: Vec<&str> = name.rsplitn(2, '_').collect();
        assert_eq!(parts[0].len(), 12, "hash should be 12 chars: {name}");
        assert!(
            parts[0].chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be hex: {name}"
        );
    }

    #[test]
    fn derive_program_name_same_file_same_name() {
        let tmp = tempfile::NamedTempFile::new().expect("create tempfile");
        let path = tmp.path().to_str().expect("temp path is valid utf-8");
        assert_eq!(derive_program_name(path), derive_program_name(path));
    }
}
