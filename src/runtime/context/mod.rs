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
pub use cache::{CacheEntry, CacheHandle};
pub use clock::{Clock, NativeClock};
pub use config::{RuntimeConfig, log_level_name, log_level_rank};
pub use env::{EnvProvider, NativeEnv};
pub use fs::{FileSystem, InMemoryFileSystem, NativeFileSystem};
pub use memory::{
    InMemoryPersistentMemoryStore, NativePersistentMemoryStore, PersistentMemoryStore,
    SessionMemoryStore,
};

#[cfg(any(test, feature = "test-util"))]
pub use clock::FixedClock;
#[cfg(any(test, feature = "test-util"))]
pub use env::MapEnv;

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

    /// Construct a runtime context with mocked backends for deterministic
    /// unit testing. Only available in test builds.
    #[cfg(any(test, feature = "test-util"))]
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

#[cfg(any(test, feature = "test-util"))]
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
