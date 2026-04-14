//! Remove directory tool
//! 
//! Removes a directory at the specified path.

use std::fs;
use std::path::Path;
use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use tracing;
use crate::types::{ToolResponse, ValidPath, ToolHandler};
use serde_json::Value;

/// Get the tool definition for remove_dir
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        function: FunctionObject {
            name: "remove_dir".to_string(),
            description: Some("Remove a directory at the specified path. Fails if the directory doesn't exist or is a file (use delete_file for files).".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "dir_path": {
                        "type": "string",
                        "description": "Path to the directory to remove"
                    }
                },
                "required": ["dir_path"]
            })),
            strict: Some(true),
        },
    }
}

/// Remove a directory at the specified path
pub fn remove_dir(valid_path: &ValidPath) -> ToolResponse {
    let dir_path = valid_path.as_str();
    
    tracing::info!("remove_dir: {}", dir_path);
    tracing::debug!("remove_dir parameters: dir_path={}", dir_path);
    
    let path = Path::new(dir_path);
    
    // Check if directory exists
    if !path.exists() {
        tracing::warn!("remove_dir failed: directory does not exist");
        return ToolResponse::error(
            "remove_dir",
            format!("Directory '{}' does not exist", dir_path)
        );
    }
    
    // Check if it's a directory
    if !path.is_dir() {
        tracing::warn!("remove_dir failed: path is a file");
        return ToolResponse::error(
            "remove_dir",
            format!("'{}' is a file, not a directory. Use delete_file to delete files.", dir_path)
        );
    }
    
    // Remove the directory
    match fs::remove_dir(path) {
        Ok(_) => {
            tracing::debug!("remove_dir completed");
            ToolResponse::success(
                "remove_dir",
                format!("Successfully removed directory '{}'", dir_path)
            ).with_metadata(format!("[DIR: {} removed]", dir_path))
        }
        Err(e) => {
            tracing::warn!("remove_dir failed: {}", e);
            ToolResponse::error("remove_dir", format!("Failed to remove directory '{}': {}", dir_path, e))
        }
    }
}

/// Remove directory handler implementing ToolHandler trait
pub struct RemoveDirHandler;

impl RemoveDirHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RemoveDirHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for RemoveDirHandler {
    fn name(&self) -> &str {
        "remove_dir"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let dir_path = match arguments["dir_path"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("remove_dir", "Missing required argument: dir_path".to_string()),
        };
        
        let valid_path = match ValidPath::from_string(dir_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("remove_dir", e),
        };
        
        remove_dir(&valid_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::types::ToolStatus;

    #[test]
    fn test_remove_dir_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let dir_to_remove = temp_dir.path().join("dir_to_delete");
        fs::create_dir(&dir_to_remove).unwrap();
        
        let valid_path = ValidPath::from_string(
            dir_to_remove.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        
        let result = remove_dir(&valid_path);
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("Successfully removed"));
        assert!(!dir_to_remove.exists());
    }

    #[test]
    fn test_remove_dir_not_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nonexistent = temp_dir.path().join("nonexistent_dir");
        
        let valid_path = ValidPath::from_string(
            nonexistent.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        
        let result = remove_dir(&valid_path);
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("does not exist"));
    }

    #[test]
    fn test_remove_dir_is_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file = temp_dir.path().join("a_file.txt");
        fs::write(&file, "content").unwrap();
        
        let valid_path = ValidPath::from_string(
            file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        
        let result = remove_dir(&valid_path);
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("file"));
    }
}