//! Tool registry module
//! 
//! This module provides a centralized registry for managing tool handlers.
//! It handles tool registration, filtering, and execution.

use crate::types::ToolHandler;
use serde_json::Value;
use async_openai::types::ChatCompletionTool;

/// Centralized registry for all tool handlers
pub struct ToolRegistry {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new ToolRegistry with all available tools
    pub fn new() -> Self {
        let handlers: Vec<Box<dyn ToolHandler>> = vec![
            Box::new(crate::tools::read_file::ReadFileHandler),
            Box::new(crate::tools::edit_file::EditFileHandler::new()),
            Box::new(crate::tools::grep::GrepHandler),
            Box::new(crate::tools::list_dir::ListDirHandler),
            Box::new(crate::tools::multi_select::MultiSelectHandler),
            Box::new(crate::tools::git::GitStatusHandler),
            Box::new(crate::tools::git::GitDiffHandler),
            Box::new(crate::tools::git::GitStageHandler),
            Box::new(crate::tools::git::GitCommitHandler),
            Box::new(crate::tools::git::GitLogHandler),
        ];
        
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
}
