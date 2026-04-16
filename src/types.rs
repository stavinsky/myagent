use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub use crate::valid_path::ValidPath;

/// Trait for tool handlers - each tool implements this to handle its own execution
pub trait ToolHandler: Send + Sync {
    /// Execute the tool with the given arguments and allowed base
    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse;
    
    /// Get the tool name
    fn name(&self) -> &str;
    
    /// Get the tool definition for OpenAI
    fn get_definition(&self) -> async_openai::types::chat::ChatCompletionTool;
    
    /// Reset the tool's internal state for a new batch of tool calls
    /// This is called before processing a new batch of tool calls from the LLM
    fn reset_batch(&self) {}
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Flow {
    pub description: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub tools: Vec<String>,
    #[serde(default)]
    pub arguments: Vec<FlowArgument>,
    #[serde(default)]
    pub common_system_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FlowArgument {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// Tracks edit positions per file to prevent line-shift conflicts
/// When multiple edits are made to the same file in a single batch,
/// we track the minimum line position edited to detect conflicts.
#[derive(Debug, Default)]
pub struct EditTracker {
    /// Maps file_path -> minimum line position edited in this batch
    last_edits: HashMap<String, u32>,
}

impl EditTracker {
    /// Create a new empty EditTracker
    pub fn new() -> Self {
        Self {
            last_edits: HashMap::new(),
        }
    }

    /// Check if edit conflicts with previous edits and record it
    pub fn check_and_record_edit(&mut self, file_path: &str, start_line: u32, end_line: u32) -> Result<(), String> {
        if let Some(&min_position) = self.last_edits.get(file_path) {
            // If this edit starts below (higher line number than) the minimum edited position,
            // it will be affected by line shifts from previous edits
            if start_line > min_position {
                return Err(format!(
                    "Edit to {} at lines {}-{} may be affected by a previous edit at line {}. \
                     Please re-read the file to get updated content and line numbers before editing.",
                    file_path, start_line, end_line, min_position
                ));
            }
        }
        // Record this edit - store the minimum of the new start_line and existing position
        let new_min = if let Some(&existing) = self.last_edits.get(file_path) {
            start_line.min(existing)
        } else {
            start_line
        };
        self.last_edits.insert(file_path.to_string(), new_min);
        Ok(())
    }
}

/// Structured tool response with status and formatted output
#[derive(Debug, Clone)]
pub struct ToolResponse {
    pub tool_name: String,
    pub status: ToolStatus,
    pub result: String,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Success,
    Error,
}

impl ToolResponse {
    /// Create a successful tool response
    pub fn success(tool_name: &str, result: String) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            status: ToolStatus::Success,
            result,
            metadata: None,
        }
    }

    /// Create an error tool response
    pub fn error(tool_name: &str, message: String) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            status: ToolStatus::Error,
            result: message,
            metadata: None,
        }
    }

    /// Create a tool response with metadata
    pub fn with_metadata(mut self, metadata: String) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Format the response as structured output with START/END markers
    pub fn format(&self) -> String {
        let status_str = match self.status {
            ToolStatus::Success => "success",
            ToolStatus::Error => "error",
        };

        let mut output = String::new();
        output.push_str(&format!("called: {}\n", self.tool_name));
        output.push_str(&format!("status: {}\n", status_str));

        if let Some(meta) = &self.metadata {
            output.push_str(&format!("metadata: {}\n", meta));
        }
        // IMPORTANT: we always put \n before the end marker. 
        // This ensures that even if the file is empty, 
        // the end marker is on a new line and can be reliably detected.
        // If the file has \n it should not be lost. 
        output.push_str("=== START ===\n");
        output.push_str(&self.result);
        if  !self.result.is_empty() { 
            output.push_str("\n");
        }
        output.push_str("=== END ===\n");

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_with_metadata() {
        let response = ToolResponse::success("read_file", "file content".to_string())
            .with_metadata("FILE: test.rs - 123 bytes".to_string());
        
        let formatted = response.format();
        
        assert!(formatted.contains("called: read_file"));
        assert!(formatted.contains("status: success"));
        assert!(formatted.contains("metadata: FILE: test.rs - 123 bytes"));
        assert!(formatted.contains("=== START ==="));
        assert!(formatted.contains("file content"));
        assert!(formatted.contains("=== END ==="));
    }

    #[test]
    fn test_format_without_metadata() {
        let response = ToolResponse::success("echo", "Hello".to_string());
        
        let formatted = response.format();
        
        assert!(formatted.contains("called: echo"));
        assert!(formatted.contains("status: success"));
        assert!(!formatted.contains("metadata:"));
        assert!(formatted.contains("=== START ==="));
        assert!(formatted.contains("Hello"));
        assert!(formatted.contains("=== END ==="));
    }

    #[test]
    fn test_format_error() {
        let response = ToolResponse::error("read_file", "File not found".to_string());
        
        let formatted = response.format();
        
        assert!(formatted.contains("called: read_file"));
        assert!(formatted.contains("status: error"));
        assert!(formatted.contains("=== START ==="));
        assert!(formatted.contains("File not found"));
        assert!(formatted.contains("=== END ==="));
    }

    #[test]
    fn test_format_multiline() {
        let response = ToolResponse::success("grep", "line1\nline2\nline3".to_string())
            .with_metadata("3 matches".to_string());
        
        let formatted = response.format();
        
        assert!(formatted.contains("called: grep"));
        assert!(formatted.contains("status: success"));
        assert!(formatted.contains("metadata: 3 matches"));
        assert!(formatted.contains("=== START ==="));
        assert!(formatted.contains("line1"));
        assert!(formatted.contains("line2"));
        assert!(formatted.contains("line3"));
        assert!(formatted.contains("=== END ==="));
    }

    #[test]
    fn test_edit_tracker_new_edit() {
        let mut tracker = EditTracker::new();
        let result = tracker.check_and_record_edit("file.txt", 10, 15);
        assert!(result.is_ok());
    }

    #[test]
    fn test_edit_tracker_same_file_higher_line() {
        let mut tracker = EditTracker::new();
        // First edit at lines 10-15
        let result1 = tracker.check_and_record_edit("file.txt", 10, 15);
        assert!(result1.is_ok());
        
        // Second edit at lines 20-25 (higher line number) should fail
        let result2 = tracker.check_and_record_edit("file.txt", 20, 25);
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("may be affected by a previous edit"));
    }

    #[test]
    fn test_edit_tracker_same_file_lower_line() {
        let mut tracker = EditTracker::new();
        // First edit at lines 20-25
        let result1 = tracker.check_and_record_edit("file.txt", 20, 25);
        assert!(result1.is_ok());
        
        // Second edit at lines 10-15 (lower line number) should succeed
        let result2 = tracker.check_and_record_edit("file.txt", 10, 15);
        assert!(result2.is_ok());
        
        // Third edit at lines 30-35 should fail (higher than the minimum of 10)
        let result3 = tracker.check_and_record_edit("file.txt", 30, 35);
        assert!(result3.is_err());
    }

    #[test]
    fn test_edit_tracker_different_files() {
        let mut tracker = EditTracker::new();
        // Edit file1 at lines 10-15
        let result1 = tracker.check_and_record_edit("file1.txt", 10, 15);
        assert!(result1.is_ok());
        
        // Edit file2 at lines 20-25 should succeed (different file)
        let result2 = tracker.check_and_record_edit("file2.txt", 20, 25);
        assert!(result2.is_ok());
    }

    #[test]
    fn test_edit_tracker_exact_boundary() {
        let mut tracker = EditTracker::new();
        // Edit at lines 10-15
        let result1 = tracker.check_and_record_edit("file.txt", 10, 15);
        assert!(result1.is_ok());
        
        // Edit starting exactly at line 10 (same as minimum) should succeed
        let result2 = tracker.check_and_record_edit("file.txt", 10, 12);
        assert!(result2.is_ok());
    }
}
