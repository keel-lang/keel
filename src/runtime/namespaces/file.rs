use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::value::Value;
use crate::interpreter::{Namespace, RuntimeErrorKind};
use crate::runtime::args::{expect_bool_named, expect_str};
use crate::runtime::namespace::{make_typed_report, ns};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "file",
        name: "read",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Read a file and return its contents as a string.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "write",
        params: &[
            BuiltinParam {
                name: "path",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "content",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Write a string to a file, creating or overwriting it.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "exists",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return true if the path exists on the filesystem.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "list",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfStr),
        doc: "List the entries in a directory.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "mkdir",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Create a directory and all intermediate parents.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "remove",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Remove a file or directory.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "copy",
        params: &[
            BuiltinParam {
                name: "src",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "dst",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Copy a file from src to dst.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "glob",
        params: &[BuiltinParam {
            name: "pattern",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfStr),
        doc: "Return file paths that match a glob pattern.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "move",
        params: &[
            BuiltinParam {
                name: "src",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "dst",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Move (rename) a file from src to dst.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "mktemp",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Create a temporary file and return its path.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("file", {
        "read" => |host, args| Box::pin(async move {
            let path = expect_str(&args, 0, "file.read")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let path_inner = path.clone();
            tokio::task::spawn_blocking(move || {
                fs.read_to_string(std::path::Path::new(&path_inner))
            })
            .await
            .map_err(|e| miette::miette!("file.read: {e}"))?
            .map(Value::String)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.read `{path}`: {e}")))
        }),
        "write" => |host, args| Box::pin(async move {
            let path = expect_str(&args, 0, "file.write")?.to_owned();
            let content = expect_str(&args, 1, "file.write")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let path_inner = path.clone();
            tokio::task::spawn_blocking(move || {
                fs.write_string(std::path::Path::new(&path_inner), &content)
            })
            .await
            .map_err(|e| miette::miette!("file.write: {e}"))?
            .map(|_| Value::None)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.write `{path}`: {e}")))
        }),
        "exists" => |host, args| Box::pin(async move {
            let path = expect_str(&args, 0, "file.exists")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let path_inner = path.clone();
            let exists = tokio::task::spawn_blocking(move || {
                fs.exists(std::path::Path::new(&path_inner))
            })
            .await
            .map_err(|e| miette::miette!("file.exists: {e}"))?;
            Ok(Value::Bool(exists))
        }),
        "list" => |host, args| Box::pin(async move {
            let dir_path = expect_str(&args, 0, "file.list")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let dir_inner = dir_path.clone();
            tokio::task::spawn_blocking(move || {
                fs.read_dir_names(std::path::Path::new(&dir_inner))
            })
            .await
            .map_err(|e| miette::miette!("file.list: {e}"))?
            .map(|names| Value::List(names.into_iter().map(Value::String).collect()))
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.list `{dir_path}`: {e}")))
        }),
        "mkdir" => |host, args| Box::pin(async move {
            let path = expect_str(&args, 0, "file.mkdir")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let path_inner = path.clone();
            tokio::task::spawn_blocking(move || {
                fs.mkdir(std::path::Path::new(&path_inner))
            })
            .await
            .map_err(|e| miette::miette!("file.mkdir: {e}"))?
            .map(|_| Value::None)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.mkdir `{path}`: {e}")))
        }),
        "remove" => |host, args| Box::pin(async move {
            let path = expect_str(&args, 0, "file.remove")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let path_inner = path.clone();
            tokio::task::spawn_blocking(move || {
                fs.remove(std::path::Path::new(&path_inner))
            })
            .await
            .map_err(|e| miette::miette!("file.remove: {e}"))?
            .map(|_| Value::None)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.remove `{path}`: {e}")))
        }),
        "copy" => |host, args| Box::pin(async move {
            let src = expect_str(&args, 0, "file.copy")?.to_owned();
            let dst = expect_str(&args, 1, "file.copy")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let (src_inner, dst_inner) = (src.clone(), dst.clone());
            tokio::task::spawn_blocking(move || {
                fs.copy_file(
                    std::path::Path::new(&src_inner),
                    std::path::Path::new(&dst_inner),
                )
            })
            .await
            .map_err(|e| miette::miette!("file.copy: {e}"))?
            .map(|_| Value::None)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.copy `{src}` -> `{dst}`: {e}")))
        }),
        "glob" => |host, args| Box::pin(async move {
            let pattern = expect_str(&args, 0, "file.glob")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let pat_inner = pattern.clone();
            tokio::task::spawn_blocking(move || fs.glob(&pat_inner))
            .await
            .map_err(|e| miette::miette!("file.glob: {e}"))?
            .map(|paths| Value::List(paths.into_iter().map(Value::String).collect()))
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.glob `{pattern}`: {e}")))
        }),
        "move" => |host, args| Box::pin(async move {
            let src = expect_str(&args, 0, "file.move")?.to_owned();
            let dst = expect_str(&args, 1, "file.move")?.to_owned();
            let fs = host.runtime().file_system.clone();
            let (src_inner, dst_inner) = (src.clone(), dst.clone());
            tokio::task::spawn_blocking(move || {
                fs.move_path(
                    std::path::Path::new(&src_inner),
                    std::path::Path::new(&dst_inner),
                )
            })
            .await
            .map_err(|e| miette::miette!("file.move: {e}"))?
            .map(|_| Value::None)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.move `{src}` -> `{dst}`: {e}")))
        }),
        "mktemp" => |host, args| Box::pin(async move {
            let is_dir = expect_bool_named(&args, "dir", "file.mktemp")?.unwrap_or(false);
            let fs = host.runtime().file_system.clone();
            tokio::task::spawn_blocking(move || fs.mktemp(is_dir))
            .await
            .map_err(|e| miette::miette!("file.mktemp: {e}"))?
            .map(Value::String)
            .map_err(|e| make_typed_report(RuntimeErrorKind::File, format!("file.mktemp: {e}")))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::interpreter::{CallArgValue, Interpreter};
    use crate::runtime::context::{InMemoryFileSystem, MapEnv, NativeClock, RuntimeContext};

    fn interp_with_fs(fs: InMemoryFileSystem) -> Interpreter {
        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
            Arc::new(NativeClock),
            Arc::new(fs),
        );
        Interpreter::with_runtime(ctx)
    }

    fn arg(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    #[test]
    fn namespace_has_all_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "file");
        assert!(ns.methods.contains_key("read"));
        assert!(ns.methods.contains_key("write"));
        assert!(ns.methods.contains_key("exists"));
        assert!(ns.methods.contains_key("list"));
        assert!(ns.methods.contains_key("mkdir"));
        assert!(ns.methods.contains_key("remove"));
        assert!(ns.methods.contains_key("copy"));
        assert!(ns.methods.contains_key("glob"));
        assert!(ns.methods.contains_key("move"));
        assert!(ns.methods.contains_key("mktemp"));
    }

    #[tokio::test]
    async fn write_and_read_roundtrip() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let read = ns.methods.get("read").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("data.txt".into())),
                arg(Value::String("hello".into())),
            ],
        )
        .await
        .unwrap();

        let result = read(&mut interp, vec![arg(Value::String("data.txt".into()))]).await;
        assert_eq!(result.unwrap(), Value::String("hello".into()));
    }

    #[tokio::test]
    async fn read_rejects_non_string_path() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let read = ns.methods.get("read").unwrap();

        let err = read(&mut interp, vec![arg(Value::Integer(42))])
            .await
            .expect_err("integer path must fail");
        assert_eq!(
            err.to_string(),
            "file.read: argument at position 0 must be str, got int"
        );
    }

    #[tokio::test]
    async fn write_rejects_non_string_content() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();

        let err = write(
            &mut interp,
            vec![
                arg(Value::String("data.txt".into())),
                arg(Value::Integer(42)),
            ],
        )
        .await
        .expect_err("integer content must fail");
        assert_eq!(
            err.to_string(),
            "file.write: argument at position 1 must be str, got int"
        );
    }

    #[tokio::test]
    async fn exists_returns_true_after_write() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let exists = ns.methods.get("exists").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("f.txt".into())),
                arg(Value::String("x".into())),
            ],
        )
        .await
        .unwrap();

        let result = exists(&mut interp, vec![arg(Value::String("f.txt".into()))]).await;
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let method = ns.methods.get("exists").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("nope.txt".into()))]).await;
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[tokio::test]
    async fn list_returns_directory_entries() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let list = ns.methods.get("list").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("d/a.txt".into())),
                arg(Value::String("1".into())),
            ],
        )
        .await
        .unwrap();
        write(
            &mut interp,
            vec![
                arg(Value::String("d/b.txt".into())),
                arg(Value::String("2".into())),
            ],
        )
        .await
        .unwrap();

        let result = list(&mut interp, vec![arg(Value::String("d".into()))]).await;
        match result.unwrap() {
            Value::List(items) => {
                let names: Vec<String> = items.iter().map(|v| v.to_display_string()).collect();
                assert!(names.contains(&"a.txt".to_string()));
                assert!(names.contains(&"b.txt".to_string()));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_nonexistent_file_raises_error() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let method = ns.methods.get("read").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("missing.txt".into()))]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FileError"));
    }

    #[tokio::test]
    async fn mkdir_creates_directory() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let mkdir = ns.methods.get("mkdir").unwrap();
        let exists = ns.methods.get("exists").unwrap();

        mkdir(&mut interp, vec![arg(Value::String("mydir".into()))])
            .await
            .unwrap();

        let result = exists(&mut interp, vec![arg(Value::String("mydir".into()))]).await;
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[tokio::test]
    async fn remove_deletes_file() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let remove = ns.methods.get("remove").unwrap();
        let exists = ns.methods.get("exists").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("del.txt".into())),
                arg(Value::String("bye".into())),
            ],
        )
        .await
        .unwrap();

        remove(&mut interp, vec![arg(Value::String("del.txt".into()))])
            .await
            .unwrap();

        let result = exists(&mut interp, vec![arg(Value::String("del.txt".into()))]).await;
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[tokio::test]
    async fn remove_directory_is_recursive() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let remove = ns.methods.get("remove").unwrap();
        let exists = ns.methods.get("exists").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("tree/a.txt".into())),
                arg(Value::String("a".into())),
            ],
        )
        .await
        .unwrap();
        write(
            &mut interp,
            vec![
                arg(Value::String("tree/b.txt".into())),
                arg(Value::String("b".into())),
            ],
        )
        .await
        .unwrap();

        remove(&mut interp, vec![arg(Value::String("tree".into()))])
            .await
            .unwrap();

        let a = exists(&mut interp, vec![arg(Value::String("tree/a.txt".into()))]).await;
        assert_eq!(a.unwrap(), Value::Bool(false));
        let b = exists(&mut interp, vec![arg(Value::String("tree/b.txt".into()))]).await;
        assert_eq!(b.unwrap(), Value::Bool(false));
    }

    #[tokio::test]
    async fn copy_duplicates_file() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let copy = ns.methods.get("copy").unwrap();
        let read = ns.methods.get("read").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("orig.txt".into())),
                arg(Value::String("content".into())),
            ],
        )
        .await
        .unwrap();

        copy(
            &mut interp,
            vec![
                arg(Value::String("orig.txt".into())),
                arg(Value::String("copy.txt".into())),
            ],
        )
        .await
        .unwrap();

        let result = read(&mut interp, vec![arg(Value::String("copy.txt".into()))]).await;
        assert_eq!(result.unwrap(), Value::String("content".into()));
    }

    #[tokio::test]
    async fn glob_matches_pattern() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let glob = ns.methods.get("glob").unwrap();

        for name in &["data/a.txt", "data/b.txt", "data/c.log"] {
            write(
                &mut interp,
                vec![
                    arg(Value::String((*name).into())),
                    arg(Value::String("x".into())),
                ],
            )
            .await
            .unwrap();
        }

        let result = glob(&mut interp, vec![arg(Value::String("data/*.txt".into()))]).await;
        match result.unwrap() {
            Value::List(items) => {
                let names: Vec<String> = items.iter().map(|v| v.to_display_string()).collect();
                assert!(
                    names.contains(&"data/a.txt".to_string()),
                    "missing a.txt: {names:?}"
                );
                assert!(
                    names.contains(&"data/b.txt".to_string()),
                    "missing b.txt: {names:?}"
                );
                assert!(
                    !names.contains(&"data/c.log".to_string()),
                    "should not match .log"
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn glob_empty_result_is_ok() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let glob = ns.methods.get("glob").unwrap();
        let result = glob(&mut interp, vec![arg(Value::String("*.nothing".into()))]).await;
        assert_eq!(result.unwrap(), Value::List(vec![]));
    }

    #[tokio::test]
    async fn glob_invalid_pattern_is_error() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let glob = ns.methods.get("glob").unwrap();
        // Unmatched `[` is an invalid glob pattern.
        let result = glob(&mut interp, vec![arg(Value::String("[invalid".into()))]).await;
        assert!(result.is_err(), "expected error for bad pattern");
    }

    #[tokio::test]
    async fn remove_missing_path_is_error() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let remove = ns.methods.get("remove").unwrap();
        let result = remove(&mut interp, vec![arg(Value::String("ghost.txt".into()))]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FileError"));
    }

    #[tokio::test]
    async fn move_renames_file() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let write = ns.methods.get("write").unwrap();
        let mv = ns.methods.get("move").unwrap();
        let exists = ns.methods.get("exists").unwrap();
        let read = ns.methods.get("read").unwrap();

        write(
            &mut interp,
            vec![
                arg(Value::String("src.txt".into())),
                arg(Value::String("payload".into())),
            ],
        )
        .await
        .unwrap();

        mv(
            &mut interp,
            vec![
                arg(Value::String("src.txt".into())),
                arg(Value::String("dst.txt".into())),
            ],
        )
        .await
        .unwrap();

        let src_gone = exists(&mut interp, vec![arg(Value::String("src.txt".into()))]).await;
        assert_eq!(src_gone.unwrap(), Value::Bool(false));

        let content = read(&mut interp, vec![arg(Value::String("dst.txt".into()))]).await;
        assert_eq!(content.unwrap(), Value::String("payload".into()));
    }

    #[tokio::test]
    async fn mktemp_returns_unique_paths() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let mktemp = ns.methods.get("mktemp").unwrap();

        let p1 = mktemp(&mut interp, vec![]).await.unwrap();
        let p2 = mktemp(&mut interp, vec![]).await.unwrap();
        assert_ne!(p1, p2, "mktemp must return unique paths");

        // Both paths should exist.
        let exists = ns.methods.get("exists").unwrap();
        let e1 = exists(&mut interp, vec![arg(p1.clone())]).await.unwrap();
        assert_eq!(e1, Value::Bool(true));
    }

    #[tokio::test]
    async fn mktemp_dir_creates_directory() {
        let ns = namespace();
        let mut interp = interp_with_fs(InMemoryFileSystem::new());
        let mktemp = ns.methods.get("mktemp").unwrap();
        let exists = ns.methods.get("exists").unwrap();

        let path = mktemp(
            &mut interp,
            vec![CallArgValue {
                name: Some("dir".into()),
                value: Value::Bool(true),
            }],
        )
        .await
        .unwrap();

        let e = exists(&mut interp, vec![arg(path.clone())]).await.unwrap();
        assert_eq!(e, Value::Bool(true));
    }
}
