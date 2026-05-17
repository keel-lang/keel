use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

use crate::interpreter::value::Value;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use parking_lot::Mutex;

use super::llm::LlmClient;

/// Pre-computed rank for the default log level `"info"`, matching `log_level_rank("info")`.
/// Avoids an unnecessary `.unwrap()` at every `RuntimeConfig::from_env` call.
const DEFAULT_LOG_RANK: u8 = 1;

pub fn log_level_rank(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "debug" => Some(0),
        "info" => Some(1),
        "warn" | "warning" => Some(2),
        "error" => Some(3),
        _ => None,
    }
}

pub fn log_level_name(rank: u8) -> &'static str {
    match rank {
        0 => "debug",
        1 => "info",
        2 => "warn",
        _ => "error",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    trace: bool,
    log_threshold: u8,
}

impl RuntimeConfig {
    pub fn from_env(env: &dyn EnvProvider) -> Self {
        let trace = env.var("KEEL_TRACE").as_deref() == Some("1");
        let log_threshold = env
            .var("KEEL_LOG_LEVEL")
            .and_then(|s| log_level_rank(&s))
            .unwrap_or(DEFAULT_LOG_RANK);

        Self {
            trace,
            log_threshold,
        }
    }

    pub fn trace_enabled(&self) -> bool {
        self.trace
    }

    pub fn set_trace(&mut self, on: bool) {
        self.trace = on;
    }

    pub fn log_threshold(&self) -> u8 {
        self.log_threshold
    }

    pub fn set_log_threshold(&mut self, name: &str) -> bool {
        match log_level_rank(name) {
            Some(rank) => {
                self.log_threshold = rank;
                true
            }
            None => false,
        }
    }
}

pub trait EnvProvider: Send + Sync {
    fn var(&self, name: &str) -> Option<String>;
    fn vars(&self) -> Vec<(String, String)>;
}

#[derive(Default)]
pub struct NativeEnv;

impl EnvProvider for NativeEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn vars(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
}

pub trait Clock: Send + Sync {
    fn now_utc(&self) -> DateTime<Utc>;
    fn now_instant(&self) -> Instant;
}

#[derive(Default)]
pub struct NativeClock;

impl Clock for NativeClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn now_instant(&self) -> Instant {
        Instant::now()
    }
}

pub trait FileSystem: Send + Sync {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    fn write_string(&self, path: &Path, content: &str) -> std::io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn read_dir_names(&self, path: &Path) -> std::io::Result<Vec<String>>;
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
    fn mkdir(&self, path: &Path) -> std::io::Result<()>;
    fn remove(&self, path: &Path) -> std::io::Result<()>;
    fn copy_file(&self, src: &Path, dst: &Path) -> std::io::Result<()>;
    fn move_path(&self, src: &Path, dst: &Path) -> std::io::Result<()>;
    fn glob(&self, pattern: &str) -> std::io::Result<Vec<String>>;
    fn mktemp(&self, is_dir: bool) -> std::io::Result<String>;
}

#[derive(Default)]
pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_string(&self, path: &Path, content: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    fn read_dir_names(&self, path: &Path) -> std::io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if let Ok(name) = entry.file_name().into_string() {
                names.push(name);
            }
        }
        Ok(names)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path).or_else(|_| std::path::absolute(path))
    }

    fn mkdir(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn remove(&self, path: &Path) -> std::io::Result<()> {
        let meta = std::fs::metadata(path)?;
        if meta.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        if let Some(parent) = dst.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst).map(|_| ())
    }

    fn move_path(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        if let Some(parent) = dst.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(src, dst)
    }

    fn glob(&self, pattern: &str) -> std::io::Result<Vec<String>> {
        let entries = ::glob::glob(pattern)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.msg))?;
        let mut paths = Vec::new();
        for entry in entries {
            let p = entry.map_err(|e| std::io::Error::other(e.to_string()))?;
            if let Some(s) = p.to_str() {
                paths.push(s.to_string());
            }
        }
        Ok(paths)
    }

    fn mktemp(&self, is_dir: bool) -> std::io::Result<String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("keel-{pid}-{n}"));
        if is_dir {
            std::fs::create_dir(&path)?;
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
        }
        Ok(path.to_string_lossy().into_owned())
    }
}

#[derive(Default)]
pub struct InMemoryFileSystem {
    files: Mutex<HashMap<PathBuf, String>>,
    dirs: Mutex<BTreeSet<PathBuf>>,
}

impl InMemoryFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    fn normalize(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        }
    }
}

