use std::collections::HashMap;

use std::path::Path;

use keel_lang::interpreter::value::Value;
use keel_lang::runtime::context::{
    EnvProvider, FileSystem, InMemoryFileSystem, InMemoryPersistentMemoryStore,
    PersistentMemoryStore, RuntimeConfig, RuntimeContext, log_level_name,
};

struct TestEnv {
    values: HashMap<String, String>,
}

impl TestEnv {
    fn new(values: &[(&str, &str)]) -> Self {
        Self {
            values: values
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }
}

impl EnvProvider for TestEnv {
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

#[test]
fn runtime_config_reads_initial_process_inputs() {
    let env = TestEnv::new(&[("KEEL_TRACE", "1"), ("KEEL_LOG_LEVEL", "warn")]);
    let config = RuntimeConfig::from_env(&env);

    assert!(config.trace_enabled());
    assert_eq!(config.log_threshold(), 2);
}

#[test]
fn runtime_settings_are_isolated_per_context() {
    let mut config_a = RuntimeConfig::from_env(&TestEnv::new(&[]));
    assert!(config_a.set_log_threshold("debug"));
    config_a.set_trace(true);

    let mut config_b = RuntimeConfig::from_env(&TestEnv::new(&[]));
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
    let runtime = RuntimeContext::native_with_config(RuntimeConfig::from_env(&TestEnv::new(&[])));

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
    let runtime_a = RuntimeContext::native_with_config(RuntimeConfig::from_env(&TestEnv::new(&[])));
    let runtime_b = RuntimeContext::native_with_config(RuntimeConfig::from_env(&TestEnv::new(&[])));

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
    let env = TestEnv::new(&[("KEEL_TRACE", "0"), ("KEEL_LOG_LEVEL", "verbose")]);
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
