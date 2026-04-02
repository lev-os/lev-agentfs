use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HookDecision {
    Allow,
    Deny,
    AllowWithMessage(String),
    Block { reason: String },
    Transform(serde_json::Value),
    Warn { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    pub event_type: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, String>,
    pub timestamp: u64,
}

impl HookContext {
    pub fn new(event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        let payload = payload;
        Self {
            event_type: event_type.into(),
            source: String::new(),
            data: payload.clone(),
            payload,
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_secs(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

pub trait SyncHook: Send + Sync {
    fn name(&self) -> &str;
    fn execute(&self, context: &HookContext) -> Result<HookDecision>;
    fn priority(&self) -> i32 {
        0
    }
}

#[async_trait]
pub trait AsyncHook: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, context: &HookContext) -> Result<HookDecision>;
    fn priority(&self) -> i32 {
        0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    fn sync_hooks(&self) -> Vec<Box<dyn SyncHook>> {
        Vec::new()
    }
    fn async_hooks(&self) -> Vec<Box<dyn AsyncHook>> {
        Vec::new()
    }
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct HookRegistry {
    sync_hooks: Arc<RwLock<Vec<Arc<dyn SyncHook>>>>,
    async_hooks: Arc<RwLock<Vec<Arc<dyn AsyncHook>>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_sync<H>(&self, hook: H)
    where
        H: SyncHook + 'static,
    {
        self.sync_hooks
            .write()
            .expect("sync_hooks lock poisoned")
            .push(Arc::new(hook));
    }

    pub fn register_async<H>(&self, hook: H)
    where
        H: AsyncHook + 'static,
    {
        self.async_hooks
            .write()
            .expect("async_hooks lock poisoned")
            .push(Arc::new(hook));
    }

    pub fn execute_sync(&self, context: &HookContext) -> Result<HookDecision> {
        let hooks = self.sync_hooks.read().expect("sync_hooks lock poisoned");
        let mut ordered: Vec<_> = hooks.iter().cloned().collect();
        ordered.sort_by_key(|hook| Reverse(hook.priority()));

        for hook in ordered {
            match hook.execute(context)? {
                HookDecision::Allow | HookDecision::AllowWithMessage(_) => continue,
                HookDecision::Warn { message } => {
                    tracing::warn!(hook = hook.name(), message = %message, "hook warning");
                }
                HookDecision::Deny => return Ok(HookDecision::Deny),
                HookDecision::Block { reason } => return Ok(HookDecision::Block { reason }),
                HookDecision::Transform(data) => return Ok(HookDecision::Transform(data)),
            }
        }

        Ok(HookDecision::Allow)
    }

    pub async fn execute_async(&self, context: &HookContext) -> Result<HookDecision> {
        let hooks = self
            .async_hooks
            .read()
            .expect("async_hooks lock poisoned")
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut ordered = hooks;
        ordered.sort_by_key(|hook| Reverse(hook.priority()));

        for hook in ordered {
            match hook.execute(context).await? {
                HookDecision::Allow | HookDecision::AllowWithMessage(_) => continue,
                HookDecision::Warn { message } => {
                    tracing::warn!(hook = hook.name(), message = %message, "hook warning");
                }
                HookDecision::Deny => return Ok(HookDecision::Deny),
                HookDecision::Block { reason } => return Ok(HookDecision::Block { reason }),
                HookDecision::Transform(data) => return Ok(HookDecision::Transform(data)),
            }
        }

        Ok(HookDecision::Allow)
    }
}

#[derive(Error, Debug)]
pub enum LevError {
    #[error("Hook execution failed: {0}")]
    HookFailed(String),
    #[error("Plugin load failed: {0}")]
    PluginLoadFailed(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Event routing error: {0}")]
    RoutingError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LevError>;
