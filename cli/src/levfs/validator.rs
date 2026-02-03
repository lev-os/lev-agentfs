use lev_reactive::{HookContext, HookDecision, Result, SyncHook};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Frontmatter metadata extracted from files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    #[serde(flatten)]
    pub data: HashMap<String, serde_json::Value>,
}

/// Schema validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub name: String,
    pub required_fields: Vec<String>,
    pub optional_fields: Option<Vec<String>>,
    pub max_size: Option<usize>,
}

/// LevFS Validator Plugin
pub struct LevFSValidator {
    name: String,
    max_size: usize,
    schemas: HashMap<String, Schema>,
    schema_dir: PathBuf,
}

impl LevFSValidator {
    /// Create a new validator with default settings
    pub fn new() -> Self {
        let schema_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lev")
            .join("schemas");

        Self {
            name: "levfs-validator".to_string(),
            max_size: 10 * 1024 * 1024, // 10MB default
            schemas: HashMap::new(),
            schema_dir,
        }
    }

    /// Configure maximum file size
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// Configure schema directory
    pub fn with_schema_dir(mut self, dir: PathBuf) -> Self {
        self.schema_dir = dir;
        self
    }

    /// Parse YAML frontmatter from content
    pub fn parse_frontmatter(&self, content: &str) -> Result<Option<Frontmatter>> {
        // Check for YAML frontmatter delimiters (---)
        if !content.starts_with("---") {
            return Ok(None);
        }

        // Find end delimiter
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 3 {
            return Ok(None);
        }

        let mut end_idx = None;
        for (idx, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                end_idx = Some(idx);
                break;
            }
        }

        let end_idx = match end_idx {
            Some(idx) => idx,
            None => return Ok(None),
        };

        // Extract frontmatter content
        let frontmatter_content = lines[1..end_idx].join("\n");

        // Parse YAML
        let data: HashMap<String, serde_json::Value> = serde_yaml::from_str(&frontmatter_content)
            .map_err(|e| lev_reactive::LevError::ConfigError(format!("Invalid YAML: {}", e)))?;

        Ok(Some(Frontmatter { data }))
    }

    /// Load schema from file
    pub fn load_schema(&mut self, schema_name: &str) -> Result<()> {
        let schema_path = self.schema_dir.join(format!("{}.yaml", schema_name));

        if !schema_path.exists() {
            return Err(lev_reactive::LevError::ConfigError(format!(
                "Schema not found: {}",
                schema_path.display()
            )));
        }

        let content = fs::read_to_string(&schema_path)
            .map_err(|e| lev_reactive::LevError::Io(e))?;

        let schema: Schema = serde_yaml::from_str(&content)
            .map_err(|e| lev_reactive::LevError::ConfigError(format!("Invalid schema YAML: {}", e)))?;

        self.schemas.insert(schema_name.to_string(), schema);
        Ok(())
    }

    /// Validate frontmatter against schema
    fn validate_against_schema(&self, frontmatter: &Frontmatter, schema: &Schema) -> Result<HookDecision> {
        // Check required fields
        for field in &schema.required_fields {
            if !frontmatter.data.contains_key(field) {
                return Ok(HookDecision::Block {
                    reason: format!("Missing required field: {}", field),
                });
            }
        }

        // All required fields present
        Ok(HookDecision::Allow)
    }

    /// Check file size
    fn check_size(&self, size: usize) -> HookDecision {
        if size > self.max_size {
            HookDecision::Block {
                reason: format!(
                    "File size {} exceeds maximum {}",
                    size, self.max_size
                ),
            }
        } else if size > (self.max_size * 80 / 100) {
            // Warn at 80% threshold
            HookDecision::Warn {
                message: format!(
                    "File size {} approaching maximum {}",
                    size, self.max_size
                ),
            }
        } else {
            HookDecision::Allow
        }
    }

    /// Validate file content
    fn validate_content(&self, content: &str, schema_name: Option<&str>) -> Result<HookDecision> {
        // Parse frontmatter
        let frontmatter = self.parse_frontmatter(content)?;

        // If schema specified, validate against it
        if let (Some(schema_name), Some(ref frontmatter)) = (schema_name, frontmatter) {
            if let Some(schema) = self.schemas.get(schema_name) {
                return self.validate_against_schema(frontmatter, schema);
            } else {
                return Ok(HookDecision::Warn {
                    message: format!("Schema '{}' not loaded", schema_name),
                });
            }
        }

        Ok(HookDecision::Allow)
    }
}

impl Default for LevFSValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncHook for LevFSValidator {
    fn name(&self) -> &str {
        &self.name
    }

    fn execute(&self, context: &HookContext) -> Result<HookDecision> {
        // Extract file content from payload
        let content = context.payload.get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let size = context.payload.get("size")
            .and_then(|v| v.as_u64())
            .unwrap_or(content.len() as u64) as usize;

        let schema_name = context.metadata.get("schema")
            .map(|s| s.as_str());

        // Check size first
        let size_decision = self.check_size(size);
        if matches!(size_decision, HookDecision::Block { .. }) {
            return Ok(size_decision);
        }

        // Validate content
        let content_decision = self.validate_content(content, schema_name)?;

        // Return most severe decision
        match (&size_decision, &content_decision) {
            (HookDecision::Block { .. }, _) => Ok(size_decision),
            (_, HookDecision::Block { .. }) => Ok(content_decision),
            (HookDecision::Warn { .. }, HookDecision::Allow) => Ok(size_decision),
            (HookDecision::Allow, HookDecision::Warn { .. }) => Ok(content_decision),
            _ => Ok(HookDecision::Allow),
        }
    }

    fn priority(&self) -> i32 {
        100 // High priority - validate early
    }
}

// C ABI for dynamic loading
// Note: Using trait objects in FFI is not ideal but works for internal use
// A proper solution would use opaque pointers with vtable indirection
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn create_plugin() -> *mut dyn SyncHook {
    let validator = Box::new(LevFSValidator::new());
    Box::into_raw(validator)
}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn destroy_plugin(ptr: *mut dyn SyncHook) {
    if !ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_valid() {
        let validator = LevFSValidator::new();
        let content = r#"---
title: Test Document
author: Test User
tags: [test, example]
---

Document content here
"#;

        let result = validator.parse_frontmatter(content).unwrap();
        assert!(result.is_some());

        let frontmatter = result.unwrap();
        assert_eq!(frontmatter.data.get("title").unwrap().as_str().unwrap(), "Test Document");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let validator = LevFSValidator::new();
        let content = "Just regular content";

        let result = validator.parse_frontmatter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_size_check() {
        let validator = LevFSValidator::new().with_max_size(1000);

        // Under threshold
        assert!(matches!(validator.check_size(500), HookDecision::Allow));

        // Warning threshold
        assert!(matches!(validator.check_size(850), HookDecision::Warn { .. }));

        // Over limit
        assert!(matches!(validator.check_size(1500), HookDecision::Block { .. }));
    }
}
