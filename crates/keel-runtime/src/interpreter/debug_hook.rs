//! `DebugHook` — the seam a step debugger (`keel-dap`) attaches to.
//!
//! Called at every statement boundary in [`super::stmt::Interpreter::exec_stmt`]
//! and at every task/closure call transition. The default [`NoopDebugHook`]
//! costs one `Arc` clone and an empty future per statement; a real
//! implementation holds `&mut Interpreter`/`&mut Environment` for as long as
//! it stays paused, servicing breakpoint/step/variable/evaluate requests
//! in place — `Environment` is a plain owned value, not `Arc<Mutex<_>>>`,
//! so there is no other way to expose a live, suspended frame to a
//! DAP request arriving on a different task.

use std::future::Future;
use std::pin::Pin;

use miette::Result;

use crate::lexer::Span;

use super::environment::Environment;
use super::state::Interpreter;

/// Boxed async future returned by `DebugHook` methods, matching the
/// `HostFuture` convention in [`super::host`].
pub type DebugHookFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Where an executing statement lives: which module (index into the
/// checked `ModuleGraph`, or `0` for a single in-memory program) and its
/// byte-offset span within that module's own source text.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub module_id: usize,
    pub span: Span,
}

/// One entry in the debugger's call stack, pushed on `on_call_enter` and
/// popped on `on_call_exit`.
#[derive(Debug, Clone)]
pub struct FrameInfo {
    /// Display name for the DAP `StackFrame` (task/handler/closure name).
    pub name: String,
    pub location: SourceLocation,
}

/// Statement-boundary and call-transition hook. Interpreter behavior is
/// unchanged when no hook is installed ([`NoopDebugHook`] is the default).
pub trait DebugHook: Send + Sync {
    /// Called before executing the statement at `location`. A real
    /// implementation checks breakpoints/step state and, if it decides to
    /// pause, services `variables`/`evaluate` DAP requests using `interp`
    /// and `env` until a `continue`/`step` command resumes it.
    fn on_statement<'a>(
        &'a self,
        interp: &'a mut Interpreter,
        env: &'a mut Environment,
        location: SourceLocation,
        call_depth: usize,
    ) -> DebugHookFuture<'a>;

    /// Called when a task/closure call begins, for `stackTrace` bookkeeping.
    fn on_call_enter(&self, frame: FrameInfo);

    /// Called when the call started by the matching `on_call_enter` returns.
    fn on_call_exit(&self);
}

/// No-op default — zero behavior change for every `keel run`/`keel test`
/// invocation that isn't under `keel dap`.
pub struct NoopDebugHook;

