use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use parking_lot::Mutex;

use super::llm::LlmClient;

mod async_tasks;
mod cache;
mod clock;
mod config;
mod env;
mod fs;
mod memory;

pub use async_tasks::{AsyncTaskHandle, AsyncTaskResult};
pub use cache::CacheHandle;
pub use clock::{Clock, NativeClock};
pub use config::{RuntimeConfig, log_level_name, log_level_rank};
pub use env::{EnvProvider, NativeEnv};
pub use fs::{FileSystem, NativeFileSystem};
pub use memory::{NativePersistentMemoryStore, PersistentMemoryStore, SessionMemoryStore};

#[cfg(test)]
pub use clock::FixedClock;
#[cfg(test)]
pub use env::MapEnv;
#[cfg(test)]
pub use fs::InMemoryFileSystem;
#[cfg(test)]
pub use memory::InMemoryPersistentMemoryStore;

pub struct RuntimeContext {
    pub env: Arc<dyn EnvProvider>,
    pub clock: Arc<dyn Clock>,
    pub file_system: Arc<dyn FileSystem>,
    pub session_memory: Arc<SessionMemoryStore>,
    pub persistent_memory: Arc<dyn PersistentMemoryStore>,
    pub cache: CacheHandle,
    pub llm: Arc<LlmClient>,
    /// Trace flag shared with the LLM client so a runtime-level
    /// `set_trace` immediately affects LLM call narration.
    trace: Arc<AtomicBool>,
    log_threshold: AtomicU8,
    async_handle_counter: AtomicU64,
    async_tasks: AsyncTaskHandle,
    /// Maximum depth of the interpreter event queue. Configured via
    /// `KEEL_EVENT_QUEUE_CAPACITY` (default 1024). Read-only after construction.
    event_queue_capacity: usize,
}

impl RuntimeContext {
    pub fn native() -> Arc<Self> {
        Self::native_with_config(RuntimeConfig::from_env(&NativeEnv))
    }

    pub fn native_with_config(config: RuntimeConfig) -> Arc<Self> {
        let env: Arc<dyn EnvProvider> = Arc::new(NativeEnv);
        let trace = Arc::new(AtomicBool::new(config.trace_enabled()));
        Arc::new(Self {
            clock: Arc::new(NativeClock),
            file_system: Arc::new(NativeFileSystem),
            session_memory: Arc::new(SessionMemoryStore::default()),
            persistent_memory: Arc::new(NativePersistentMemoryStore::new(env.clone())),
            cache: Arc::new(Mutex::new(HashMap::new())),
            llm: Arc::new(LlmClient::from_env_with_trace(
                env.as_ref(),
                Arc::clone(&trace),
            )),
            trace,
            log_threshold: AtomicU8::new(config.log_threshold()),
            async_handle_counter: AtomicU64::new(0),
            async_tasks: Arc::new(Mutex::new(HashMap::new())),
            event_queue_capacity: config.event_queue_capacity(),
            env,
        })
    }

    /// Construct an isolated runtime with the same host-facing backends.
    ///
    /// Used by `keel test` so each test gets fresh session memory, cache,
    /// async handles, and LLM client state while preserving CLI/env settings.
    pub fn isolated_from(base: &Arc<Self>) -> Arc<Self> {
        let trace = Arc::new(AtomicBool::new(base.trace_enabled()));
        Arc::new(Self {
            env: Arc::clone(&base.env),
            clock: Arc::clone(&base.clock),
            file_system: Arc::clone(&base.file_system),
            session_memory: Arc::new(SessionMemoryStore::default()),
            persistent_memory: Arc::new(NativePersistentMemoryStore::new(Arc::clone(&base.env))),
            cache: Arc::new(Mutex::new(HashMap::new())),
            llm: Arc::new(LlmClient::from_env_with_trace(
                base.env.as_ref(),
                Arc::clone(&trace),
            )),
            trace,
            log_threshold: AtomicU8::new(base.current_log_threshold()),
            async_handle_counter: AtomicU64::new(0),
            async_tasks: Arc::new(Mutex::new(HashMap::new())),
            event_queue_capacity: base.event_queue_capacity(),
        })
    }

