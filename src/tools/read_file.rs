use std::fs;
use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use tracing;
use crate::types::{ToolResponse, ValidPath, ToolHandler};
use serde_json::Value;

/// Maximum number of lines to return from a file read. Excess lines are truncated.
const MAX_LINES: usize = 2000;

/// Get the tool definition for read_file
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        function: FunctionObject {
            name: "read_file".to_string(),
            description: Some(format!("Read the contents of a file by path with optional line range. Lines are numbered with the following format [<line-number>]<content>. Maximum {} lines are returned; larger requests are truncated.", MAX_LINES)),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Starting line number (1-indexed). If omitted, starts from line 1"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": format!("Ending line number (1-indexed, inclusive). If omitted, reads to the end of the file. Note: Maximum {} lines will be returned.", MAX_LINES)
                    }
                },
                "required": ["file_path"]
            })),
            strict: None,
        },
    }
}

/// Read file contents with optional line range (1-indexed)
pub fn read_file(valid_path: &ValidPath, _allowed_base: &str, start_line: Option<u32>, end_line: Option<u32>) -> ToolResponse {
    let file_path = valid_path.as_str();
    
    // Log requested offsets at info level
    tracing::info!("read_file: {} (range: {:?}-{:?})", file_path, start_line, end_line);
    
    // Log all parameters at debug level
    tracing::debug!(
        "read_file parameters: file_path={}, start_line={:?}, end_line={:?}",
        file_path,
        start_line,
        end_line
    );
    
    // Validate input parameters to prevent underflow and invalid ranges
    if let Some(start) = start_line {
        if start < 1 {
            return ToolResponse::error(
                "read_file",
                format!("start_line must be >= 1, got {}", start)
            );
        }
    }
    
    if let Some(end) = end_line {
        if end < 1 {
            return ToolResponse::error(
                "read_file",
                format!("end_line must be >= 1, got {}", end)
            );
        }
    }
    
    let response = match fs::read_to_string(file_path) {
        Ok(content) => {
            tracing::debug!("read_file completed ({} bytes)", content.len());
            
            if content.is_empty() {
                ToolResponse::success("read_file", String::new())
                    .with_metadata(format!("[FILE: {} - 0 bytes] (empty file)", file_path))
            } else {
                // Extract the requested line range
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();
                
                // Calculate actual range (safe: validated start_line >= 1 above)
                let start = start_line.map(|l| l as usize).unwrap_or(1);
                let end = end_line.map(|l| l as usize).unwrap_or(total_lines);
                
                // Validate range - combined check for clearer error messages
                if start > total_lines || end > total_lines {
                    return ToolResponse::error(
                        "read_file",
                        format!(
                            "Line range {}-{} is beyond the end of the file ({} lines)",
                            start, end, total_lines
                        )
                    );
                }
                
                if start > end {
                    return ToolResponse::error(
                        "read_file",
                        format!("Start line ({}) is greater than end line ({})", start, end)
                    );
                }
                
                // Extract the requested lines, applying MAX_LINES limit
                // Calculate the actual end (0-indexed), capped at MAX_LINES from start and file length
                let max_end = (start - 1 + MAX_LINES).min(end).min(total_lines);
                let range_lines = &lines[(start - 1)..max_end];
                let selected_content = range_lines.join("\n");
                let lines_returned = range_lines.len();
                
                // Truncated only if MAX_LINES limit was hit (not just file being short)
                let is_truncated = (max_end == start - 1 + MAX_LINES) && (max_end < end);
                
                // Add line numbers to the content with clear formatting
                let numbered_content = add_line_numbers_with_offset(range_lines, start);
                
                let metadata = if is_truncated {
                    format!("[FILE: {} - {} bytes] (lines {}-{} of {} total, MAX_LINES={}) - content truncated", 
                        file_path, 
                        selected_content.len(),
                        start,
                        start + lines_returned - 1,
                        total_lines,
                        MAX_LINES
                    )
                } else {
                    format!("[FILE: {} - {} bytes] (lines {}-{} of {} total)", 
                        file_path, 
                        selected_content.len(),
                        start,
                        end,
                        total_lines
                    )
                };
                ToolResponse::success("read_file", numbered_content).with_metadata(metadata)
            }
        },
        Err(e) => {
            tracing::warn!("read_file failed: {}", e);
            ToolResponse::error("read_file", format!("Failed to read file '{}': {}", file_path, e))
        }
    };
    
    response
}

/// Add line numbers to file content with a custom starting line number
fn add_line_numbers_with_offset(lines: &[&str], start_line: usize) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("[{}]{}", start_line + i, line))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Tool handler implementation for read_file
pub struct ReadFileHandler;

