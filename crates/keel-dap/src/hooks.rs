//! `DapHook` — the `DebugHook` implementation driving a `keel dap` session.
//!
//! Pause semantics: when `on_statement` decides to stop, it does not return
//! control to the interpreter — it enters a loop reading `PausedRequest`s
//! off a channel it just published, servicing `stackTrace`/`scopes`/
//! `variables`/`evaluate` directly against the `&mut Interpreter`/`&mut
//! Environment` it's already holding, and only returns once a
//! `continue`/`next`/`stepIn`/`stepOut` request arrives. This is the only
//! way to expose a live, suspended frame to requests arriving on a separate
//! stdin-reading task — `Environment` is a plain owned value, not shared.
//!
//! D0 limitation: only the innermost (currently executing) frame's
//! variables are inspectable. Each task/closure call gets its own fresh
//! `Environment` with no link back to the caller's (see `call.rs`), so a
//! `scopes` request for any outer stack frame returns no scopes rather than
//! stale or wrong data.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value as Json;
use tokio::sync::{mpsc, oneshot};

use keel_runtime::interpreter::debug_hook::{
    DebugHook, DebugHookFuture, FrameInfo, SourceLocation,
};
use keel_runtime::interpreter::environment::Environment;
use keel_runtime::interpreter::{Host, Interpreter};

use crate::eval;
use crate::line_index::LineIndex;
use crate::protocol::{DapCommand, DapScope, DapSource, DapStackFrame, Transport};
use crate::variables::{VariablesArena, locals_from_env};

/// Sentinel `variablesReference` values for the two top-level DAP scopes —
/// chosen far outside the range `VariablesArena` hands out (which starts at
/// 1 and grows by one per compound value registered in a single pause).
const LOCALS_REF: i64 = i64::MAX - 1;
const AGENT_STATE_REF: i64 = i64::MAX - 2;

pub struct ModuleInfo {
    pub name: String,
    pub path: String,
    pub line_index: LineIndex,
}

/// One DAP request forwarded to the currently-paused hook; `respond` carries
/// the reply back to whichever task is servicing the client's stdin.
pub struct PausedRequest {
    pub command: String,
    pub arguments: Json,
    pub respond: oneshot::Sender<Json>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StepMode {
    Continue,
    StepIn,
    StepOver(usize),
    StepOut(usize),
}

pub struct DapHook {
    modules: HashMap<usize, ModuleInfo>,
    breakpoints: Mutex<HashMap<usize, HashSet<u32>>>,
    step_mode: Mutex<StepMode>,
    frames: Mutex<Vec<FrameInfo>>,
    paused_tx: Mutex<Option<mpsc::Sender<PausedRequest>>>,
    transport: Arc<Transport>,
    /// The client's declared line/column indexing convention, read from
    /// `initialize`'s `linesStartAt1`/`columnsStartAt1` (DAP defaults both
    /// to `true` when absent). Everywhere else in this crate works in
    /// 1-indexed lines internally (`LineIndex::line_at`); these flags are
    /// only consulted at the client-facing edge — converting an incoming
    /// breakpoint line, or an outgoing stack frame's line/column — so a
    /// 0-indexed client sees numbers in its own convention without the rest
    /// of the server needing to care.
    lines_start_at_1: Mutex<bool>,
    columns_start_at_1: Mutex<bool>,
}

impl DapHook {
    pub fn new(transport: Arc<Transport>, modules: HashMap<usize, ModuleInfo>) -> Self {
        Self {
            modules,
            breakpoints: Mutex::new(HashMap::new()),
            step_mode: Mutex::new(StepMode::Continue),
            frames: Mutex::new(Vec::new()),
            paused_tx: Mutex::new(None),
            transport,
            lines_start_at_1: Mutex::new(true),
            columns_start_at_1: Mutex::new(true),
        }
    }

    pub fn module_id_for_path(&self, path: &str) -> Option<usize> {
        self.modules
            .iter()
            .find(|(_, m)| m.path == path)
            .map(|(id, _)| *id)
    }

    /// Record the client's declared indexing convention. Called once, from
    /// `initialize`, before any breakpoint or stack frame is reported.
    pub fn set_indexing(&self, lines_start_at_1: bool, columns_start_at_1: bool) {
        *self.lines_start_at_1.lock() = lines_start_at_1;
        *self.columns_start_at_1.lock() = columns_start_at_1;
    }