impl FileSystem for InMemoryFileSystem {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        let path = Self::normalize(path);
        self.files
            .lock()
            .get(&path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"))
    }

    fn write_string(&self, path: &Path, content: &str) -> std::io::Result<()> {
        let path = Self::normalize(path);
        if let Some(parent) = path.parent() {
            self.dirs.lock().insert(parent.to_path_buf());
        }
        self.files.lock().insert(path, content.to_string());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let path = Self::normalize(path);
        self.files.lock().contains_key(&path) || self.dirs.lock().contains(&path)
    }

    fn read_dir_names(&self, path: &Path) -> std::io::Result<Vec<String>> {
        let path = Self::normalize(path);
        let mut names = BTreeSet::new();
        for file in self.files.lock().keys() {
            if let Some(parent) = file.parent()
                && parent == path
                && let Some(name) = file.file_name().and_then(|n| n.to_str())
            {
                names.insert(name.to_string());
            }
        }
        if names.is_empty() && !self.dirs.lock().contains(&path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "directory not found",
            ));
        }
        Ok(names.into_iter().collect())
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(Self::normalize(path))
    }

    fn mkdir(&self, path: &Path) -> std::io::Result<()> {
        let path = Self::normalize(path);
        // Insert the directory and all its ancestors.
        let mut dirs = self.dirs.lock();
        let mut current = path.as_path();
        loop {
            dirs.insert(current.to_path_buf());
            match current.parent() {
                Some(p) if p != current => current = p,
                _ => break,
            }
        }
        Ok(())
    }

    fn remove(&self, path: &Path) -> std::io::Result<()> {
        let path = Self::normalize(path);
        let mut files = self.files.lock();
        let mut dirs = self.dirs.lock();
        // Remove a single file.
        if files.remove(&path).is_some() {
            return Ok(());
        }
        // Remove a directory and everything under it (rm -rf semantics).
        if dirs.contains(&path) {
            let file_keys: Vec<PathBuf> = files
                .keys()
                .filter(|k| k.starts_with(&path))
                .cloned()
                .collect();
            for k in file_keys {
                files.remove(&k);
            }
            let dir_keys: Vec<PathBuf> = dirs
                .iter()
                .filter(|k| k.starts_with(&path))
                .cloned()
                .collect();
            for k in dir_keys {
                dirs.remove(&k);
            }
            return Ok(());
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "path not found",
        ))
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        let src = Self::normalize(src);
        let dst = Self::normalize(dst);
        let content =
            self.files.lock().get(&src).cloned().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "source not found")
            })?;
        if let Some(parent) = dst.parent() {
            self.dirs.lock().insert(parent.to_path_buf());
        }
        self.files.lock().insert(dst, content);
        Ok(())
    }

    fn move_path(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        let src = Self::normalize(src);
        let dst = Self::normalize(dst);
        let content =
            self.files.lock().remove(&src).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "source not found")
            })?;
        if let Some(parent) = dst.parent() {
            self.dirs.lock().insert(parent.to_path_buf());
        }
        self.files.lock().insert(dst, content);
        Ok(())
    }

    fn glob(&self, pattern: &str) -> std::io::Result<Vec<String>> {
        let pat = ::glob::Pattern::new(pattern)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.msg))?;
        let files = self.files.lock();
        let mut matched: Vec<String> = files
            .keys()
            .filter_map(|p| {
                let s = p.to_str()?;
                // Match against the full absolute path or, for relative patterns,
                // also strip the leading `/` prefix used by InMemoryFileSystem.
                let candidate = s.trim_start_matches('/');
                if pat.matches(s) || pat.matches(candidate) {
                    Some(candidate.to_string())
                } else {
                    None
                }
            })
            .collect();
        matched.sort();
        Ok(matched)
    }

    fn mktemp(&self, is_dir: bool) -> std::io::Result<String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = PathBuf::from(format!("/tmp/keel-mock-{n}"));
        if is_dir {
            self.dirs.lock().insert(path.clone());
        } else {
            self.files.lock().insert(path.clone(), String::new());
        }
        Ok(path.to_string_lossy().into_owned())
    }
}

#[derive(Default)]
pub struct SessionMemoryStore {
    values: Mutex<HashMap<String, HashMap<String, Value>>>,
}

impl SessionMemoryStore {
    pub fn remember(&self, agent: &str, key: String, value: Value) {
        self.values
            .lock()
            .entry(agent.to_string())
            .or_default()
            .insert(key, value);
    }

