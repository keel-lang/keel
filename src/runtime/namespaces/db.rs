use crate::interpreter::Namespace;
use crate::runtime::namespace::ns;

pub(crate) fn namespace() -> Namespace {
    ns!("Db", {
        "query" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Db is planned for v0.2 and is not available in v0.1."))
        }),
        "execute" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Db is planned for v0.2 and is not available in v0.1."))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::Interpreter;

    #[test]
    fn namespace_has_query_and_execute_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Db");
        assert!(ns.methods.contains_key("query"));
        assert!(ns.methods.contains_key("execute"));
    }

    #[tokio::test]
    async fn query_returns_planned_for_v02_error() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("query").expect("query method exists");
        let err = method(&mut interp, vec![]).await.unwrap_err();
        assert!(err.to_string().contains("planned for v0.2"));
    }

    #[tokio::test]
    async fn execute_returns_planned_for_v02_error() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("execute").expect("execute method exists");
        let err = method(&mut interp, vec![]).await.unwrap_err();
        assert!(err.to_string().contains("planned for v0.2"));
    }
}
