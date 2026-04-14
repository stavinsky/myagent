//! Delete file tool
//! 
//! Deletes a file at the specified path.

use std::fs;
use std::path::Path;
use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use tracing;
use crate::types::{ToolResponse, ValidPath, ToolHandler};
use serde_json::Value;

/// Get the tool definition for delete_file
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        function: FunctionObject {
            name: "delete_file".to_string(),
            description: Some("Delete a file at the specified path. Fails if the file doesn't exist or is a directory (use remove_dir for directories).".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to delete"
                    }
                },
                "required": ["file_path"]
            })),
            strict: Some(true),
        },
    }
}

/// Delete a file at the specified path
pub fn delete_file(valid_path: &ValidPath) -> ToolResponse {
    let file_path = valid_path.as_str();
    
    tracing::info!("delete_file: {}", file_path);
    tracing::debug!("delete_file parameters: file_path={}", file_path);
    
    let path = Path::new(file_path);
    
    // Check if file exists
    if !path.exists() {
        tracing::warn!("delete_file failed: file does not exist");
        return ToolResponse::error(
            "delete_file",
            format!("File '{}' does not exist", file_path)
        );
    }
    
    // Check if it's a directory
    if path.is_dir() {
        tracing::warn!("delete_file failed: path is a directory");
        return ToolResponse::error(
            "delete_file",
            format!("'{}' is a directory, not a file. Use remove_dir to delete directories.", file_path)
        );
    }
    
    // Get file size before deletion for metadata
    let metadata = match fs::metadata(path) {
        Ok(m) => format!("[FILE: {} - {} bytes deleted]", file_path, m.len()),
        Err(_) => format!("[FILE: {} deleted]", file_path),
    };
    
    // Delete the file
    match fs::remove_file(path) {
        Ok(_) => {
            tracing::debug!("delete_file completed");
            ToolResponse::success(
                "delete_file",
                format!("Successfully deleted file '{}'", file_path)
            ).with_metadata(metadata)
        }
        Err(e) => {
            tracing::warn!("delete_file failed: {}", e);
            ToolResponse::error("delete_file", format!("Failed to delete file '{}': {}", file_path, e))
        }
    }
}

/// Delete file handler implementing ToolHandler trait
pub struct DeleteFileHandler;

impl DeleteFileHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DeleteFileHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for DeleteFileHandler {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let file_path = match arguments["file_path"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("delete_file", "Missing required argument: file_path".to_string()),
        };
        
        let valid_path = match ValidPath::from_string(file_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("delete_file", e),
        };
        
        delete_file(&valid_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::types::ToolStatus;

    #[test]
    fn test_delete_file_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("to_delete.txt");
        fs::write(&test_file, "content to delete").unwrap();
        
        let valid_path = ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        
        let result = delete_file(&valid_path);
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("Successfully deleted"));
        assert!(!test_file.exists());
    }

    #[test]
    fn test_delete_file_not_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nonexistent = temp_dir.path().join("nonexistent.txt");
        
        let valid_path = ValidPath::from_string(
            nonexistent.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        
        let result = delete_file(&valid_path);
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("does not exist"));
    }

    #[test]
    fn test_delete_file_is_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let dir = temp_dir.path().join("a_directory");
        fs::create_dir(&dir).unwrap();
        
        let valid_path = ValidPath::from_string(
            dir.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        
        let result = delete_file(&valid_path);
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("directory"));
    }
}