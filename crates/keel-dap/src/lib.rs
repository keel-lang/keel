//! Interpreter-backed Debug Adapter Protocol server for Keel (`keel dap`).
//!
//! Depends on `keel-syntax`, `keel-compiler` (for the `ModuleGraph` type —
//! it lives there, not in `keel-runtime`, which only re-exports it
//! crate-privately) and `keel-runtime`. No LLVM, no codegen: this drives the
//! same tree-walking interpreter every `keel run`/`keel test` already uses,
//! via the `DebugHook` seam in `keel_runtime::interpreter`.
//!
//! The caller (the `keel` CLI) parses and type-checks the target file
//! itself — same gate every other subcommand uses — and hands this crate an
//! already-checked `ModuleGraph` plus a `RuntimeContext`.

mod eval;
mod hooks;
mod line_index;
mod protocol;
mod variables;

use std::collections::HashMap;
use std::sync::Arc;

use keel_compiler::modules::ModuleGraph;
use keel_runtime::interpreter::Interpreter;
use keel_runtime::runtime::context::RuntimeContext;
use serde_json::Value as Json;
use tokio::io::BufReader;

use hooks::{DapHook, ModuleInfo};
use line_index::LineIndex;
use protocol::{DapCommand, IncomingMessage, RawRequest, Transport};

/// What `run_dap_session` should execute once the client finishes the DAP
/// handshake (`initialize`/`launch`/`setBreakpoints`/`configurationDone`).
pub enum SessionMode {
    /// `keel dap <file>` — run the program under the debugger.
    Run,
    /// `keel test --debug --filter <name> <file>` — debug one
    /// already-resolved-to-be-unique, non-parameterized test.
    Test { name: String },
}

/// Drive one DAP session to completion over stdio.
///
/// # Errors
///
/// Returns an error if stdin cannot be read, or if the interpreter (`Run`
/// mode) or the debugged test (`Test` mode) fails.
pub async fn run_dap_session(
    graph: &ModuleGraph,
    runtime: Arc<RuntimeContext>,
    mode: SessionMode,
) -> miette::Result<()> {
    let transport = Arc::new(Transport::new());

    let mut modules = HashMap::new();
    for (id, unit) in graph.modules.iter().enumerate() {
        let path = unit
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| unit.name.clone());
        modules.insert(
            id,
            ModuleInfo {
                name: unit.name.clone(),
                path,
                line_index: LineIndex::new(unit.source.inner()),
            },
        );
    }
    let hook = Arc::new(DapHook::new(transport.clone(), modules));

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);

    let stop_on_entry = match handshake(&mut reader, &hook, &transport).await? {
        Some(stop_on_entry) => stop_on_entry,
        None => return Ok(()), // client disconnected before configurationDone
    };
    if stop_on_entry {
        hook.stop_on_next_statement();
    }

    match mode {
        SessionMode::Run => {
            let mut interp = Interpreter::with_runtime(runtime);
            interp.set_debug_hook(hook.clone());
            let result =
                drive_session(interp.execute_graph(graph), &mut reader, &hook, &transport).await;
            match &result {
                Ok(()) => transport.emit("exited", serde_json::json!({"exitCode": 0})),
                Err(err) => {
                    transport.emit(
                        "output",
                        serde_json::json!({"category": "stderr", "output": format!("{err:?}\n")}),
                    );
                    transport.emit("exited", serde_json::json!({"exitCode": 1}));
                }
            }
            transport.emit("terminated", serde_json::json!({}));
            result
        }
        SessionMode::Test { name } => {
            let result = drive_session(
                keel_runtime::interpreter::debug_graph_test(graph, runtime, &name, hook.clone()),
                &mut reader,
                &hook,
                &transport,
            )
            .await;
            match result {
                Ok(outcome) => {
                    let message = if outcome.passed {
                        format!("PASS {}\n", outcome.name)
                    } else {
                        format!(
                            "FAIL {}: {}\n",
                            outcome.name,
                            outcome.error.clone().unwrap_or_default()
                        )
                    };
                    transport.emit(
                        "output",
                        serde_json::json!({"category": "console", "output": message}),
                    );
                    transport.emit(
                        "exited",
                        serde_json::json!({"exitCode": i64::from(!outcome.passed)}),
                    );
                    transport.emit("terminated", serde_json::json!({}));
                    if outcome.passed {
                        Ok(())
                    } else {
                        Err(miette::miette!(
                            "test `{}` failed: {}",
                            outcome.name,
                            outcome.error.unwrap_or_default()
                        ))
                    }
                }
                Err(err) => {
                    transport.emit(
                        "output",
                        serde_json::json!({"category": "stderr", "output": format!("{err:?}\n")}),
                    );
                    transport.emit("terminated", serde_json::json!({}));
                    Err(err)
                }
            }
        }
    }
}

