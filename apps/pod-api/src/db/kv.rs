use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::ApiError;

/// Abstraction over a key-value store used for tokens, auth codes, etc.
///
/// Backed by Redis in production and an in-memory map in tests.
#[async_trait]
pub trait KeyValueStore: Send + Sync {
    async fn set_ex(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), ApiError>;
    async fn get(&self, key: &str) -> Result<Option<String>, ApiError>;
    async fn del(&self, key: &str) -> Result<(), ApiError>;
}

// ---------------------------------------------------------------------------
// In-memory implementation (for Phase 1 / tests)
// ---------------------------------------------------------------------------

pub struct MemoryStore {
    data: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl KeyValueStore for MemoryStore {
    async fn set_ex(&self, key: &str, value: &str, _ttl_secs: u64) -> Result<(), ApiError> {
        self.data
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, ApiError> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }

    async fn del(&self, key: &str) -> Result<(), ApiError> {
        self.data.lock().unwrap().remove(key);
        Ok(())
    }
}
