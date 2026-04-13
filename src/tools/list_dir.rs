use std::fs;
use std::path::Path;
use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use tracing;
use crate::types::{ToolResponse, ValidPath, ToolHandler};
use serde_json::Value;

/// Get the tool definition for list_dir
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: "list_dir".to_string(),
            description: Some("List contents of a directory".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "dir_path": {
                        "type": "string",
                        "description": "Path to the directory to list"
                    }
                },
                "required": ["dir_path"]
            })),
        }
    }
}

/// List directory contents
pub fn list_dir(valid_dir_path: &ValidPath, _allowed_base: &str) -> ToolResponse {
    let dir_path = valid_dir_path.as_str();
    tracing::info!("list_dir: {}", dir_path);
    
    let path = Path::new(&dir_path);
    
    // Validate the path exists
    if !path.exists() {
        tracing::warn!("list_dir failed: Directory does not exist");
        return ToolResponse::error("list_dir", format!("Directory '{}' does not exist", dir_path));
    }
    
    if !path.is_dir() {
        tracing::warn!("list_dir failed: Path is not a directory");
        return ToolResponse::error("list_dir", format!("'{}' is not a directory", dir_path));
    }
    
    // Read directory entries
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("list_dir failed: {}", e);
            return ToolResponse::error("list_dir", format!("Error reading directory '{}': {}", dir_path, e));
        }
    };
    
    // Collect and sort entries
    let mut files: Vec<String> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    
    for entry in entries {
        if let Ok(entry) = entry {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_string();
            let entry_path = entry.path();
            
            if entry_path.is_dir() {
                dirs.push(format!("📁 {}/", name));
            } else {
                files.push(format!("📄 {}", name));
            }
        }
    }
    
    // Sort alphabetically
    files.sort();
    dirs.sort();
    
    // Combine with directories first
    let mut all_entries = dirs.clone();
    all_entries.extend(files.clone());
    
    let result = if all_entries.is_empty() {
        format!("[EMPTY DIRECTORY] Directory '{}' exists but contains no entries.", dir_path)
    } else {
        format!(
            "Directory listing for '{}':\n{}",
            dir_path,
            all_entries.join("\n")
        )
    };
    
    let metadata = if all_entries.is_empty() {
        "0 entries".to_string()
    } else {
        format!("{} entries ({} dirs, {} files)", all_entries.len(), dirs.len(), files.len())
    };
    
    tracing::debug!("list_dir completed: {} entries", all_entries.len());
    
    ToolResponse::success("list_dir", result).with_metadata(metadata)
}

/// Tool handler implementation for list_dir
pub struct ListDirHandler;

impl ToolHandler for ListDirHandler {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn get_definition(&self) -> async_openai::types::ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let dir_path = match arguments["dir_path"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("list_dir", "Missing required argument: dir_path".to_string()),
        };
        let valid_path = match ValidPath::from_string(dir_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("list_dir", e),
        };
        list_dir(&valid_path, allowed_base)
    }
}