    /// Convert a line number as sent by the client into this server's
    /// internal 1-indexed convention.
    pub fn line_from_client(&self, line: u32) -> u32 {
        if *self.lines_start_at_1.lock() {
            line
        } else {
            line + 1
        }
    }

    /// Convert an internal 1-indexed line number into the client's
    /// declared convention.
    pub fn line_to_client(&self, line: u32) -> u32 {
        if *self.lines_start_at_1.lock() {
            line
        } else {
            line.saturating_sub(1)
        }
    }

    /// The column to report for a stack frame, in the client's declared
    /// convention. Column tracking isn't implemented (only lines are), so
    /// this is always "the start of the line" in whichever indexing the
    /// client asked for.
    fn client_column(&self) -> u32 {
        if *self.columns_start_at_1.lock() {
            1
        } else {
            0
        }
    }

    pub fn set_breakpoints(&self, module_id: usize, lines: Vec<u32>) {
        self.breakpoints
            .lock()
            .insert(module_id, lines.into_iter().collect());
    }

    /// Force the very next statement to pause, regardless of breakpoints —
    /// `launch`'s `stopOnEntry`.
    pub fn stop_on_next_statement(&self) {
        *self.step_mode.lock() = StepMode::StepIn;
    }

    /// Forward a DAP request to the paused hook and await its reply. Returns
    /// `None` if nothing is currently paused (caller should reply with an
    /// error — these commands are meaningless outside a `stopped` state).
    pub async fn forward_if_paused(&self, command: String, arguments: Json) -> Option<Json> {
        let tx = self.paused_tx.lock().clone()?;
        let (respond, rx) = oneshot::channel();
        tx.send(PausedRequest {
            command,
            arguments,
            respond,
        })
        .await
        .ok()?;
        rx.await.ok()
    }

    fn should_pause(&self, location: &SourceLocation, call_depth: usize) -> bool {
        let is_breakpoint = self
            .modules
            .get(&location.module_id)
            .map(|m| m.line_index.line_at(location.span.start))
            .is_some_and(|line| {
                self.breakpoints
                    .lock()
                    .get(&location.module_id)
                    .is_some_and(|set| set.contains(&line))
            });
        if is_breakpoint {
            return true;
        }
        match *self.step_mode.lock() {
            StepMode::Continue => false,
            StepMode::StepIn => true,
            StepMode::StepOver(baseline) => call_depth <= baseline,
            StepMode::StepOut(baseline) => call_depth < baseline,
        }
    }

    fn stack_trace_body(&self) -> Json {
        let frames = self.frames.lock();
        let stack_frames: Vec<DapStackFrame> = frames
            .iter()
            .rev()
            .enumerate()
            .map(|(id, f)| {
                let module = self.modules.get(&f.location.module_id);
                let line = module
                    .map(|m| m.line_index.line_at(f.location.span.start))
                    .unwrap_or(0);
                let source = module.map(|m| DapSource {
                    name: m.name.clone(),
                    path: m.path.clone(),
                });
                DapStackFrame {
                    id: id as i64,
                    name: f.name.clone(),
                    line: self.line_to_client(line),
                    column: self.client_column(),
                    source,
                }
            })
            .collect();
        let total = stack_frames.len();
        serde_json::json!({ "stackFrames": stack_frames, "totalFrames": total })
    }

