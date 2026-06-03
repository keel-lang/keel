use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs2::FileExt;
use parking_lot::Mutex;

use crate::interpreter::value::Value;
use crate::runtime::{json_to_value, value_to_json};

use super::env::EnvProvider;

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
        validate_memory_path_component(program, "program")?;
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
            store.insert(key, value_to_json(&value));
            Ok(())
        })
    }

    fn recall(&self, program: &str, agent: &str, key: &str) -> miette::Result<Value> {
        self.with_locked(program, agent, false, move |store| {
            Ok(store.get(key).map(json_to_value).unwrap_or(Value::None))
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
        validate_memory_path_component(program, "program")?;
        validate_memory_path_component(agent, "agent")?;
        self.values
            .lock()
            .entry((program.to_string(), agent.to_string()))
            .or_default()
            .insert(key, value);
        Ok(())
    }

    fn recall(&self, program: &str, agent: &str, key: &str) -> miette::Result<Value> {
        validate_memory_path_component(program, "program")?;
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
        validate_memory_path_component(program, "program")?;
        validate_memory_path_component(agent, "agent")?;
        self.values
            .lock()
            .entry((program.to_string(), agent.to_string()))
            .or_default()
            .remove(key);
        Ok(())
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
