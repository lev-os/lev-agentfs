use async_trait::async_trait;
use lev_reactive::{AsyncHook, HookContext, HookDecision, Plugin, PluginMetadata};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// LevFS Workflow Plugin - spawns Flowmind CLI workflows for hook events
pub struct LevFSWorkflow {
    workflow_name: String,
}

impl LevFSWorkflow {
    pub fn new(workflow_name: impl Into<String>) -> Self {
        Self {
            workflow_name: workflow_name.into(),
        }
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

impl Plugin for LevFSWorkflow {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "levfs-workflow".to_string(),
            version: "0.1.0".to_string(),
            author: Some("Leviathan".to_string()),
            description: Some("Async hook plugin that spawns Flowmind workflows".to_string()),
        }
    }

    fn async_hooks(&self) -> Vec<Box<dyn AsyncHook>> {
        vec![Box::new(WorkflowHook {
            workflow_name: self.workflow_name.clone(),
        })]
    }
}

/// Async hook that executes Flowmind workflows
struct WorkflowHook {
    workflow_name: String,
}

#[async_trait]
impl AsyncHook for WorkflowHook {
    fn name(&self) -> &str {
        "flowmind-workflow"
    }

    async fn execute(&self, context: &HookContext) -> lev_reactive::Result<HookDecision> {
        let workflow_name = self.workflow_name.clone();
        let context_json = serde_json::to_string(context).map_err(|e| {
            lev_reactive::LevError::Serialization(e)
        })?;

        // Spawn async workflow execution without blocking
        tokio::spawn(async move {
            if let Err(e) = execute_workflow(&workflow_name, &context_json).await {
                tracing::error!(
                    workflow = %workflow_name,
                    error = %e,
                    "Workflow execution failed"
                );
            }
        });

        // Return Allow immediately - workflow runs in background
        Ok(HookDecision::Allow)
    }

    fn priority(&self) -> i32 {
        100 // High priority to ensure workflows trigger early
    }
}

/// Execute Flowmind CLI with workflow name and context
async fn execute_workflow(workflow_name: &str, context_json: &str) -> anyhow::Result<()> {
    tracing::info!(
        workflow = %workflow_name,
        "Spawning Flowmind workflow"
    );

    let mut child = Command::new("flowmind")
        .arg("run")
        .arg(workflow_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Write context as JSON to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(context_json.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin);
    }

    // Wait for completion and capture output
    let output = child.wait_with_output().await?;

    if output.status.success() {
        tracing::info!(
            workflow = %workflow_name,
            "Workflow completed successfully"
        );
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!(
            workflow = %workflow_name,
            stderr = %stderr,
            "Workflow failed"
        );
        anyhow::bail!("Workflow execution failed: {}", stderr);
    }

    Ok(())
}

/// C ABI entry point for dynamic plugin loading
#[no_mangle]
pub extern "C" fn _plugin_create() -> *mut dyn Plugin {
    let plugin = LevFSWorkflow::new("default-workflow");
    Box::into_raw(Box::new(plugin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let plugin = LevFSWorkflow::new("test-workflow");
        let metadata = plugin.metadata();

        assert_eq!(metadata.name, "levfs-workflow");
        assert_eq!(metadata.version, "0.1.0");
    }

    #[test]
    fn test_async_hooks_registration() {
        let plugin = LevFSWorkflow::new("test-workflow");
        let hooks = plugin.async_hooks();

        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].name(), "flowmind-workflow");
    }

    #[tokio::test]
    async fn test_hook_returns_allow() {
        let hook = WorkflowHook {
            workflow_name: "test".to_string(),
        };

        let context = HookContext::new(
            "test-event",
            json!({"key": "value"})
        );

        let result = hook.execute(&context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), HookDecision::Allow);
    }
}