    fn agent_state_variables(
        &self,
        interp: &Interpreter,
        arena: &mut VariablesArena,
    ) -> Vec<crate::protocol::DapVariable> {
        let Some(agent_name) = interp.current_agent_name() else {
            return Vec::new();
        };
        let Some(instance) = interp.live_agents().lock().get(&agent_name).cloned() else {
            return Vec::new();
        };
        let guard = instance.lock();
        crate::variables::locals_from_env(arena, std::iter::once(&guard.state))
    }
}

impl DebugHook for DapHook {
    fn on_statement<'a>(
        &'a self,
        interp: &'a mut Interpreter,
        env: &'a mut Environment,
        location: SourceLocation,
        call_depth: usize,
    ) -> DebugHookFuture<'a> {
        Box::pin(async move {
            if self.paused_tx.lock().is_some() {
                // Reentrant: we're already paused and servicing a request
                // (an `evaluate` whose expression calls a task/closure runs
                // that call's statements back through this same hook). Let
                // it run straight through — pausing again here would
                // overwrite the single `paused_tx` slot and misdirect
                // subsequent commands to this inner call instead of the
                // outer, still-waiting pause.
                return Ok(());
            }
            if !self.should_pause(&location, call_depth) {
                return Ok(());
            }

            {
                let mut frames = self.frames.lock();
                if let Some(top) = frames.last_mut() {
                    top.location = location.clone();
                } else {
                    frames.push(FrameInfo {
                        name: "main".to_string(),
                        location: location.clone(),
                    });
                }
            }

            let mut arena = VariablesArena::new();
            let (tx, mut rx) = mpsc::channel::<PausedRequest>(8);
            *self.paused_tx.lock() = Some(tx);

            self.transport.emit(
                "stopped",
                serde_json::json!({
                    "reason": "breakpoint",
                    "threadId": 1,
                    "allThreadsStopped": true,
                }),
            );

            while let Some(req) = rx.recv().await {
                let PausedRequest {
                    command,
                    arguments,
                    respond,
                } = req;
                match DapCommand::parse(&command) {
                    DapCommand::Threads => {
                        let _ = respond
                            .send(serde_json::json!({"threads": [{"id": 1, "name": "main"}]}));
                    }
                    DapCommand::StackTrace => {
                        let _ = respond.send(self.stack_trace_body());
                    }
                    DapCommand::Scopes => {
                        let is_top_frame =
                            arguments.get("frameId").and_then(Json::as_i64).unwrap_or(0) == 0;
                        let mut scopes = Vec::new();
                        if is_top_frame {
                            scopes.push(DapScope {
                                name: "Locals".to_string(),
                                variables_reference: LOCALS_REF,
                                expensive: false,
                            });
                            if interp.current_agent_name().is_some() {
                                scopes.push(DapScope {
                                    name: "Agent State".to_string(),
                                    variables_reference: AGENT_STATE_REF,
                                    expensive: false,
                                });
                            }
                        }
                        let _ = respond.send(serde_json::json!({ "scopes": scopes }));
                    }
                    DapCommand::Variables => {
                        let reference = arguments
                            .get("variablesReference")
                            .and_then(Json::as_i64)
                            .unwrap_or(0);
                        let vars = if reference == LOCALS_REF {
                            locals_from_env(&mut arena, env.scopes())
                        } else if reference == AGENT_STATE_REF {
                            self.agent_state_variables(interp, &mut arena)
                        } else {
                            arena.variables_for(reference)
                        };
                        let _ = respond.send(serde_json::json!({ "variables": vars }));
                    }
                    DapCommand::Evaluate => {
                        let expression = arguments
                            .get("expression")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string();
                        let body = match eval::evaluate(interp, env, &expression).await {
                            Ok((display, ty)) => {
                                serde_json::json!({"result": display, "type": ty, "variablesReference": 0})
                            }
                            Err(message) => {
                                serde_json::json!({"result": message, "variablesReference": 0})
                            }
                        };
                        let _ = respond.send(body);
                    }
                    cmd @ (DapCommand::Continue
                    | DapCommand::Next
                    | DapCommand::StepIn
                    | DapCommand::StepOut) => {
                        *self.step_mode.lock() = match cmd {
                            DapCommand::Continue => StepMode::Continue,
                            DapCommand::Next => StepMode::StepOver(call_depth),
                            DapCommand::StepIn => StepMode::StepIn,
                            DapCommand::StepOut => StepMode::StepOut(call_depth),
                            _ => unreachable!("cmd is constrained to the four bound above"),
                        };
                        let _ = respond.send(serde_json::json!({}));
                        break;
                    }
                    // Every other variant either never reaches the paused
                    // hook at all (`SetBreakpoints`/`Pause`/`Disconnect`/
                    // `Terminate` are answered directly by
                    // `dispatch_running_request`) or has no meaning while
                    // paused (`Unsupported`) — reply empty rather than
                    // leave the client's request hanging.
                    DapCommand::SetBreakpoints
                    | DapCommand::Pause
                    | DapCommand::Disconnect
                    | DapCommand::Terminate
                    | DapCommand::Unsupported => {
                        let _ = respond.send(serde_json::json!({}));
                    }
                }
            }

            *self.paused_tx.lock() = None;
            Ok(())
        })
    }

    fn on_call_enter(&self, frame: FrameInfo) {
        self.frames.lock().push(frame);
    }

    fn on_call_exit(&self) {
        self.frames.lock().pop();
    }
}