    /// Construct a runtime context with mocked backends for deterministic
    /// unit testing. Only available in test builds.
    #[cfg(test)]
    pub fn test_context(
        env: Arc<dyn EnvProvider>,
        clock: Arc<dyn Clock>,
        file_system: Arc<dyn FileSystem>,
    ) -> Arc<Self> {
        let trace = Arc::new(AtomicBool::new(false));
        Arc::new(Self {
            env,
            clock,
            file_system,
            session_memory: Arc::new(SessionMemoryStore::default()),
            persistent_memory: Arc::new(InMemoryPersistentMemoryStore::default()),
            cache: Arc::new(Mutex::new(HashMap::new())),
            llm: Arc::new(LlmClient::mock_with_trace(Arc::clone(&trace))),
            trace,
            log_threshold: AtomicU8::new(1),
            async_handle_counter: AtomicU64::new(0),
            async_tasks: Arc::new(Mutex::new(HashMap::new())),
            event_queue_capacity: config::DEFAULT_EVENT_QUEUE_CAPACITY,
        })
    }

    pub fn current_log_threshold(&self) -> u8 {
        self.log_threshold.load(Ordering::Relaxed)
    }

    /// Sets this runtime's log threshold.
    ///
    /// Returns `false` if `name` is not a recognized level.
    pub fn set_log_threshold(&self, name: &str) -> bool {
        match log_level_rank(name) {
            Some(rank) => {
                self.log_threshold.store(rank, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub fn trace_enabled(&self) -> bool {
        self.trace.load(Ordering::Relaxed)
    }

    /// Enable or disable LLM call narration at runtime.
    ///
    /// Because the trace flag is shared with the LLM client via
    /// `Arc<AtomicBool>`, a call to `set_trace` is immediately visible
    /// to every `Ai.*` operation that follows.
    pub fn set_trace(&self, on: bool) {
        self.trace.store(on, Ordering::Relaxed);
    }

    pub fn event_queue_capacity(&self) -> usize {
        self.event_queue_capacity
    }

    pub fn next_async_handle_id(&self) -> u64 {
        self.async_handle_counter.fetch_add(1, Ordering::Relaxed)
    }

    pub fn insert_async_task(&self, id: u64, handle: tokio::task::JoinHandle<AsyncTaskResult>) {
        self.async_tasks.lock().insert(id, handle);
    }

    pub fn take_async_task(&self, id: u64) -> Option<tokio::task::JoinHandle<AsyncTaskResult>> {
        self.async_tasks.lock().remove(&id)
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn session_memory_is_instance_scoped() {
        use crate::interpreter::value::Value;

        let first = SessionMemoryStore::default();
        let second = SessionMemoryStore::default();

        first.remember(
            "Agent",
            "key".to_string(),
            Value::String("first".to_string()),
        );

        assert_eq!(
            first.recall("Agent", "key"),
            Value::String("first".to_string())
        );
        assert_eq!(second.recall("Agent", "key"), Value::None);
    }

    #[test]
    fn in_memory_file_system_round_trips_files() {
        use std::path::Path;

        let fs = InMemoryFileSystem::new();
        let path = Path::new("tmp/data.txt");

        fs.write_string(path, "hello").expect("write");

        assert!(fs.exists(path));
        assert_eq!(fs.read_to_string(path).expect("read"), "hello");
        assert_eq!(
            fs.read_dir_names(Path::new("tmp")).expect("list"),
            vec!["data.txt".to_string()]
        );
    }

    #[test]
    fn in_memory_persistent_memory_is_program_scoped() {
        use crate::interpreter::value::Value;

        let store = InMemoryPersistentMemoryStore::default();

        store
            .remember("one", "Agent", "k".to_string(), Value::Integer(1))
            .expect("remember");

        assert_eq!(
            store.recall("one", "Agent", "k").expect("recall"),
            Value::Integer(1)
        );
        assert_eq!(
            store.recall("two", "Agent", "k").expect("recall"),
            Value::None
        );
    }

    #[test]
    fn map_env_drives_llm_configuration() {
        let env = MapEnv::with(&[
            ("OLLAMA_HOST", "http://example.test"),
            ("KEEL_MODEL_FAST", "llama3"),
        ]);
        let llm = LlmClient::from_env(&env);

        assert_eq!(
            llm.describe_model("fast"),
            "llama3 (ollama @ http://example.test)"
        );
    }

    #[test]
    fn persistent_recall_does_not_rename_corrupt_file_under_shared_lock() {
        let home = tempfile::tempdir().expect("tempdir");
        let home_str = home.path().to_str().expect("utf-8 tempdir");
        let env = Arc::new(MapEnv::with(&[
            ("HOME", home_str),
            ("USERPROFILE", home_str),
        ]));
        let store = NativePersistentMemoryStore::new(env);
        let dir = home.path().join(".keel").join("memory").join("program");
        std::fs::create_dir_all(&dir).expect("create memory dir");
        let json_path = dir.join("Agent.json");
        let bak_path = dir.join("Agent.json.bak");
        std::fs::write(&json_path, b"not json").expect("write corrupt memory");

        let err = store
            .recall("program", "Agent", "key")
            .expect_err("recall fails");

        assert!(
            err.to_string().contains("is corrupt"),
            "unexpected error: {err}"
        );
        assert!(
            json_path.exists(),
            "shared recall must not rename data file"
        );
        assert!(
            !bak_path.exists(),
            "shared recall must not create backup file"
        );
    }

    #[test]
    fn trace_flag_is_shared_between_runtime_context_and_llm_client() {
        use std::sync::atomic::Ordering;

        use chrono::Utc;

        let env = Arc::new(MapEnv::with(&[]));
        let clock = Arc::new(FixedClock::new(Utc::now()));
        let fs = Arc::new(InMemoryFileSystem::new());

        let rt = RuntimeContext::test_context(env, clock, fs);

        // Both should start disabled (default).
        assert!(!rt.trace_enabled());
        assert!(!rt.llm.trace_flag().load(Ordering::Relaxed));

        // Enabling trace on the runtime context must be visible
        // through the LLM client's shared flag.
        rt.set_trace(true);
        assert!(rt.trace_enabled());
        assert!(rt.llm.trace_flag().load(Ordering::Relaxed));

        // Disabling must likewise propagate.
        rt.set_trace(false);
        assert!(!rt.trace_enabled());
        assert!(!rt.llm.trace_flag().load(Ordering::Relaxed));

        // The two Arcs must point to the same allocation.
        assert!(Arc::ptr_eq(&rt.llm.trace_flag(), &rt.trace));
    }
}

#[cfg(test)]
mod runtime_config_tests {
    use super::{
        FileSystem, InMemoryFileSystem, InMemoryPersistentMemoryStore, MapEnv,
        PersistentMemoryStore, RuntimeConfig, RuntimeContext, log_level_name,
    };
    use crate::interpreter::value::Value;
    use std::path::Path;

    #[test]
    fn runtime_config_reads_initial_process_inputs() {
        let env = MapEnv::with(&[("KEEL_TRACE", "1"), ("KEEL_LOG_LEVEL", "warn")]);
        let config = RuntimeConfig::from_env(&env);

        assert!(config.trace_enabled());
        assert_eq!(config.log_threshold(), 2);
    }

    #[test]
    fn runtime_settings_are_isolated_per_context() {
        let mut config_a = RuntimeConfig::from_env(&MapEnv::with(&[]));
        assert!(config_a.set_log_threshold("debug"));
        config_a.set_trace(true);

        let mut config_b = RuntimeConfig::from_env(&MapEnv::with(&[]));
        assert!(config_b.set_log_threshold("error"));
        config_b.set_trace(false);

        let runtime_a = RuntimeContext::native_with_config(config_a);
        let runtime_b = RuntimeContext::native_with_config(config_b);

        assert!(runtime_a.trace_enabled());
        assert!(!runtime_b.trace_enabled());
        assert_eq!(runtime_a.current_log_threshold(), 0);
        assert_eq!(runtime_b.current_log_threshold(), 3);

        assert!(runtime_a.set_log_threshold("warn"));
        assert_eq!(runtime_a.current_log_threshold(), 2);
        assert_eq!(
            runtime_b.current_log_threshold(),
            3,
            "mutating one runtime must not affect another runtime"
        );
    }

    #[test]
    fn invalid_log_level_does_not_mutate_context() {
        let runtime =
            RuntimeContext::native_with_config(RuntimeConfig::from_env(&MapEnv::with(&[])));

        assert!(runtime.set_log_threshold("WARNING"));
        assert_eq!(runtime.current_log_threshold(), 2);
        assert!(!runtime.set_log_threshold("louder"));
        assert_eq!(
            runtime.current_log_threshold(),
            2,
            "invalid level must not mutate"
        );
    }

    #[test]
    fn async_handle_ids_are_isolated_per_context() {
        let runtime_a =
            RuntimeContext::native_with_config(RuntimeConfig::from_env(&MapEnv::with(&[])));
        let runtime_b =
            RuntimeContext::native_with_config(RuntimeConfig::from_env(&MapEnv::with(&[])));

        assert_eq!(runtime_a.next_async_handle_id(), 0);
        assert_eq!(runtime_a.next_async_handle_id(), 1);
        assert_eq!(
            runtime_b.next_async_handle_id(),
            0,
            "a separate runtime must start its own task handle sequence"
        );
    }

    #[test]
    fn log_level_names_are_canonicalized_from_rank() {
        assert_eq!(log_level_name(0), "debug");
        assert_eq!(log_level_name(1), "info");
        assert_eq!(log_level_name(2), "warn");
        assert_eq!(log_level_name(3), "error");
        assert_eq!(log_level_name(99), "error");
    }

    #[test]
    fn runtime_config_ignores_invalid_env_log_level() {
        let env = MapEnv::with(&[("KEEL_TRACE", "0"), ("KEEL_LOG_LEVEL", "verbose")]);
        let config = RuntimeConfig::from_env(&env);

        assert!(!config.trace_enabled());
        assert_eq!(
            config.log_threshold(),
            1,
            "invalid env level should fall back to info"
        );
    }

    #[test]
    fn in_memory_file_system_handles_relative_paths_and_missing_directories() {
        let fs = InMemoryFileSystem::new();
        let path = Path::new("nested/main.keel");

        fs.write_string(path, "agent A {}")
            .expect("write in-memory file");

        assert!(fs.exists(path));
        assert_eq!(
            fs.read_to_string(Path::new("/nested/main.keel"))
                .expect("read normalized absolute path"),
            "agent A {}"
        );
        assert_eq!(
            fs.canonicalize(path).expect("canonicalize relative path"),
            Path::new("/nested/main.keel")
        );
        let names = fs
            .read_dir_names(Path::new("/nested"))
            .expect("read parent directory");
        assert_eq!(names, vec!["main.keel".to_string()]);
        assert!(
            fs.read_dir_names(Path::new("/missing")).is_err(),
            "missing in-memory directory should report NotFound"
        );
    }

    #[test]
    fn in_memory_persistent_store_rejects_invalid_agent_names() {
        let store = InMemoryPersistentMemoryStore::default();

        let err = store
            .remember(
                "program",
                "../bad",
                "key".to_string(),
                Value::String("v".into()),
            )
            .expect_err("invalid agent path component must fail");

        assert!(
            err.to_string().contains("invalid agent name"),
            "expected path safety diagnostic, got: {err}"
        );
    }

    #[test]
    fn in_memory_persistent_store_forget_missing_key_is_idempotent() {
        let store = InMemoryPersistentMemoryStore::default();

        store
            .forget("program", "Agent", "missing")
            .expect("forgetting missing key should be harmless");

        let value = store
            .recall("program", "Agent", "missing")
            .expect("recall missing key");
        assert_eq!(value, Value::None);
    }
}
