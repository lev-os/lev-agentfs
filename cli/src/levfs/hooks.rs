//! Reactive hook runner for AgentFS filesystem operations.
//!
//! Provides a `HookRunner` that can execute synchronous policy hooks (before an
//! operation) and asynchronous notification hooks (after an operation).  When no
//! hook configuration is present the runner is a no-op.

use serde_json::Value;

/// Errors returned by synchronous hook execution.
#[derive(Debug)]
pub enum HookRunError {
    /// The hook policy denied the operation.
    Denied(String),
    /// The hook failed to execute.
    Failed(String),
}

impl std::fmt::Display for HookRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "hook denied: {}", reason),
            Self::Failed(err) => write!(f, "hook failed: {}", err),
        }
    }
}

impl std::error::Error for HookRunError {}

/// Runs reactive hooks around filesystem operations.
///
/// Currently a no-op stub — real implementation will integrate with
/// `lev-reactive` for policy enforcement and event notification.
#[derive(Debug, Clone)]
pub struct HookRunner {
    _enabled: bool,
}

impl HookRunner {
    /// Create a disabled hook runner (all operations pass through).
    pub fn disabled() -> Self {
        Self { _enabled: false }
    }

    /// Create a hook runner configured from environment variables.
    ///
    /// Currently returns a disabled runner; will read `AGENTFS_HOOKS_*`
    /// env vars once the reactive integration is wired up.
    pub fn from_env() -> Self {
        // TODO: read hook config from env / XDG config
        Self::disabled()
    }

    /// Execute synchronous pre-operation hooks.
    ///
    /// Returns `Ok(())` if the operation is allowed, or a `HookRunError` if
    /// a policy hook denies or fails.
    pub fn before_event(
        &self,
        _source: &str,
        _event_type: &str,
        _payload: Value,
    ) -> Result<(), HookRunError> {
        // No-op when disabled
        Ok(())
    }

    /// Execute asynchronous post-operation hooks.
    ///
    /// Fire-and-forget notification; errors are logged but not propagated.
    pub async fn after_event(&self, _source: &str, _event_type: &str, _payload: Value) {
        // No-op when disabled
    }
}
