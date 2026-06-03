use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::interpreter::value::Value;

pub type AsyncTaskResult = Result<Value, String>;
pub type AsyncTaskHandle = Arc<Mutex<HashMap<u64, tokio::task::JoinHandle<AsyncTaskResult>>>>;
