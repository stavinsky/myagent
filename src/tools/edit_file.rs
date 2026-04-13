use std::fs;
use std::io::Write;
use std::sync::RwLock;
use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use tracing;
use crate::types::{ToolResponse, ValidPath, ToolHandler};
use serde_json::Value;

/// Get the tool definition for edit_file
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: "edit_file".to_string(),
            description: Some("Edit a file by replacing a line range with new text. Lines are 1-indexed. Ranges are inclusive.".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to edit"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Starting line number to replace (1-indexed, inclusive). Example: To replace line 10 only, set to 10."
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Ending line number to replace (1-indexed, inclusive). Example: To replace line 10 only, set to 10. To replace lines 10-15, set to 15."
                    },
                    "new_text": {
                        "type": "string",
                        "description": "The replacement text. To remove lines, set to empty string or the desired replacement."
                    }
                },
                "required": ["file_path", "start_line", "end_line", "new_text"]
            })),
        }
    }
}

/// Edit a file by replacing a line range with new text.
/// Lines are 1-indexed and ranges are inclusive.
/// The file_path parameter is already validated (ValidPath type ensures this).
pub fn edit_file(valid_path: &ValidPath, start_line: u32, end_line: u32, new_text: &str) -> ToolResponse {
    let file_path = valid_path.as_str();
    
    // Log requested offsets at info level
    tracing::info!("edit_file: {} (lines {}-{})", file_path, start_line, end_line);
    
    // Log all parameters at debug level
    tracing::debug!(
        "edit_file parameters: file_path={}, start_line={}, end_line={}, new_text_length={} bytes",
        file_path,
        start_line,
        end_line,
        new_text.len()
    );
    
    // Validate input parameters to prevent underflow and invalid ranges
    if start_line < 1 {
        return ToolResponse::error(
            "edit_file",
            format!("start_line must be >= 1, got {}", start_line)
        );
    }
    
    if end_line < 1 {
        return ToolResponse::error(
            "edit_file",
            format!("end_line must be >= 1, got {}", end_line)
        );
    }
    
    if start_line > end_line {
        return ToolResponse::error(
            "edit_file",
            format!("Start line ({}) is greater than end line ({})", start_line, end_line)
        );
    }
    
    let content = match fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(e) => {
            tracing::warn!("edit_file failed: {}", e);
            return ToolResponse::error("edit_file", format!("Error reading file '{}': {}", file_path, e));
        }
    };
    
    // Check if original content ends with a newline to preserve it
    let ends_with_newline = content.ends_with('\n');
    
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    
    // Validate line range
    // Allow editing empty files: if file is empty (total_lines=0), allow start_line=1 to insert initial content
    if total_lines > 0 && start_line as usize > total_lines {
        return ToolResponse::error(
            "edit_file",
            format!("Start line ({}) is beyond the end of the file ({} lines)", start_line, total_lines)
        );
    }
    
    if total_lines > 0 && end_line as usize > total_lines {
        return ToolResponse::error(
            "edit_file",
            format!("End line ({}) is beyond the end of the file ({} lines)", end_line, total_lines)
        );
    }
    
    // For empty files, allow end_line to be 0 (insert at start) or 1 (replace line 1 which doesn't exist yet)
    let actual_end_line = if total_lines == 0 && end_line == 0 {
        1 // Treat end_line=0 on empty file as end_line=1 for insertion
    } else {
        end_line
    };
    
    let old_lines = if total_lines > 0 {
        &lines[(start_line as usize - 1)..actual_end_line as usize]
    } else {
        &[]
    };
    let old_text_preview = old_lines.iter().take(5).cloned().collect::<Vec<_>>().join("\n");
    let old_line_count = old_lines.len();
    
    // Normalize replacement text: strip trailing newlines to avoid confusion
    // The trailing newline status will be determined by whether new_text originally ended with \n
    let replacement_ends_with_newline = new_text.ends_with('\n');
    let normalized_text = new_text.trim_end_matches('\n');
    
    let mut new_lines: Vec<String> = Vec::new();
    
    for i in 0..(start_line as usize - 1) {
        new_lines.push(lines[i].to_string());
    }
    
    for line in normalized_text.lines() {
        new_lines.push(line.to_string());
    }
    
    // Only add remaining lines if file had content and we're not at the end
    if total_lines > 0 && (actual_end_line as usize) < total_lines {
        for i in actual_end_line as usize..total_lines {
            new_lines.push(lines[i].to_string());
        }
    }
    
    let mut new_content = new_lines.join("\n");
    
    // Preserve trailing newline: prefer the replacement's trailing newline status if provided,
    // otherwise fall back to the original file's status
    let should_have_trailing_newline = if normalized_text.is_empty() {
        // If replacement is empty, preserve original file's newline status
        ends_with_newline
    } else {
        // Use the replacement's trailing newline status
        replacement_ends_with_newline
    };
    
    if should_have_trailing_newline {
        new_content.push('\n');
    }
    
    match fs::File::create(&file_path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(new_content.as_bytes()) {
                tracing::warn!("edit_file failed: {}", e);
                return ToolResponse::error("edit_file", format!("Error writing to file '{}': {}", file_path, e));
            }
            // Sync to ensure data is written to disk
            if let Err(e) = file.sync_all() {
                tracing::warn!("edit_file failed: {}", e);
                return ToolResponse::error("edit_file", format!("Error syncing file '{}': {}", file_path, e));
            }
        }
        Err(e) => {
            tracing::warn!("edit_file failed: {}", e);
            return ToolResponse::error("edit_file", format!("Error creating file '{}': {}", file_path, e));
        }
    }
    
    let new_line_count = new_text.lines().count();
    
    tracing::debug!("edit_file completed");
    
    let metadata = format!("Edited {} - lines {}-{} ({} lines → {} lines)", file_path, start_line, end_line, old_line_count, new_line_count);
    ToolResponse::success("edit_file", format!(
        "Successfully edited '{}'. Replaced lines {}-{} ({} line(s)) with {} line(s). \n\nPrevious content preview (first 5 lines):\n{}\n\nFile now contains {} characters.",
        file_path, start_line, end_line, old_line_count, new_line_count,
        old_text_preview,
        new_content.len()
    )).with_metadata(metadata)
}