impl ToolHandler for ReadFileHandler {
    fn name(&self) -> &str {
        "read_file"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let file_path = match arguments["file_path"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("read_file", "Missing required argument: file_path".to_string()),
        };
        let valid_path = match ValidPath::from_string(file_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("read_file", e),
        };
        let start_line = match arguments["start_line"].as_u64() {
            Some(l) if l > 0 => Some(l as u32),
            Some(l) => return ToolResponse::error(
                "read_file",
                format!("start_line must be positive, got {}", l)
            ),
            None => None,
        };
        let end_line = match arguments["end_line"].as_u64() {
            Some(l) if l > 0 => Some(l as u32),
            Some(l) => return ToolResponse::error(
                "read_file",
                format!("end_line must be positive, got {}", l)
            ),
            None => None,
        };
        read_file(&valid_path, allowed_base, start_line, end_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolStatus;

    fn create_test_valid_path() -> ValidPath {
        ValidPath::from_string("Cargo.toml", ".").expect("Test fixture file Cargo.toml not found")
    }

    #[test]
    fn test_read_file_full() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", None, None);
        
        assert!(matches!(response.status, ToolStatus::Success));
        assert!(response.result.contains("myagent"));
        assert!(response.metadata.as_ref().unwrap().contains("total"));
    }

    #[test]
    fn test_read_file_with_range() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(1), Some(10));
        
        assert!(matches!(response.status, ToolStatus::Success));
        assert!(response.result.contains("[1]"));
        assert!(response.result.contains("[10]"));
        assert!(response.metadata.as_ref().unwrap().contains("lines 1-10"));
    }

    #[test]
    fn test_read_file_with_start_only() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(5), None);
        
        assert!(matches!(response.status, ToolStatus::Success));
        assert!(response.result.contains("[5]"));
        assert!(!response.result.contains("[1]"));
        assert!(!response.result.contains("[2]"));
        assert!(!response.result.contains("[3]"));
        assert!(!response.result.contains("[4]"));
    }

    #[test]
    fn test_read_file_with_end_only() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", None, Some(5));
        
        assert!(matches!(response.status, ToolStatus::Success));
        assert!(response.result.contains("[1]"));
        assert!(response.result.contains("[5]"));
        assert!(!response.result.contains("[6]"));
    }

    #[test]
    fn test_read_file_start_beyond_end() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(1000), Some(1010));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("beyond the end of the file"));
    }

    #[test]
    fn test_read_file_end_beyond_total() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(1), Some(10000));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("beyond the end of the file"));
    }

    #[test]
    fn test_read_file_start_greater_than_end() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(10), Some(5));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("Start line (10) is greater than end line (5)"));
    }

    #[test]
    fn test_read_file_single_line() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(3), Some(3));
        
        assert!(matches!(response.status, ToolStatus::Success));
        assert!(response.result.contains("[3]"));
        if let Some(metadata) = &response.metadata {
            assert!(metadata.contains("lines 3-3"));
        } else {
            panic!("Expected metadata to be present");
        }
    }

    #[test]
    fn test_read_file_line_numbers_correct() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(10), Some(15));
        
        assert!(matches!(response.status, ToolStatus::Success));
        // Verify line numbers are correctly labeled 10-15
        assert!(response.result.contains("[10]"));
        assert!(response.result.contains("[11]"));
        assert!(response.result.contains("[12]"));
        assert!(response.result.contains("[13]"));
        assert!(response.result.contains("[14]"));
        assert!(response.result.contains("[15]"));
    }

    #[test]
    fn test_read_file_start_line_zero() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(0), Some(10));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("start_line must be >= 1"));
    }

    #[test]
    fn test_read_file_end_line_zero() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(1), Some(0));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("end_line must be >= 1"));
    }

    #[test]
    fn test_read_file_both_lines_zero() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(0), Some(0));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("start_line must be >= 1"));
    }

    #[test]
    fn test_read_file_empty_range() {
        let valid_path = create_test_valid_path();
        // Request a range that results in no lines (start equals end+1 after validation)
        let response = read_file(&valid_path, ".", Some(5), Some(4));
        
        assert!(matches!(response.status, ToolStatus::Error));
        assert!(response.result.contains("greater than end line"));
    }

    #[test]
    fn test_read_file_metadata_present_on_success() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(1), Some(5));
        
        assert!(matches!(response.status, ToolStatus::Success));
        assert!(response.metadata.is_some());
        if let Some(metadata) = &response.metadata {
            assert!(metadata.contains("FILE:"));
            assert!(metadata.contains("bytes"));
            assert!(metadata.contains("lines"));
        }
    }

    #[test]
    fn test_read_file_metadata_absent_on_error() {
        let valid_path = create_test_valid_path();
        let response = read_file(&valid_path, ".", Some(0), Some(10));
        
        assert!(matches!(response.status, ToolStatus::Error));
        // Error responses may or may not have metadata, so we just verify no panic
    }
}