/// Record the breakpoints a `setBreakpoints` request describes for one
/// source file and reply with each line marked verified. Shared between the
/// handshake (before the program starts) and `dispatch_running_request`
/// (after `configurationDone`, while the program may be running or paused).
///
/// Line numbers on the wire are in the client's declared indexing
/// convention (`linesStartAt1`), not necessarily this server's internal
/// 1-indexed one — `hook.line_from_client`/`line_to_client` convert at this
/// boundary so `hook.set_breakpoints` always stores 1-indexed lines.
fn handle_set_breakpoints(req: &RawRequest, hook: &DapHook, transport: &Transport) {
    let path = req
        .arguments
        .get("source")
        .and_then(|s| s.get("path"))
        .and_then(Json::as_str)
        .unwrap_or("");
    let client_lines: Vec<u32> = req
        .arguments
        .get("breakpoints")
        .and_then(Json::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|bp| bp.get("line").and_then(Json::as_u64).map(|l| l as u32))
                .collect()
        })
        .unwrap_or_default();
    let internal_lines: Vec<u32> = client_lines
        .iter()
        .map(|&line| hook.line_from_client(line))
        .collect();
    if let Some(module_id) = hook.module_id_for_path(path) {
        hook.set_breakpoints(module_id, internal_lines);
    }
    let verified: Vec<Json> = client_lines
        .iter()
        .map(|&line| serde_json::json!({"verified": true, "line": line}))
        .collect();
    transport.respond(
        req.seq,
        "setBreakpoints",
        true,
        serde_json::json!({"breakpoints": verified}),
    );
}

/// Handshake requests, before the program starts running:
/// `initialize` → `launch` → any number of `setBreakpoints` →
/// `configurationDone`. Returns the `stopOnEntry` flag from `launch`, or
/// `None` if the client disconnected first.
async fn handshake(
    reader: &mut BufReader<tokio::io::Stdin>,
    hook: &DapHook,
    transport: &Transport,
) -> miette::Result<Option<bool>> {
    let mut stop_on_entry = false;
    loop {
        let Some(msg) = protocol::read_message(reader)
            .await
            .map_err(|err| miette::miette!("stdin read error: {err}"))?
        else {
            return Ok(None);
        };
        let IncomingMessage::Request(req) = msg else {
            continue;
        };
        match req.command.as_str() {
            "initialize" => {
                // DAP defaults both flags to `true` when the client omits
                // them, so `unwrap_or(true)` matches an absent field to the
                // spec rather than to this server's own convention.
                let lines_start_at_1 = req
                    .arguments
                    .get("linesStartAt1")
                    .and_then(Json::as_bool)
                    .unwrap_or(true);
                let columns_start_at_1 = req
                    .arguments
                    .get("columnsStartAt1")
                    .and_then(Json::as_bool)
                    .unwrap_or(true);
                hook.set_indexing(lines_start_at_1, columns_start_at_1);
                transport.respond(
                    req.seq,
                    "initialize",
                    true,
                    serde_json::json!({"supportsConfigurationDoneRequest": true}),
                );
                transport.emit("initialized", serde_json::json!({}));
            }
            "launch" => {
                stop_on_entry = req
                    .arguments
                    .get("stopOnEntry")
                    .and_then(Json::as_bool)
                    .unwrap_or(false);
                transport.respond(req.seq, "launch", true, serde_json::json!({}));
            }
            "setBreakpoints" => handle_set_breakpoints(&req, hook, transport),
            "configurationDone" => {
                transport.respond(req.seq, "configurationDone", true, serde_json::json!({}));
                return Ok(Some(stop_on_entry));
            }
            "disconnect" => {
                transport.respond(req.seq, "disconnect", true, serde_json::json!({}));
                return Ok(None);
            }
            other => {
                transport.respond_error(req.seq, other, "not supported before configurationDone");
            }
        }
    }
}

