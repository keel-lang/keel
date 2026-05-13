use std::collections::HashMap;

use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::convert::value_to_json;
use crate::runtime::namespace::{find_arg, ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Http", {
        "get" => |_i, args| Box::pin(async move {
            let url = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Http.get: missing URL"))?;
            let headers = map_from_arg(find_arg(&args, "headers"));
            let response = http_send("GET", &url, headers, None).await?;
            Ok(response)
        }),
        "post" => |_i, args| Box::pin(async move {
            let url = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Http.post: missing URL"))?;
            let headers = map_from_arg(find_arg(&args, "headers"));
            let body = find_arg(&args, "json")
                .or_else(|| find_arg(&args, "body"))
                .cloned();
            let response = http_send("POST", &url, headers, body).await?;
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
                            m.insert(n.clone(), a.value.clone());
                        }
                    }
                    m
                }
            };
            let method = cfg.get("method").map(|v| v.to_display_string()).unwrap_or_else(|| "GET".into());
            let url = cfg.get("url").map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Http.request: missing `url`"))?;
            let headers = cfg.get("headers").cloned().and_then(|v| match v {
                Value::Map(m) => Some(m),
                _ => None,
            }).unwrap_or_default();
            let body = cfg.get("json").or_else(|| cfg.get("body")).cloned();
            http_send(&method, &url, headers, body).await
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
        "serve" => |interp, args| Box::pin(async move {
            let port = match positional(&args, 0) {
                Some(Value::Integer(p)) if *p > 0 && *p < 65536 => *p as u16,
                _ => 8080u16,
            };

            // Extract closure from args
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Http.serve: missing closure argument"))?;

            let closure_id = interp.register_closure("__http_serve__".to_string(), params, body);
            let event_tx = interp.event_tx.clone();
            let server_counter = interp.active_http_servers.clone();

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
                        let _ = tx.send(crate::interpreter::Event::FireClosureWithArgs {
                            closure_id,
                            request_json: req_json,
                            response_tx: resp_tx,
                        });

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

fn map_from_arg(arg: Option<&Value>) -> HashMap<String, Value> {
    match arg {
        Some(Value::Map(m)) => m.clone(),
        _ => HashMap::new(),
    }
}

async fn http_send(
    method: &str,
    url: &str,
    headers: HashMap<String, Value>,
    body: Option<Value>,
) -> miette::Result<Value> {
    let client = reqwest::Client::new();
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
        req = req.header(k, v.to_display_string());
    }
    if let Some(b) = body {
        match b {
            Value::Map(_) | Value::List(_) => {
                // Serialise via serde_json round-trip.
                if let Ok(json) = serde_json::to_value(value_to_json(&b)) {
                    req = req.json(&json);
                }
            }
            Value::String(s) => {
                req = req.body(s);
            }
            _ => {
                req = req.body(b.to_display_string());
            }
        }
    }

    let response = req
        .send()
        .await
        .map_err(|e| miette::miette!("Http {method_upper} {url}: {e}"))?;
    let status = response.status().as_u16() as i64;
    let response_headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect::<HashMap<_, _>>();
    let body_text = response.text().await.unwrap_or_default();

    let mut result = HashMap::new();
    result.insert("status".to_string(), Value::Integer(status));
    result.insert("body".to_string(), Value::String(body_text));
    result.insert("headers".to_string(), Value::Map(response_headers));
    result.insert(
        "is_ok".to_string(),
        Value::Bool((200..300).contains(&status)),
    );
    Ok(Value::Map(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── map_from_arg ────────────────────────────────────────────────────

    #[test]
    fn map_from_arg_returns_map_when_given_map() {
        let mut m = HashMap::new();
        m.insert("k".to_string(), Value::Integer(1));
        let result = map_from_arg(Some(&Value::Map(m.clone())));
        assert_eq!(result, m);
    }

    #[test]
    fn map_from_arg_returns_empty_when_given_non_map() {
        assert!(map_from_arg(Some(&Value::String("x".into()))).is_empty());
    }

    #[test]
    fn map_from_arg_returns_empty_when_none() {
        assert!(map_from_arg(None).is_empty());
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
            Some(Value::String("hello".into())),
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
                .get("is_ok")
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
            Some(Value::Integer(42)),
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
                .get("body")
                .map(|v| v.to_display_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn extract_status(v: &Value) -> i64 {
        match v {
            Value::Map(m) => m.get("status").and_then(|v| v.as_int()).unwrap_or(0),
            _ => 0,
        }
    }
}
