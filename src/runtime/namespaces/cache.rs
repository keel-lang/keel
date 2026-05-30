use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::args::{expect_duration_named, expect_str};
use crate::runtime::namespace::{ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Cache", {
        "set" => |interp, args| Box::pin(async move {
            let key = expect_str(&args, 0, "Cache.set")?.to_owned();
            let value = positional(&args, 1)
                .cloned()
                .ok_or_else(|| miette::miette!("Cache.set: missing value argument"))?;

            let expires_at = expect_duration_named(&args, "ttl", "Cache.set")?
                .map(|secs| interp.runtime.clock.now_instant() + std::time::Duration::from_secs_f64(secs));

            let cache = &interp.runtime.cache;
            cache.lock().insert(key, (value, expires_at));
            Ok(Value::None)
        }),
        "get" => |interp, args| Box::pin(async move {
            let key = expect_str(&args, 0, "Cache.get")?;

            let cache = &interp.runtime.cache;
            let mut cache_lock = cache.lock();

            match cache_lock.get(key) {
                None => Ok(Value::None),
                Some((_, Some(expiry))) if interp.runtime.clock.now_instant() > *expiry => {
                    cache_lock.remove(key);
                    Ok(Value::None)
                }
                Some((v, _)) => Ok(v.clone()),
            }
        }),
        "delete" => |interp, args| Box::pin(async move {
            let key = expect_str(&args, 0, "Cache.delete")?;

            let cache = &interp.runtime.cache;
            cache.lock().remove(key);
            Ok(Value::None)
        }),
        "clear" => |interp, _args| Box::pin(async move {
            let cache = &interp.runtime.cache;
            cache.lock().clear();
            Ok(Value::None)
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::interpreter::{CallArgValue, Interpreter};
    use crate::runtime::context::{MapEnv, NativeClock, NativeFileSystem, RuntimeContext};

    fn interp_with_cache() -> Interpreter {
        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
            Arc::new(NativeClock),
            Arc::new(NativeFileSystem),
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
        assert_eq!(ns.name, "Cache");
        assert!(ns.methods.contains_key("set"));
        assert!(ns.methods.contains_key("get"));
        assert!(ns.methods.contains_key("delete"));
        assert!(ns.methods.contains_key("clear"));
    }

    #[tokio::test]
    async fn set_and_get_roundtrip() {
        let ns = namespace();
        let mut interp = interp_with_cache();
        let set = ns.methods.get("set").unwrap();
        let get = ns.methods.get("get").unwrap();

        set(
            &mut interp,
            vec![arg(Value::String("k".into())), arg(Value::Integer(42))],
        )
        .await
        .unwrap();

        let result = get(&mut interp, vec![arg(Value::String("k".into()))]).await;
        assert_eq!(result.unwrap(), Value::Integer(42));
    }

    #[tokio::test]
    async fn get_returns_none_for_missing_key() {
        let ns = namespace();
        let mut interp = interp_with_cache();
        let get = ns.methods.get("get").unwrap();
        let result = get(&mut interp, vec![arg(Value::String("missing".into()))]).await;
        assert_eq!(result.unwrap(), Value::None);
    }

    #[tokio::test]
    async fn set_rejects_non_string_key() {
        let ns = namespace();
        let mut interp = interp_with_cache();
        let set = ns.methods.get("set").unwrap();
        let err = set(
            &mut interp,
            vec![arg(Value::Integer(1)), arg(Value::String("value".into()))],
        )
        .await
        .expect_err("integer key must fail");
        assert_eq!(
            err.to_string(),
            "Cache.set: argument at position 0 must be str, got int"
        );
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let ns = namespace();
        let mut interp = interp_with_cache();
        let set = ns.methods.get("set").unwrap();
        let del = ns.methods.get("delete").unwrap();
        let get = ns.methods.get("get").unwrap();

        set(
            &mut interp,
            vec![arg(Value::String("k".into())), arg(Value::Integer(1))],
        )
        .await
        .unwrap();
        del(&mut interp, vec![arg(Value::String("k".into()))])
            .await
            .unwrap();

        assert_eq!(
            get(&mut interp, vec![arg(Value::String("k".into()))])
                .await
                .unwrap(),
            Value::None
        );
    }

    #[tokio::test]
    async fn clear_removes_all() {
        let ns = namespace();
        let mut interp = interp_with_cache();
        let set = ns.methods.get("set").unwrap();
        let clear = ns.methods.get("clear").unwrap();
        let get = ns.methods.get("get").unwrap();

        set(
            &mut interp,
            vec![arg(Value::String("a".into())), arg(Value::Integer(1))],
        )
        .await
        .unwrap();
        set(
            &mut interp,
            vec![arg(Value::String("b".into())), arg(Value::Integer(2))],
        )
        .await
        .unwrap();
        clear(&mut interp, vec![]).await.unwrap();

        assert_eq!(
            get(&mut interp, vec![arg(Value::String("a".into()))])
                .await
                .unwrap(),
            Value::None
        );
        assert_eq!(
            get(&mut interp, vec![arg(Value::String("b".into()))])
                .await
                .unwrap(),
            Value::None
        );
    }
}