impl DebugHook for NoopDebugHook {
    fn on_statement<'a>(
        &'a self,
        _interp: &'a mut Interpreter,
        _env: &'a mut Environment,
        _location: SourceLocation,
        _call_depth: usize,
    ) -> DebugHookFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn on_call_enter(&self, _frame: FrameInfo) {}
    fn on_call_exit(&self) {}
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use parking_lot::Mutex;

    use super::*;
    use crate::runtime::context::RuntimeContext;

    /// Records every `on_statement`/`on_call_enter`/`on_call_exit` call so
    /// tests can assert on exactly what the interpreter reports.
    #[derive(Default)]
    struct RecordingHook {
        statements: Mutex<Vec<(usize, usize)>>, // (module_id, call_depth)
        frames: Mutex<Vec<FrameInfo>>,
        enters: AtomicUsize,
        exits: AtomicUsize,
    }

    impl DebugHook for RecordingHook {
        fn on_statement<'a>(
            &'a self,
            _interp: &'a mut Interpreter,
            _env: &'a mut Environment,
            location: SourceLocation,
            call_depth: usize,
        ) -> DebugHookFuture<'a> {
            self.statements
                .lock()
                .push((location.module_id, call_depth));
            Box::pin(async { Ok(()) })
        }

        fn on_call_enter(&self, frame: FrameInfo) {
            self.enters.fetch_add(1, Ordering::SeqCst);
            self.frames.lock().push(frame);
        }

        fn on_call_exit(&self) {
            self.exits.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// A hook whose `on_statement` panics — proves `debug_active == false`
    /// (the default) skips the hook call entirely rather than invoking a
    /// no-op-shaped version of it.
    struct PanicIfCalledHook;

    impl DebugHook for PanicIfCalledHook {
        fn on_statement<'a>(
            &'a self,
            _interp: &'a mut Interpreter,
            _env: &'a mut Environment,
            _location: SourceLocation,
            _call_depth: usize,
        ) -> DebugHookFuture<'a> {
            panic!("on_statement must not be called unless set_debug_hook was used");
        }
        fn on_call_enter(&self, _frame: FrameInfo) {
            panic!("on_call_enter must not be called unless set_debug_hook was used");
        }
        fn on_call_exit(&self) {
            panic!("on_call_exit must not be called unless set_debug_hook was used");
        }
    }

    #[tokio::test]
    async fn hook_is_not_invoked_without_set_debug_hook() {
        // A fresh Interpreter defaults to NoopDebugHook + debug_active=false.
        // Swap in a hook that panics if called at all, proving the fast-path
        // gate — not just a no-op hook — is what runs by default.
        let mut interp = Interpreter::new();
        interp.debug_hook = Arc::new(PanicIfCalledHook);
        // debug_active stays false: only set_debug_hook flips it.
        let program = crate::ast::Program {
            declarations: vec![crate::ast::Node::synthetic(crate::ast::Decl::Stmt(
                crate::ast::Node::synthetic(crate::ast::Stmt::Expr(crate::ast::Node::synthetic(
                    crate::ast::Expr::Integer(1),
                ))),
            ))],
        };
        interp.execute(program).await.unwrap();
    }

    #[tokio::test]
    async fn set_debug_hook_activates_the_fast_path_gate() {
        let mut interp = Interpreter::new();
        let hook = Arc::new(RecordingHook::default());
        interp.set_debug_hook(hook.clone());
        assert!(interp.debug_active);

        let program = crate::ast::Program {
            declarations: vec![crate::ast::Node::synthetic(crate::ast::Decl::Stmt(
                crate::ast::Node::synthetic(crate::ast::Stmt::Expr(crate::ast::Node::synthetic(
                    crate::ast::Expr::Integer(1),
                ))),
            ))],
        };
        interp.execute(program).await.unwrap();

        // One top-level statement, module 0, call depth 0.
        assert_eq!(*hook.statements.lock(), vec![(0, 0)]);
    }

    #[tokio::test]
    async fn call_depth_tracks_nested_task_calls_and_frames_pair_up() {
        let src = r#"
task inner() -> int {
  x = 1
  return x
}
task outer() -> int {
  return inner()
}
y = outer()
"#;
        let (program, _source) = keel_syntax::parse_source(src, "t.keel").unwrap();
        let mut interp = Interpreter::new();
        let hook = Arc::new(RecordingHook::default());
        interp.set_debug_hook(hook.clone());
        interp.execute(program).await.unwrap();

        let depths: Vec<usize> = hook.statements.lock().iter().map(|(_, d)| *d).collect();
        // Top-level `y = outer()` at depth 0, `return inner()` inside outer
        // at depth 1, `x = 1`/`return x` inside inner at depth 2.
        assert!(depths.contains(&0));
        assert!(depths.contains(&1));
        assert!(depths.contains(&2));

        // Every on_call_enter has a matching on_call_exit (outer + inner).
        assert_eq!(hook.enters.load(Ordering::SeqCst), 2);
        assert_eq!(hook.exits.load(Ordering::SeqCst), 2);
        let names: Vec<String> = hook.frames.lock().iter().map(|f| f.name.clone()).collect();
        assert_eq!(names, vec!["outer", "inner"]);
    }

    #[tokio::test]
    async fn module_id_switches_across_a_two_file_graph() {
        let dir = tempfile::tempdir().unwrap();
        let lib_path = dir.path().join("lib.keel");
        std::fs::write(&lib_path, "task helper() -> int {\n  return 7\n}\n").unwrap();
        let main_path = dir.path().join("main.keel");
        std::fs::write(&main_path, "use \"./lib.keel\"\nx = helper()\n").unwrap();

        let main_src = std::fs::read_to_string(&main_path).unwrap();
        let graph = crate::modules::load_graph(&main_src, "main.keel", Some(&main_path)).unwrap();
        assert_eq!(graph.modules.len(), 2);
        let entry_index = graph.entry_index();
        let lib_index = 1 - entry_index; // the other module in a 2-module graph

        let runtime = RuntimeContext::native();
        let mut interp = Interpreter::with_runtime(runtime);
        let hook = Arc::new(RecordingHook::default());
        interp.set_debug_hook(hook.clone());
        interp.execute_graph(&graph).await.unwrap();

        let module_ids: Vec<usize> = hook.statements.lock().iter().map(|(m, _)| *m).collect();
        // The top-level `x = helper()` runs in the entry module; the body of
        // `helper()` runs in the imported module — never the entry module's id.
        assert!(module_ids.contains(&entry_index));
        assert!(module_ids.contains(&lib_index));
    }
}