    pub fn recall(&self, agent: &str, key: &str) -> Value {
        self.values
            .lock()
            .get(agent)
            .and_then(|values| values.get(key))
            .cloned()
            .unwrap_or(Value::None)
    }

    pub fn forget(&self, agent: &str, key: &str) {
        self.values
            .lock()
            .entry(agent.to_string())
            .or_default()
            .remove(key);
    }
}

pub trait PersistentMemoryStore: Send + Sync {
    fn remember(&self, program: &str, agent: &str, key: String, value: Value)
    -> miette::Result<()>;
    fn recall(&self, program: &str, agent: &str, key: &str) -> miette::Result<Value>;
    fn forget(&self, program: &str, agent: &str, key: &str) -> miette::Result<()>;
}

pub struct NativePersistentMemoryStore {
    env: Arc<dyn EnvProvider>,
}

impl NativePersistentMemoryStore {
    pub fn new(env: Arc<dyn EnvProvider>) -> Self {
        Self { env }
    }

    fn persistent_memory_dir(&self, program_name: &str) -> PathBuf {
        let home = self
            .env
            .var("HOME")
            .or_else(|| self.env.var("USERPROFILE"))
            .unwrap_or_else(|| ".".to_string());
        PathBuf::from(home)
            .join(".keel")
            .join("memory")
            .join(program_name)
    }

    fn with_locked<R>(
        &self,
        program: &str,
        agent: &str,
        exclusive: bool,
        body: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> miette::Result<R>,
    ) -> miette::Result<R> {
        validate_memory_path_component(agent, "agent")?;
        let dir = self.persistent_memory_dir(program);
        std::fs::create_dir_all(&dir).map_err(|e| {
            miette::miette!("Memory: failed to create directory {}: {e}", dir.display())
        })?;

        let lock_path = dir.join(format!("{agent}.lock"));
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                miette::miette!(
                    "Memory: failed to open lock file {}: {e}",
                    lock_path.display()
                )
            })?;

        if exclusive {
            lock_file
                .lock_exclusive()
                .map_err(|e| miette::miette!("Memory: failed to acquire exclusive lock: {e}"))?;
        } else {
            lock_file
                .lock_shared()
                .map_err(|e| miette::miette!("Memory: failed to acquire shared lock: {e}"))?;
        }

        let data_path = dir.join(format!("{agent}.json"));
        if exclusive {
            let _ = std::fs::remove_file(data_path.with_extension("json.tmp"));
        }

        let mut store = memory_read_or_empty(&data_path, exclusive)?;
        let result = body(&mut store)?;
        if exclusive {
            memory_write_atomic(&data_path, &store)?;
        }
        Ok(result)
    }
}

impl PersistentMemoryStore for NativePersistentMemoryStore {
    fn remember(
        &self,
        program: &str,
        agent: &str,
        key: String,
        value: Value,
    ) -> miette::Result<()> {
        self.with_locked(program, agent, true, move |store| {
            store.insert(key, super::value_to_json(&value));
            Ok(())
        })
    }

    fn recall(&self, program: &str, agent: &str, key: &str) -> miette::Result<Value> {
        self.with_locked(program, agent, false, move |store| {
            Ok(store
                .get(key)
                .map(super::json_to_value)
                .unwrap_or(Value::None))
        })
    }

    fn forget(&self, program: &str, agent: &str, key: &str) -> miette::Result<()> {
        self.with_locked(program, agent, true, move |store| {
            store.remove(key);
            Ok(())
        })
    }
}

#[derive(Default)]
pub struct InMemoryPersistentMemoryStore {
    values: Mutex<HashMap<(String, String), HashMap<String, Value>>>,
}

impl PersistentMemoryStore for InMemoryPersistentMemoryStore {
    fn remember(
        &self,
        program: &str,
        agent: &str,
        key: String,
        value: Value,
    ) -> miette::Result<()> {
        validate_memory_path_component(agent, "agent")?;
        self.values
            .lock()
            .entry((program.to_string(), agent.to_string()))
            .or_default()
            .insert(key, value);
        Ok(())
    }

    fn recall(&self, program: &str, agent: &str, key: &str) -> miette::Result<Value> {
        validate_memory_path_component(agent, "agent")?;
        Ok(self
            .values
            .lock()
            .get(&(program.to_string(), agent.to_string()))
            .and_then(|values| values.get(key))
            .cloned()
            .unwrap_or(Value::None))
    }

    fn forget(&self, program: &str, agent: &str, key: &str) -> miette::Result<()> {
        validate_memory_path_component(agent, "agent")?;
        self.values
            .lock()
            .entry((program.to_string(), agent.to_string()))
            .or_default()
            .remove(key);
        Ok(())
    }
}

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
}

