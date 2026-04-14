//! Tool registry module
//! 
//! This module provides a centralized registry for managing tool handlers.
//! It handles tool registration, filtering, and execution.

use crate::config::CustomTool;
use crate::types::ToolHandler;
use serde_json::Value;
use async_openai::types::chat::ChatCompletionTool;

/// Centralized registry for all tool handlers
pub struct ToolRegistry {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new ToolRegistry with all available tools
    pub fn new() -> Self {
        Self::with_custom_tools(&std::collections::HashMap::new())
    }

    /// Create a new ToolRegistry with custom tools from config
    pub fn with_custom_tools(custom_tools: &std::collections::HashMap<String, CustomTool>) -> Self {
        let mut handlers: Vec<Box<dyn ToolHandler>> = vec![
            Box::new(crate::tools::read_file::ReadFileHandler),
            Box::new(crate::tools::edit_file::EditFileHandler::new()),
            Box::new(crate::tools::create_file::CreateFileHandler::new()),
            Box::new(crate::tools::delete_file::DeleteFileHandler::new()),
            Box::new(crate::tools::remove_dir::RemoveDirHandler::new()),
            Box::new(crate::tools::grep::GrepHandler),
            Box::new(crate::tools::list_dir::ListDirHandler),
            Box::new(crate::tools::multi_select::MultiSelectHandler),
            Box::new(crate::tools::git::GitStatusHandler),
            Box::new(crate::tools::git::GitDiffHandler),
            Box::new(crate::tools::git::GitStageHandler),
            Box::new(crate::tools::git::GitCommitHandler),
            Box::new(crate::tools::git::GitLogHandler),
        ];

        // Add custom tools from config
        for (name, tool_config) in custom_tools {
            handlers.push(Box::new(
                crate::tools::custom_tools::CustomToolHandler::new(
                    tool_config.name.clone(),
                    tool_config.command.clone(),
                    tool_config.description.clone(),
                    tool_config.timeout,
                )
            ));
            tracing::debug!("Registered custom tool: {}", name);
        }

        Self { handlers }
    }

    /// Get tool definitions for the specified available tools
    pub fn get_tools(&self, available_tools: &[String]) -> Vec<ChatCompletionTool> {
        available_tools
            .iter()
            .filter_map(|tool_name| {
                self.handlers
                    .iter()
                    .find(|h| h.name() == tool_name)
                    .map(|h| h.get_definition())
            })
            .collect()
    }

    /// Execute a tool by name with the given arguments
    pub fn execute_tool(&self, tool_name: &str, arguments: &Value, allowed_base: &str) -> String {
        self.handlers
            .iter()
            .find(|h| h.name() == tool_name)
            .map(|handler| {
                tracing::info!("Executing tool: {}", tool_name);
                let response = handler.execute(arguments, allowed_base);
                let formatted = response.format();
                tracing::debug!("Tool {} response: {}", tool_name, formatted);
                formatted
            })
            .unwrap_or_else(|| {
                tracing::warn!("Unknown tool: {}", tool_name);
                crate::types::ToolResponse::error("unknown", format!("Unknown tool: {}", tool_name)).format()
            })
    }

    /// Reset all handlers' internal state before processing a new batch
    pub fn reset_batch(&self) {
        for handler in &self.handlers {
            handler.reset_batch();
        }
    }

}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert!(registry.get_tools(&["read_file".to_string()]).len() == 1);
    }

    #[test]
    fn test_unknown_tool() {
        let registry = ToolRegistry::new();
        let result = registry.execute_tool("unknown_tool", &serde_json::json!({}), ".");
        assert!(result.contains("Unknown tool"));
    }

    #[test]
    fn test_custom_tool_registry() {
        let mut custom_tools = HashMap::new();
        custom_tools.insert(
            "cargo_test".to_string(),
            crate::config::CustomTool {
                name: "cargo_test".to_string(),
                command: "echo 'test'".to_string(),
                description: Some("Test command".to_string()),
                timeout: 30,
            },
        );

        let registry = ToolRegistry::with_custom_tools(&custom_tools);
        
        // Check that custom tool is registered
        let tools = registry.get_tools(&["cargo_test".to_string()]);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "cargo_test");
        
        // Check that built-in tools still work
        let tools = registry.get_tools(&["read_file".to_string()]);
        assert_eq!(tools.len(), 1);
    }
}
