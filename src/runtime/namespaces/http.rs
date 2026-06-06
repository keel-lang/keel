use std::collections::HashMap;
use std::sync::LazyLock;

use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::value::{MapKey, Value};
use crate::interpreter::{Namespace, RuntimeErrorKind};
use crate::runtime::args::{expect_int, expect_str, expect_str_value};
use crate::runtime::convert::value_to_json;
use crate::runtime::namespace::{find_arg, make_typed_report, ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "Http",
        name: "get",
        params: &[BuiltinParam {
            name: "url",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP GET request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "Http",
        name: "post",
        params: &[
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: true,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP POST request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "Http",
        name: "request",
        params: &[
            BuiltinParam {
                name: "method",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP request with full control and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "Http",
        name: "serve",
        params: &[BuiltinParam {
            name: "port",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Start an HTTP server on the given port.",
    },
];

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

pub(crate) fn namespace() -> Namespace {
    ns!("Http", {
        "get" => |_i, args| Box::pin(async move {
            let url = expect_str(&args, 0, "Http.get")?;
            let headers = map_from_arg(find_arg(&args, "headers"), "Http.get")?;
            let response = http_send("GET", url, headers, None).await?;
            Ok(response)
        }),
        "post" => |_i, args| Box::pin(async move {
            let url = expect_str(&args, 0, "Http.post")?;
            let headers = map_from_arg(find_arg(&args, "headers"), "Http.post")?;
            let body = request_body(
                find_arg(&args, "json"),
                find_arg(&args, "body"),
                "Http.post",
            )?;
            let response = http_send("POST", url, headers, body).await?;
            Ok(response)
        }),
        "request" => |_i, args| Box::pin(async move {
            // Accepts a single map argument with keys `method`, `url`,
            // `headers`, `body`, `json`.
            let cfg = match positional(&args, 0) {
                Some(Value::Map(m)) => m.clone(),
                _ => {
                    // Also accept direct named args.
                    let mut m = HashMap::new();
                    for a in &args {
                        if let Some(n) = &a.name {
                            m.insert(MapKey::Str(n.clone()), a.value.clone());
                        }
                    }
                    m
                }
            };
            let method = cfg
                .get(&MapKey::Str("method".into()))
                .map(|v| expect_str_value(v, "`method`", "Http.request"))
                .transpose()?
                .unwrap_or("GET");
            let url = cfg
                .get(&MapKey::Str("url".into()))
                .map(|v| expect_str_value(v, "`url`", "Http.request"))
                .transpose()?
                .ok_or_else(|| miette::miette!("Http.request: missing `url`"))?;
            let headers = cfg
                .get(&MapKey::Str("headers".into()))
                .map(|v| map_from_arg(Some(v), "Http.request"))
                .transpose()?
                .unwrap_or_default();
            let body = request_body(
                cfg.get(&MapKey::Str("json".into())),
                cfg.get(&MapKey::Str("body".into())),
                "Http.request",
            )?;
            http_send(method, url, headers, body).await
        }),
        // Http.serve(port, handler) — start an HTTP server on the given port.
        // The handler closure receives a request map with {method, path, body}
        // and should return a response map with {status, body}.
        //
        // IMPORTANT: handlers run OUTSIDE any agent context. The closure is
        // registered with the sentinel name `"__http_serve__"` and fired via
        // `Event::FireClosureWithArgs`, which calls `call_closure` directly
        // without setting `self.current_agent`. Concretely:
        //   - `self.<field>` from inside a handler raises a runtime error
        //     (no current agent).
        //   - `Ai.*` calls work, but with no agent `@role`, no `@rules`,
        //     and the model defaults to `KEEL_OLLAMA_MODEL` (no `@model`
        //     injection). For agent-aware behaviour, dispatch into a live
        //     agent via `Agent.send(MyAgent, data, event: "http_request")`.
        // See `docs/src/guide/connections.md` for the user-facing callout.
        "serve" => |host, args| Box::pin(async move {
            let port = match positional(&args, 0) {
                None => 8080u16,
                Some(_) => {
                    let port = expect_int(&args, 0, "Http.serve")?;
                    u16::try_from(port)
                        .ok()
                        .filter(|port| *port > 0)
                        .ok_or_else(|| miette::miette!(
                            "Http.serve: port must be an integer in the range 1..=65535"
                        ))?
                }
            };

            // Extract closure from args
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Http.serve: missing closure argument"))?;

            let closure_id = host.register_closure("__http_serve__".to_string(), params, body);
            let event_tx = host.background_event_tx();
            let server_counter = host.active_http_servers().clone();

            server_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            tokio::spawn(async move {
                use axum::{Router, routing::any, extract::Request, response::{Response, IntoResponse}, body::Body};

                let app = Router::new().fallback(any(move |req: Request<Body>| {
                    let tx = event_tx.clone();
                    async move {
                        let method = req.method().as_str().to_string();
                        let path = req.uri().path().to_string();
                        let (_, body) = req.into_parts();
                        let body_bytes = axum::body::to_bytes(body, 1_048_576).await.unwrap_or_default();
                        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                        let req_json = serde_json::json!({
                            "method": method,
                            "path": path,
                            "body": body_str,
                        }).to_string();

                        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<String>();
                        if tx.try_send(crate::interpreter::Event::FireClosureWithArgs {
                            closure_id,
                            request_json: req_json,
                            response_tx: resp_tx,
                        }).is_err() {
                            return Response::builder()
                                .status(503)
                                .body(Body::from(r#"{"status":503,"body":"server busy"}"#))
                                .unwrap_or_else(|_| Response::new(Body::from("server busy")))
                                .into_response();
                        }

                        let resp_json = resp_rx.await.unwrap_or_else(|_| r#"{"status":500,"body":"error"}"#.into());
                        let v: serde_json::Value = serde_json::from_str(&resp_json).unwrap_or_else(|_| serde_json::json!({}));
                        let status_u16 = v.get("status").and_then(|s| s.as_u64())
                            .and_then(|n| if (100..1000).contains(&n) { Some(n as u16) } else { None })
                            .unwrap_or(200);
                        let body_out = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();

                        Response::builder()
                            .status(status_u16)
                            .body(Body::from(body_out))
                            .unwrap_or_else(|_| Response::new(Body::from("internal error")))
                            .into_response()
                    }
                }));

                let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("Http.serve: failed to bind 0.0.0.0:{port}: {e}");
                        server_counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                };

                if let Err(e) = axum::serve(listener, app).await {
                    eprintln!("Http.serve: server error: {e}");
                }
                server_counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            });

            Ok(Value::None)
        }),
    })
}

#[derive(Debug)]
enum RequestBody {
    Text(String),
    Json(Value),
}

fn request_body(
    json: Option<&Value>,
    body: Option<&Value>,
    caller: &str,
) -> miette::Result<Option<RequestBody>> {
    if let Some(value) = json {
        return Ok(Some(RequestBody::Json(value.clone())));
    }
    body.map(|value| {
        expect_str_value(value, "`body:`", caller)
            .map(str::to_owned)
            .map(RequestBody::Text)
    })
    .transpose()
}

fn map_from_arg(arg: Option<&Value>, caller: &str) -> miette::Result<HashMap<MapKey, Value>> {
    match arg {
        Some(Value::Map(m)) => Ok(m.clone()),
        Some(other) => Err(miette::miette!(
            "{caller}: `headers:` must be map[str, str], got {}",
            other.type_name()
        )),
        None => Ok(HashMap::new()),
    }
}

async fn http_send(
    method: &str,
    url: &str,
    headers: HashMap<MapKey, Value>,
    body: Option<RequestBody>,
) -> miette::Result<Value> {
    let client = &*HTTP_CLIENT;
    let method_upper = method.to_uppercase();
    let reqwest_method = match method_upper.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        other => return Err(miette::miette!("Http: unsupported method `{other}`")),
    };

    let mut req = client.request(reqwest_method, url);
    for (k, v) in &headers {
        let MapKey::Str(name) = k else {
            return Err(miette::miette!(
                "Http: header names must be str, got {}",
                k.to_value().type_name()
            ));
        };
        let value = expect_str_value(v, "header value", "Http")?;
        req = req.header(name, value);
    }
    if let Some(b) = body {
        match b {
            RequestBody::Text(s) => {
                req = req.body(s);
            }
            RequestBody::Json(value) => {
                req = req.json(&value_to_json(&value));
            }
        }
    }

    let response = req.send().await.map_err(|e| {
        make_typed_report(
            RuntimeErrorKind::Http,
            format!("Http {method_upper} {url}: {e}"),
        )
    })?;
    let status = response.status().as_u16() as i64;
    let response_headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                MapKey::Str(k.as_str().to_string()),
                Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect::<HashMap<_, _>>();
    let body_text = response.text().await.unwrap_or_default();

    let mut result = HashMap::new();
    result.insert(MapKey::Str("status".into()), Value::Integer(status));
    result.insert(MapKey::Str("body".into()), Value::String(body_text));
    result.insert(MapKey::Str("headers".into()), Value::Map(response_headers));
    result.insert(
        MapKey::Str("is_ok".into()),
        Value::Bool((200..300).contains(&status)),
    );
    Ok(Value::Map(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::interpreter::{CallArgValue, Interpreter};

    // ── map_from_arg ────────────────────────────────────────────────────

    #[test]
    fn map_from_arg_returns_map_when_given_map() {
        let mut m = HashMap::new();
        m.insert(MapKey::Str("k".into()), Value::Integer(1));
        let result = map_from_arg(Some(&Value::Map(m.clone())), "Http.get").unwrap();
        assert_eq!(result, m);
    }

    #[test]
    fn map_from_arg_rejects_non_map() {
        let err = map_from_arg(Some(&Value::String("x".into())), "Http.get")
            .expect_err("string headers must fail");
        assert!(err.to_string().contains("must be map[str, str]"));
    }

    #[test]
    fn map_from_arg_returns_empty_when_none() {
        assert!(map_from_arg(None, "Http.get").unwrap().is_empty());
    }

    #[test]
    fn request_body_rejects_non_string_text_body() {
        let err = request_body(None, Some(&Value::Integer(42)), "Http.post")
            .expect_err("integer text body must fail");
        assert_eq!(err.to_string(), "Http.post: `body:` must be str, got int");
    }

    #[tokio::test]
    async fn serve_rejects_non_integer_port() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let serve = ns.methods.get("serve").unwrap();
        let err = serve(
            &mut interp,
            vec![CallArgValue {
                name: None,
                value: Value::String("8080".into()),
            }],
        )
        .await
        .expect_err("string port must fail");
        assert_eq!(
            err.to_string(),
            "Http.serve: argument at position 0 must be int, got str"
        );
    }

    // ── http_send helpers ───────────────────────────────────────────────

    async fn start_echo_server() -> (String, tokio::sync::oneshot::Sender<()>) {
        use axum::Router;
        use axum::extract::Request;
        use axum::routing::any;

        let app = Router::new().route(
            "/echo",
            any(|req: Request| async move {
                let method = req.method().to_string();
                let body_bytes = axum::body::to_bytes(req.into_body(), 1_048_576)
                    .await
                    .unwrap_or_default();
                let body_str = String::from_utf8_lossy(&body_bytes).to_string();
                format!("{method}:{body_str}")
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });

        (url, tx)
    }

    // ── http_send ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn http_send_delete_returns_success() {
        let (url, _shutdown) = start_echo_server().await;
        let result = http_send("DELETE", &format!("{url}/echo"), HashMap::new(), None)
            .await
            .unwrap();
        let body = extract_body(&result);
        assert!(body.starts_with("DELETE:"));
        assert_eq!(extract_status(&result), 200);
    }

    #[tokio::test]
    async fn http_send_put_sends_body() {
        let (url, _shutdown) = start_echo_server().await;
        let result = http_send(
            "PUT",
            &format!("{url}/echo"),
            HashMap::new(),
            Some(RequestBody::Text("hello".into())),
        )
        .await
        .unwrap();
        assert_eq!(extract_body(&result), "PUT:hello");
    }

    #[tokio::test]
    async fn http_send_patch_returns_success() {
        let (url, _shutdown) = start_echo_server().await;
        let result = http_send("PATCH", &format!("{url}/echo"), HashMap::new(), None)
            .await
            .unwrap();
        assert!(extract_body(&result).starts_with("PATCH:"));
    }

    #[tokio::test]
    async fn http_send_non_2xx_response() {
        let (url, _shutdown) = start_echo_server().await;
        // hit a non-existent path — server returns 404 (axum fallback)
        let result = http_send("GET", &format!("{url}/nope"), HashMap::new(), None)
            .await
            .unwrap();
        assert_eq!(extract_status(&result), 404);
        let is_ok = match &result {
            Value::Map(m) => m
                .get(&MapKey::Str("is_ok".into()))
                .map(|v| matches!(v, Value::Bool(true)))
                .unwrap_or(true),
            _ => true,
        };
        assert!(!is_ok, "is_ok should be false for 404");
    }

    #[tokio::test]
    async fn http_send_scalar_body() {
        let (url, _shutdown) = start_echo_server().await;
        let result = http_send(
            "POST",
            &format!("{url}/echo"),
            HashMap::new(),
            Some(RequestBody::Json(Value::Integer(42))),
        )
        .await
        .unwrap();
        assert_eq!(extract_body(&result), "POST:42");
    }

    #[tokio::test]
    async fn http_send_network_error() {
        // Bind then drop the listener so the port is definitely closed.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        drop(listener);

        let result = http_send("GET", &url, HashMap::new(), None).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("Http GET"), "expected Http error in: {err}");
    }

    // ── helpers ─────────────────────────────────────────────────────────

    fn extract_body(v: &Value) -> String {
        match v {
            Value::Map(m) => m
                .get(&MapKey::Str("body".into()))
                .map(|v| v.to_display_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn extract_status(v: &Value) -> i64 {
        match v {
            Value::Map(m) => m
                .get(&MapKey::Str("status".into()))
                .and_then(|v| v.as_int())
                .unwrap_or(0),
            _ => 0,
        }
    }
}
