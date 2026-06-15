use crate::interpreter::Namespace;
use crate::runtime::namespace::ns;

pub(crate) fn namespace() -> Namespace {
    ns!("search", {
        "web" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Search is planned for v0.2 and is not available in v0.1."))
        }),
        "news" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Search is planned for v0.2 and is not available in v0.1."))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::Interpreter;

    #[test]
    fn namespace_has_web_and_news_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "search");
        assert!(ns.methods.contains_key("web"));
        assert!(ns.methods.contains_key("news"));
    }

    #[tokio::test]
    async fn web_returns_planned_for_v02_error() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("web").expect("web method exists");
        let err = method(&mut interp, vec![]).await.unwrap_err();
        assert!(err.to_string().contains("planned for v0.2"));
    }

    #[tokio::test]
    async fn news_returns_planned_for_v02_error() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let method = ns.methods.get("news").expect("news method exists");
        let err = method(&mut interp, vec![]).await.unwrap_err();
        assert!(err.to_string().contains("planned for v0.2"));
    }
}
