use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use crate::interpreter::value::Value;

pub type CacheEntry = (Value, Option<Instant>);
pub type CacheHandle = Arc<Mutex<HashMap<String, CacheEntry>>>;