pub type CacheEntry = (Value, Option<Instant>);
pub type CacheHandle = Arc<Mutex<HashMap<String, CacheEntry>>>;
pub type AsyncTaskResult = Result<Value, String>;
pub type AsyncTaskHandle = Arc<Mutex<HashMap<u64, tokio::task::JoinHandle<AsyncTaskResult>>>>;

impl RuntimeContext {
    pub fn native() -> Arc<Self> {
        let env: Arc<dyn EnvProvider> = Arc::new(NativeEnv);
        let config = RuntimeConfig::from_env(env.as_ref());
        Self::native_with_config(config)
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

    pub fn next_async_handle_id(&self) -> u64 {
        self.async_handle_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert_async_task(&self, id: u64, handle: tokio::task::JoinHandle<AsyncTaskResult>) {
        self.async_tasks.lock().insert(id, handle);
    }

    pub fn take_async_task(&self, id: u64) -> Option<tokio::task::JoinHandle<AsyncTaskResult>> {
        self.async_tasks.lock().remove(&id)
    }
}

fn validate_memory_path_component(name: &str, kind: &str) -> miette::Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(miette::miette!("Memory: invalid {kind} name {name:?}"));
    }
    Ok(())
}

fn memory_read_or_empty(
    path: &Path,
    recover_corrupt_file: bool,
) -> miette::Result<serde_json::Map<String, serde_json::Value>> {
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("Memory: failed to read {}: {e}", path.display()))?;
    match serde_json::from_str::<serde_json::Value>(&contents) {
        Ok(serde_json::Value::Object(m)) => Ok(m),
        _ => {
            let bak = path.with_extension("json.bak");
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("agent");
            if recover_corrupt_file {
                let _ = std::fs::rename(path, &bak);
                Err(miette::miette!(
                    "Memory: {stem}.json was corrupt; renamed to {} — starting fresh",
                    bak.display()
                ))
            } else {
                Err(miette::miette!(
                    "Memory: {stem}.json is corrupt; rerun a write operation to rename it to {}",
                    bak.display()
                ))
            }
        }
    }
}

fn memory_write_atomic(
    path: &Path,
    store: &serde_json::Map<String, serde_json::Value>,
) -> miette::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&serde_json::Value::Object(store.clone()))
        .map_err(|e| miette::miette!("Memory: serialization failed: {e}"))?;
    {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| {
                miette::miette!("Memory: failed to open temp file {}: {e}", tmp.display())
            })?;
        std::io::Write::write_all(&mut file, json.as_bytes())
            .map_err(|e| miette::miette!("Memory: failed to write {}: {e}", tmp.display()))?;
        file.sync_all()
            .map_err(|e| miette::miette!("Memory: failed to sync {}: {e}", tmp.display()))?;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(miette::miette!(
            "Memory: failed to rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        ));
    }
    if let Some(parent) = path.parent()
        && let Ok(dir) = std::fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(any(test, feature = "test-util"))]
pub struct FixedClock {
    utc: parking_lot::Mutex<DateTime<Utc>>,
    instant: parking_lot::Mutex<Instant>,
}

#[cfg(any(test, feature = "test-util"))]
impl FixedClock {
    pub fn new(utc: DateTime<Utc>) -> Self {
        Self {
            utc: parking_lot::Mutex::new(utc),
            instant: parking_lot::Mutex::new(Instant::now()),
        }
    }

    pub fn advance(&self, d: std::time::Duration) {
        let cd = chrono::Duration::from_std(d).unwrap_or(chrono::Duration::zero());
        {
            let mut utc = self.utc.lock();
            *utc += cd;
        }
        {
            let mut instant = self.instant.lock();
            *instant += d;
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Clock for FixedClock {
    fn now_utc(&self) -> DateTime<Utc> {
        *self.utc.lock()
    }
    fn now_instant(&self) -> Instant {
        *self.instant.lock()
    }
}

#[cfg(any(test, feature = "test-util"))]
#[derive(Default)]
pub struct MapEnv {
    values: HashMap<String, String>,
}

#[cfg(any(test, feature = "test-util"))]
impl MapEnv {
    pub fn with(values: &[(&str, &str)]) -> Self {
        Self {
            values: values
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl EnvProvider for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.values.get(name).cloned()
    }

    fn vars(&self) -> Vec<(String, String)> {
        self.values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

#[cfg(any(test, feature = "test-util"))]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn session_memory_is_instance_scoped() {
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
