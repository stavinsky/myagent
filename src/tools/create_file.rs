//! Create file tool
//!
//! Creates a new file with optional content at the specified path.

use crate::types::{ToolHandler, ToolResponse, ValidPath};
use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tracing;

/// Get the tool definition for create_file
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        function: FunctionObject {
            name: "create_file".to_string(),
            description: Some("Create a new file at the specified path with optional content. Fails if the file already exists.".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path where the new file should be created"
                    },
                    "content": {
                        "type": "string",
                        "description": "Initial content for the file. If omitted, creates an empty file."
                    }
                },
                "required": ["file_path"]
            })),
            strict: Some(true),
        },
    }
}

/// Create a new file at the specified path
pub fn create_file(valid_path: &ValidPath, content: &str) -> ToolResponse {
    let file_path = valid_path.as_str();

    tracing::info!("create_file: {}", file_path);
    tracing::debug!(
        "create_file parameters: file_path={}, content_length={}",
        file_path,
        content.len()
    );

    // Check if file already exists
    if Path::new(file_path).exists() {
        tracing::warn!("create_file failed: file already exists");
        return ToolResponse::error(
            "create_file",
            format!("File '{}' already exists", file_path),
        );
    }

    // Create parent directories if they don't exist
    if let Some(parent) = Path::new(file_path).parent() {
        if !parent.as_os_str().is_empty() {
            match fs::create_dir_all(parent) {
                Ok(_) => {
                    tracing::debug!("Created parent directories for: {}", file_path);
                }
                Err(e) => {
                    tracing::warn!(
                        "create_file failed: could not create parent directories: {}",
                        e
                    );
                    return ToolResponse::error(
                        "create_file",
                        format!(
                            "Could not create parent directories for '{}': {}",
                            file_path, e
                        ),
                    );
                }
            }
        }
    }

    // Create the file
    match fs::write(file_path, content) {
        Ok(_) => {
            tracing::debug!("create_file completed: {} bytes", content.len());
            let metadata = format!("[FILE: {} - {} bytes created]", file_path, content.len());
            ToolResponse::success(
                "create_file",
                format!(
                    "Successfully created file '{}' with {} bytes",
                    file_path,
                    content.len()
                ),
            )
            .with_metadata(metadata)
        }
        Err(e) => {
            tracing::warn!("create_file failed: {}", e);
            ToolResponse::error(
                "create_file",
                format!("Failed to create file '{}': {}", file_path, e),
            )
        }
    }
}

/// Create file handler implementing ToolHandler trait
pub struct CreateFileHandler;

impl CreateFileHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CreateFileHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for CreateFileHandler {
    fn name(&self) -> &str {
        "create_file"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let file_path = match arguments["file_path"].as_str() {
            Some(s) => s,
            None => {
                return ToolResponse::error(
                    "create_file",
                    "Missing required argument: file_path".to_string(),
                )
            }
        };

        let valid_path = match ValidPath::from_string(file_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("create_file", e),
        };

        let content = arguments["content"].as_str().unwrap_or("");

        create_file(&valid_path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolStatus;
    use tempfile::TempDir;

    #[test]
    fn test_create_file_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("new_file.txt");

        let valid_path = ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap(),
        )
        .unwrap();

        let result = create_file(&valid_path, "Hello world");
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("Successfully created"));

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "Hello world");
    }

    #[test]
    fn test_create_file_empty() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("empty_file.txt");

        let valid_path = ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap(),
        )
        .unwrap();

        let result = create_file(&valid_path, "");
        assert!(matches!(result.status, ToolStatus::Success));

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_create_file_already_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("existing.txt");
        fs::write(&test_file, "existing content").unwrap();

        let valid_path = ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap(),
        )
        .unwrap();

        let result = create_file(&valid_path, "new content");
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("already exists"));
    }

    #[test]
    fn test_create_file_creates_parent_dirs() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nested_file = temp_dir
            .path()
            .join("subdir")
            .join("nested")
            .join("file.txt");

        let valid_path = ValidPath::from_string(
            nested_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap(),
        )
        .unwrap();

        let result = create_file(&valid_path, "nested content");
        assert!(matches!(result.status, ToolStatus::Success));

        let content = fs::read_to_string(&nested_file).unwrap();
        assert_eq!(content, "nested content");
    }
}