/// Run `exec_fut` to completion while concurrently forwarding DAP requests
/// arriving on stdin to the paused hook (or answering them directly once
/// stdin closes, so the loop never busy-spins on repeated EOF reads).
///
/// Each request is dispatched on its own spawned task rather than awaited
/// inline: a request that needs the paused hook's reply (`stackTrace`,
/// `scopes`, …) blocks on a channel that only the interpreter's own task —
/// polling `exec_fut` — can ever satisfy. Awaiting that reply directly
/// inside this loop would stop this loop from polling `exec_fut` at all,
/// deadlocking both sides forever.
///
/// A `continue`/`step` request is the one that makes `exec_fut` resume and
/// finish — its own response is sent by that same spawned task, *after*
/// `on_statement` has already returned control to `exec_fut`. So `exec_fut`
/// can resolve before that task gets scheduled to actually write the
/// response. Draining every still-running dispatch task once `exec_fut`
/// finishes closes that race: the caller (which hard-exits the process
/// right after this returns, to avoid a separate stdin-shutdown hang) only
/// proceeds once every in-flight response has actually been written.
async fn drive_session<T>(
    exec_fut: impl std::future::Future<Output = T>,
    reader: &mut BufReader<tokio::io::Stdin>,
    hook: &Arc<DapHook>,
    transport: &Arc<Transport>,
) -> T {
    tokio::pin!(exec_fut);
    let mut stdin_open = true;
    let mut pending = tokio::task::JoinSet::new();
    let result = loop {
        tokio::select! {
            biased;
            result = &mut exec_fut => {
                break result;
            }
            msg = protocol::read_message(reader), if stdin_open => {
                match msg {
                    Ok(Some(IncomingMessage::Request(req))) => {
                        pending.spawn(dispatch_running_request(req, hook.clone(), transport.clone()));
                    }
                    Ok(Some(IncomingMessage::Other)) | Err(_) => {}
                    Ok(None) => stdin_open = false,
                }
            }
        }
    };
    while pending.join_next().await.is_some() {}
    result
}

/// Dispatch one request that arrived while the interpreter is running
/// (paused or not). Matches `DapCommand` exhaustively: session-scoped
/// commands (`scopes`/`variables`/`evaluate`/`continue`/`next`/`stepIn`/
/// `stepOut`/`threads`/`stackTrace`) only make sense while stopped, so they
/// forward through the hook — an unpaused program answers them with an
/// error rather than hanging.
async fn dispatch_running_request(req: RawRequest, hook: Arc<DapHook>, transport: Arc<Transport>) {
    match DapCommand::parse(&req.command) {
        cmd if cmd.requires_paused_frame() => {
            match hook
                .forward_if_paused(req.command.clone(), req.arguments.clone())
                .await
            {
                Some(body) => transport.respond(req.seq, &req.command, true, body),
                None => transport.respond_error(req.seq, &req.command, "program is not stopped"),
            }
        }
        // Unlike the commands above, adding/removing breakpoints doesn't
        // need a paused frame — it just updates the hook's breakpoint set —
        // so it's handled the same way whether the program is running or
        // stopped. Real clients (VS Code included) routinely send this
        // after `configurationDone`, e.g. when a breakpoint is toggled
        // while the program is already running.
        DapCommand::SetBreakpoints => handle_set_breakpoints(&req, &hook, &transport),
        DapCommand::Pause => {
            transport.respond(req.seq, &req.command, true, serde_json::json!({}));
        }
        DapCommand::Disconnect | DapCommand::Terminate => {
            // `keel dap`/`keel test --debug` only ever launch the debuggee
            // (never attach to an existing process), so there is no
            // "detach and leave it running" case to support: a client
            // disconnect must end the debuggee, or the interpreter — paused
            // or not — keeps running as an orphaned process after the
            // client that started it has gone away.
            transport.respond(req.seq, &req.command, true, serde_json::json!({}));
            std::process::exit(0);
        }
        // Unreachable in practice (the guard arm above already claims every
        // `requires_paused_frame` variant), but the guard isn't visible to
        // exhaustiveness checking, so this catch-all is what makes the match
        // exhaustive; genuinely-unrecognized command strings also land here.
        _ => {
            transport.respond_error(req.seq, &req.command, "unsupported request");
        }
    }
}