/// Tool handler implementation for edit_file
/// This handler maintains internal state (EditTracker) for tracking edit conflicts
pub struct EditFileHandler {
    edit_tracker: RwLock<crate::types::EditTracker>,
}

impl EditFileHandler {
    /// Create a new EditFileHandler with fresh state
    pub fn new() -> Self {
        Self {
            edit_tracker: RwLock::new(crate::types::EditTracker::new()),
        }
    }

    /// Get a write lock on the edit tracker, handling poisoned locks gracefully
    fn get_tracker_write(&self) -> Result<std::sync::RwLockWriteGuard<'_, crate::types::EditTracker>, String> {
        match self.edit_tracker.write() {
            Ok(t) => Ok(t),
            Err(e) => {
                tracing::warn!("edit_file lock poisoned: {:?}", e);
                // Try to recover by getting a new write lock
                match self.edit_tracker.write() {
                    Ok(t) => Ok(t),
                    Err(_) => Err("Internal error: unable to acquire edit tracker lock".to_string()),
                }
            }
        }
    }
}

impl Default for EditFileHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for EditFileHandler {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn get_definition(&self) -> async_openai::types::ChatCompletionTool {
        get_tool_definition()
    }

    /// Reset the internal edit tracker for a new batch
    fn reset_batch(&self) {
        if let Ok(mut tracker) = self.get_tracker_write() {
            *tracker = crate::types::EditTracker::new();
        } else {
            tracing::warn!("Failed to reset edit tracker - lock unavailable");
        }
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let file_path = match arguments["file_path"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("edit_file", "Missing required argument: file_path".to_string()),
        };
        let valid_path = match ValidPath::from_string(file_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("edit_file", e),
        };
        let start_line = match arguments["start_line"].as_u64() {
            Some(l) if l > 0 => l as u32,
            Some(l) => return ToolResponse::error(
                "edit_file",
                format!("start_line must be positive, got {}", l)
            ),
            None => return ToolResponse::error("edit_file", "Missing required argument: start_line".to_string()),
        };
        let end_line = match arguments["end_line"].as_u64() {
            Some(l) if l > 0 => l as u32,
            Some(l) => return ToolResponse::error(
                "edit_file",
                format!("end_line must be positive, got {}", l)
            ),
            None => return ToolResponse::error("edit_file", "Missing required argument: end_line".to_string()),
        };
        let new_text = match arguments["new_text"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("edit_file", "Missing required argument: new_text".to_string()),
        };
        
        // Check for conflicts with previous edits using internal tracker
        let mut tracker = match self.get_tracker_write() {
            Ok(t) => t,
            Err(e) => return ToolResponse::error("edit_file", e),
        };
        
        match tracker.check_and_record_edit(file_path, start_line, end_line) {
            Ok(_) => {
                // Proceed with the actual edit
                drop(tracker); // Release the lock before calling edit_file
                edit_file(&valid_path, start_line, end_line, new_text)
            }
            Err(e) => {
                // Return error without executing the edit
                ToolResponse::error("edit_file", e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolStatus;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_file(content: &str) -> (TempDir, ValidPath) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, content).expect("Failed to write test file");
        let valid_path = ValidPath::from_string(
            file_path.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).expect("Failed to create ValidPath");
        (temp_dir, valid_path)
    }

    #[test]
    fn test_preserve_trailing_newline() {
        // Test that replacement without trailing newline removes it from result
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "replaced_line");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        // Replacement has no trailing newline, so result shouldn't have it either
        assert_eq!(content, "line1\nreplaced_line\nline3");
        assert!(!content.ends_with('\n'), "File should not have trailing newline when replacement doesn't");
    }

    #[test]
    fn test_preserve_no_trailing_newline() {
        // Test that files without trailing newline stay without it
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "replaced_line");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "line1\nreplaced_line\nline3");
        assert!(!content.ends_with('\n'), "File should not have trailing newline if original didn't");
    }

    #[test]
    fn test_add_trailing_newline_via_replacement() {
        // Test that replacement text with trailing newline adds it to the result
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3");
        let file_path = valid_path.as_str();
        
        // Replace the last line with text that ends in newline
        let result = edit_file(&valid_path, 3, 3, "replaced_line\n");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        // Replacement has trailing newline, so result should have it
        assert_eq!(content, "line1\nline2\nreplaced_line\n");
        assert!(content.ends_with('\n'), "Should have trailing newline from replacement");
    }

    #[test]
    fn test_empty_replacement_preserves_newline_status() {
        // Test that empty replacement preserves the original file's newline status
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "line1\nline3\n");
        assert!(content.ends_with('\n'), "Should preserve trailing newline");
    }

    #[test]
    fn test_multiline_replacement_with_newline() {
        // Test multiline replacement with trailing newline
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "new_line1\nnew_line2\n");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "line1\nnew_line1\nnew_line2\nline3\n");
        assert!(content.ends_with('\n'), "Should have trailing newline from replacement");
    }

    #[test]
    fn test_replace_entire_file() {
        // Test replacing the entire file content
        let (_temp_dir, valid_path) = setup_test_file("old1\nold2\nold3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 1, 3, "new1\nnew2\nnew3");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        // Replacement has no trailing newline, so result shouldn't either
        assert_eq!(content, "new1\nnew2\nnew3");
        assert!(!content.ends_with('\n'), "Should not have trailing newline when replacement doesn't");
    }

    #[test]
    fn test_single_line_file_with_newline() {
        // Test single line file - replacement without trailing newline removes it
        let (_temp_dir, valid_path) = setup_test_file("single_line\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 1, 1, "replaced");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "replaced");
        assert!(!content.ends_with('\n'), "Should not have trailing newline when replacement doesn't");
    }

    #[test]
    fn test_single_line_file_without_newline() {
        // Test single line file without trailing newline
        let (_temp_dir, valid_path) = setup_test_file("single_line");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 1, 1, "replaced");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "replaced");
        assert!(!content.ends_with('\n'), "Should not add trailing newline");
    }
    #[test]
    fn test_replacement_with_trailing_newline() {
        // Test that replacement text with trailing newline adds it to the result
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "replaced_line\n");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        // Replacement has trailing newline, so result should have it too
        assert_eq!(content, "line1\nreplaced_line\nline3\n");
        assert!(content.ends_with('\n'), "Should have trailing newline from replacement");
    }

    #[test]
    fn test_replacement_without_trailing_newline() {
        // Test that replacement text without trailing newline removes it
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "replaced_line");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        // Original had trailing newline, but replacement doesn't, so result shouldn't
        assert_eq!(content, "line1\nreplaced_line\nline3");
        assert!(!content.ends_with('\n'), "Should not have trailing newline when replacement doesn't");
    }

    #[test]
    fn test_multiline_replacement_with_trailing_newline() {
        // Test multiline replacement with trailing newline
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "new_line1\nnew_line2\n");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "line1\nnew_line1\nnew_line2\nline3\n");
        assert!(content.ends_with('\n'), "Should preserve trailing newline from replacement");
    }

    #[test]
    fn test_empty_replacement_uses_original_newline_status() {
        // Test that empty replacement preserves the original file's newline status
        let (_temp_dir, valid_path) = setup_test_file("line1\nline2\nline3\n");
        let file_path = valid_path.as_str();
        
        let result = edit_file(&valid_path, 2, 2, "");
        assert!(matches!(result.status, ToolStatus::Success), "Edit should succeed: {:?}", result);
        
        let content = fs::read_to_string(file_path).expect("Failed to read file");
        assert_eq!(content, "line1\nline3\n");
        assert!(content.ends_with('\n'), "Should preserve original trailing newline for empty replacement");
    }}
