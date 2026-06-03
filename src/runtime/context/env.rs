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

#[cfg(any(test, feature = "test-util"))]
use std::collections::HashMap;

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
