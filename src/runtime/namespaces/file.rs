use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("File", {
        "read" => |interp, args| Box::pin(async move {
            let path = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("File.read: missing path argument"))?;
            let fs = interp.runtime.file_system.clone();
            let path_inner = path.clone();
            tokio::task::spawn_blocking(move || {
                fs.read_to_string(std::path::Path::new(&path_inner))
            })
            .await
            .map_err(|e| miette::miette!("File.read: {e}"))?
            .map(Value::String)
            .map_err(|e| miette::miette!("FileError: File.read `{path}`: {e}"))
        }),
        "write" => |interp, args| Box::pin(async move {
            let path = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("File.write: missing path argument"))?;
            let content = positional(&args, 1)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("File.write: missing content argument"))?;
            let fs = interp.runtime.file_system.clone();
            let path_inner = path.clone();
            tokio::task::spawn_blocking(move || {
                fs.write_string(std::path::Path::new(&path_inner), &content)
            })
            .await
            .map_err(|e| miette::miette!("File.write: {e}"))?
            .map(|_| Value::None)
            .map_err(|e| miette::miette!("FileError: File.write `{path}`: {e}"))
        }),
        "exists" => |interp, args| Box::pin(async move {
            let path = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("File.exists: missing path argument"))?;
            let fs = interp.runtime.file_system.clone();
            let path_inner = path.clone();
            let exists = tokio::task::spawn_blocking(move || {
                fs.exists(std::path::Path::new(&path_inner))
            })
            .await
            .map_err(|e| miette::miette!("File.exists: {e}"))?;
            Ok(Value::Bool(exists))
        }),
        "list" => |interp, args| Box::pin(async move {
            let dir_path = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("File.list: missing directory argument"))?;
            let fs = interp.runtime.file_system.clone();
            let dir_inner = dir_path.clone();
            tokio::task::spawn_blocking(move || {
                fs.read_dir_names(std::path::Path::new(&dir_inner))
            })
            .await
            .map_err(|e| miette::miette!("File.list: {e}"))?
            .map(|names| Value::List(names.into_iter().map(Value::String).collect()))
            .map_err(|e| miette::miette!("FileError: File.list `{dir_path}`: {e}"))
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
        assert_eq!(ns.name, "File");
        assert!(ns.methods.contains_key("read"));
        assert!(ns.methods.contains_key("write"));
        assert!(ns.methods.contains_key("exists"));
        assert!(ns.methods.contains_key("list"));
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
}
