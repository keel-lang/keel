//! Minimal Debug Adapter Protocol message types and stdio transport.
//!
//! Hand-rolled rather than depending on an external DAP crate: the message
//! subset D0 needs is small (a dozen request/response/event shapes), and the
//! only crate offering a server harness (`dap`) documents itself as pre-1.0
//! with frequent breaking changes — not worth the churn for this surface.
//! Message shapes follow the published DAP spec directly.

use std::io::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

/// One decoded `Content-Length`-framed DAP message from the client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum IncomingMessage {
    Request(RawRequest),
    #[serde(other)]
    Other,
}

/// Every request command this server recognizes once past the handshake
/// (`setBreakpoints`/`configurationDone` are handshake-only and aren't
/// represented here). Parsing the raw command string into this enum once,
/// then matching on it, is the single source of truth for "which commands
/// exist and what each one means" — `dispatch_running_request` and
/// `DapHook`'s paused-request loop both match on it exhaustively, so adding
/// a command here forces both call sites to decide what it does instead of
/// silently disagreeing about which commands are recognized.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DapCommand {
    Threads,
    StackTrace,
    Scopes,
    Variables,
    Evaluate,
    Continue,
    Next,
    StepIn,
    StepOut,
    SetBreakpoints,
    Pause,
    Disconnect,
    Terminate,
    /// Any command string this server doesn't implement.
    Unsupported,
}

impl DapCommand {
    pub fn parse(command: &str) -> Self {
        match command {
            "threads" => Self::Threads,
            "stackTrace" => Self::StackTrace,
            "scopes" => Self::Scopes,
            "variables" => Self::Variables,
            "evaluate" => Self::Evaluate,
            "continue" => Self::Continue,
            "next" => Self::Next,
            "stepIn" => Self::StepIn,
            "stepOut" => Self::StepOut,
            "setBreakpoints" => Self::SetBreakpoints,
            "pause" => Self::Pause,
            "disconnect" => Self::Disconnect,
            "terminate" => Self::Terminate,
            _ => Self::Unsupported,
        }
    }

    /// Whether this command only makes sense while the interpreter is
    /// paused inside `DebugHook::on_statement` — it must be forwarded there
    /// via `DapHook::forward_if_paused` rather than answered directly.
    pub fn requires_paused_frame(self) -> bool {
        matches!(
            self,
            Self::Threads
                | Self::StackTrace
                | Self::Scopes
                | Self::Variables
                | Self::Evaluate
                | Self::Continue
                | Self::Next
                | Self::StepIn
                | Self::StepOut
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct RawRequest {
    pub seq: i64,
    pub command: String,
    #[serde(default)]
    pub arguments: Json,
}

/// Read one `Content-Length`-framed JSON message from `reader`. Returns
/// `Ok(None)` on clean EOF (the client closed the pipe).
pub async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<Option<IncomingMessage>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let Some(len) = content_length else {
        return Ok(None);
    };
    let mut buf = vec![0u8; len];
    tokio::io::AsyncReadExt::read_exact(reader, &mut buf).await?;
    let msg = serde_json::from_slice(&buf).unwrap_or(IncomingMessage::Other);
    Ok(Some(msg))
}

/// Outgoing side of the stdio transport. Every server→client message needs a
/// unique, increasing `seq` — this is the one place that assigns it, so
/// there's exactly one counter and one lock, and frames never interleave.
pub struct Transport {
    out: parking_lot::Mutex<std::io::Stdout>,
    seq: std::sync::atomic::AtomicI64,
}

impl Transport {
    pub fn new() -> Self {
        Self {
            out: parking_lot::Mutex::new(std::io::stdout()),
            seq: std::sync::atomic::AtomicI64::new(1),
        }
    }

    fn send(&self, mut msg: Json) {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        msg["seq"] = Json::from(seq);
        let text = serde_json::to_string(&msg).expect("DAP message serializes");
        let mut stdout = self.out.lock();
        let _ = write!(stdout, "Content-Length: {}\r\n\r\n{}", text.len(), text);
        let _ = stdout.flush();
    }

    pub fn respond(&self, request_seq: i64, command: &str, success: bool, body: Json) {
        self.send(serde_json::json!({
            "type": "response",
            "request_seq": request_seq,
            "success": success,
            "command": command,
            "body": body,
        }));
    }

    pub fn respond_error(&self, request_seq: i64, command: &str, message: &str) {
        self.send(serde_json::json!({
            "type": "response",
            "request_seq": request_seq,
            "success": false,
            "command": command,
            "message": message,
        }));
    }

    pub fn emit(&self, name: &str, body: Json) {
        self.send(serde_json::json!({
            "type": "event",
            "event": name,
            "body": body,
        }));
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
pub struct DapStackFrame {
    pub id: i64,
    pub name: String,
    pub line: u32,
    pub column: u32,
    pub source: Option<DapSource>,
}

#[derive(Debug, Serialize)]
pub struct DapSource {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct DapScope {
    pub name: String,
    #[serde(rename = "variablesReference")]
    pub variables_reference: i64,
    pub expensive: bool,
}

#[derive(Debug, Serialize)]
pub struct DapVariable {
    pub name: String,
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(rename = "variablesReference")]
    pub variables_reference: i64,
}
