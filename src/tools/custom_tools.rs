//! Custom tool for executing shell commands defined in config
//! 
//! This module provides a flexible tool that allows executing predefined shell
//! commands with post-processing (head/tail/filter) on the output.

use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tracing;
use tokio::process::Command as TokioCommand;

use crate::types::{ToolHandler, ToolResponse};

/// Custom tool that executes a predefined shell command
pub struct CustomToolHandler {
    name: String,
    command: String,
    description: String,
    timeout: u64,
}

impl CustomToolHandler {
    pub fn new(name: String, command: String, description: Option<String>, timeout: u64) -> Self {
        let description = description.unwrap_or_else(|| format!("Execute custom command: {}", command));
        Self {
            name,
            command,
            description,
            timeout,
        }
    }

    /// Execute the custom command with timeout and optional post-processing
    pub fn execute_command(command: &str, tool_name: &str, args: &Value, allowed_base: &str, timeout_secs: u64) -> Result<ToolResponse, ToolResponse> {
        tracing::info!("custom_tool: name='{}', command='{}'", tool_name, command);
        
        // Parse and validate arguments
        let head_lines: Option<u32> = Self::validate_head_lines(args, tool_name)?;
        let tail_lines: Option<u32> = Self::validate_tail_lines(args, tool_name)?;
        let pattern: Option<String> = Self::validate_pattern(args, tool_name)?;

        tracing::debug!("custom_tool: name='{}', head_lines={:?}, tail_lines={:?}, pattern={:?}", 
                       tool_name, head_lines, tail_lines, pattern);

        // Execute the command with timeout using tokio for proper process termination
        let timeout_duration = Duration::from_secs(timeout_secs);
        let output = match Self::execute_with_timeout(command, allowed_base, timeout_duration) {
            Ok(output) => output,
            Err(e) => {
                tracing::warn!("custom_tool '{}' timed out or failed: {}", tool_name, e);
                return Err(ToolResponse::error(tool_name, e));
            }
        };

        // Collect output
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let error_msg = if !stderr.is_empty() {
                stderr
            } else {
                format!("Command exited with code: {:?}", output.status.code())
            };
            tracing::warn!("custom_tool '{}' failed: {}", tool_name, error_msg);
            return Err(ToolResponse::error(tool_name, error_msg));
        }

        // Apply post-processing
        let processed_output = Self::post_process_output(&stdout, head_lines, tail_lines, &pattern);

        tracing::info!("custom_tool '{}': success", tool_name);
        let metadata = format!("Command '{}' executed successfully ({} bytes)", command, processed_output.len());
        Ok(ToolResponse::success(tool_name, processed_output).with_metadata(metadata))
    }

    /// Execute a command with a timeout using tokio
    /// Properly terminates the process if it exceeds the timeout
    fn execute_with_timeout(command: &str, allowed_base: &str, timeout_duration: Duration) -> Result<std::process::Output, String> {
        // Validate that allowed_base is a valid path and canonicalize it
        let allowed_path = Path::new(allowed_base)
            .canonicalize()
            .map_err(|e| format!("Invalid allowed base path '{}': {}", allowed_base, e))?;
        
        // Try to use existing runtime if available
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // We're in a tokio context - use block_in_place with timeout
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        tokio::time::timeout(timeout_duration, Self::execute_async_with_timeout(command, &allowed_path, timeout_duration))
                            .await
                            .map_err(|_| format!("Command '{}' timed out after {:?}", command, timeout_duration))?
                    })
                })
            }
            Err(_) => {
                // No runtime available (e.g., in tests), create a new one
                let runtime = tokio::runtime::Runtime::new()
                    .map_err(|e| format!("Failed to create runtime: {}", e))?;
                runtime.block_on(async {
                    tokio::time::timeout(timeout_duration, Self::execute_async_with_timeout(command, &allowed_path, timeout_duration))
                        .await
                        .map_err(|_| format!("Command '{}' timed out after {:?}", command, timeout_duration))?
                })
            }
        }
    }

    /// Async version of execute_with_timeout that properly kills the process
    async fn execute_async_with_timeout(command: &str, allowed_base: &Path, timeout_duration: Duration) -> Result<std::process::Output, String> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        
        // Spawn the command
        let mut child = TokioCommand::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(allowed_base)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn command '{}': {}", command, e))?;

        // Create futures for reading stdout and stderr
        let stdout = child.stdout.take().expect("stdout should be piped");
        let stderr = child.stderr.take().expect("stderr should be piped");
        
        let stdout_handle = tokio::task::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut output = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                output.push(line);
            }
            output
        });
        
        let stderr_handle = tokio::task::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut output = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                output.push(line);
            }
            output
        });

        // Use tokio::select! to race between timeout and command completion
        tokio::select! {
            status_result = child.wait() => {
                match status_result {
                    Ok(status) => {
                        // Collect stdout and stderr
                        let stdout_lines = stdout_handle.await.unwrap_or_default();
                        let stderr_lines = stderr_handle.await.unwrap_or_default();
                        
                        let stdout = stdout_lines.join("\n").into_bytes();
                        let stderr = stderr_lines.join("\n").into_bytes();
                        
                        if status.success() {
                            Ok(std::process::Output {
                                status,
                                stdout,
                                stderr,
                            })
                        } else {
                            let error_msg = if !stderr.is_empty() {
                                String::from_utf8_lossy(&stderr).to_string()
                            } else {
                                format!("Command exited with code: {:?}", status.code())
                            };
                            Err(error_msg)
                        }
                    }
                    Err(e) => Err(format!("Failed to get status: {}", e)),
                }
            }
            _ = tokio::time::sleep(timeout_duration) => {
                // Timeout occurred - kill the process
                tracing::warn!("Command '{}' timed out after {:?}, killing process", command, timeout_duration);
                let _ = child.kill().await;
                Err(format!("Command '{}' timed out after {:?}", command, timeout_duration))
            }
        }
    }

    /// Validate head_lines argument
    fn validate_head_lines(args: &Value, tool_name: &str) -> Result<Option<u32>, ToolResponse> {
        match args.get("head_lines") {
            None => Ok(None),
            Some(value) => {
                // Check if it's a valid positive integer
                if let Some(num) = value.as_i64() {
                    if num <= 0 {
                        let msg = format!("head_lines must be positive, got {}", num);
                        tracing::warn!("custom_tool '{}' validation error: {}", tool_name, msg);
                        return Err(ToolResponse::error(tool_name, msg));
                    }
                    Ok(Some(num as u32))
                } else if let Some(num) = value.as_u64() {
                    Ok(Some(num as u32))
                } else {
                    let msg = format!("head_lines must be an integer, got {:?}", value);
                    tracing::warn!("custom_tool '{}' validation error: {}", tool_name, msg);
                    Err(ToolResponse::error(tool_name, msg))
                }
            }
        }
    }

    /// Validate tail_lines argument
    fn validate_tail_lines(args: &Value, tool_name: &str) -> Result<Option<u32>, ToolResponse> {
        match args.get("tail_lines") {
            None => Ok(None),
            Some(value) => {
                // Check if it's a valid positive integer
                if let Some(num) = value.as_i64() {
                    if num <= 0 {
                        let msg = format!("tail_lines must be positive, got {}", num);
                        tracing::warn!("custom_tool '{}' validation error: {}", tool_name, msg);
                        return Err(ToolResponse::error(tool_name, msg));
                    }
                    Ok(Some(num as u32))
                } else if let Some(num) = value.as_u64() {
                    Ok(Some(num as u32))
                } else {
                    let msg = format!("tail_lines must be an integer, got {:?}", value);
                    tracing::warn!("custom_tool '{}' validation error: {}", tool_name, msg);
                    Err(ToolResponse::error(tool_name, msg))
                }
            }
        }
    }

    /// Validate pattern argument - checks both type and regex validity
    fn validate_pattern(args: &Value, tool_name: &str) -> Result<Option<String>, ToolResponse> {
        match args.get("pattern") {
            None => Ok(None),
            Some(value) => {
                if let Some(pattern_str) = value.as_str() {
                    if pattern_str.is_empty() {
                        let msg = "pattern cannot be empty".to_string();
                        tracing::warn!("custom_tool '{}' validation error: {}", tool_name, msg);
                        return Err(ToolResponse::error(tool_name, msg));
                    }
                    // Validate that the pattern is a valid regex
                    match regex::Regex::new(pattern_str) {
                        Ok(_) => Ok(Some(pattern_str.to_string())),
                        Err(e) => {
                            let msg = format!("Invalid regex pattern '{}': {}", pattern_str, e);
                            tracing::error!("custom_tool '{}' validation error: {}", tool_name, msg);
                            Err(ToolResponse::error(tool_name, msg))
                        }
                    }
                } else {
                    let msg = format!("pattern must be a string, got {:?}", value);
                    tracing::warn!("custom_tool '{}' validation error: {}", tool_name, msg);
                    Err(ToolResponse::error(tool_name, msg))
                }
            }
        }
    }

    /// Post-process the output with head, tail, and grep-like filtering
    fn post_process_output(
        output: &str,
        head_lines: Option<u32>,
        tail_lines: Option<u32>,
        pattern: &Option<String>,
    ) -> String {
        let mut lines: Vec<&str> = output.lines().collect();

        // Apply head filter
        if let Some(n) = head_lines {
            let n = n as usize;
            if lines.len() > n {
                lines = lines[..n].to_vec();
            }
        }

        // Apply tail filter (after head, so we get last N of remaining)
        if let Some(n) = tail_lines {
            let n = n as usize;
            if lines.len() > n {
                let start = lines.len() - n;
                lines = lines[start..].to_vec();
            }
        }

        // Apply pattern filter (grep)
        // Pattern is already validated in validate_pattern(), so unwrap() is safe
        if let Some(pattern_str) = pattern {
            let regex = regex::Regex::new(&pattern_str)
                .expect("Pattern validation should have caught invalid regex");
            let before_count = lines.len();
            lines = lines.into_iter().filter(|line| regex.is_match(line)).collect();
            let after_count = lines.len();
            tracing::debug!("Pattern '{}' matched {} of {} lines", pattern_str, after_count, before_count);
        }

        let result = lines.join("\n");
        
        // Add metadata if output was truncated
        let original_line_count = output.lines().count();
        let final_line_count = lines.len();
        
        if original_line_count != final_line_count {
            format!(
                "[Output filtered: {} lines → {} lines]\n{}",
                original_line_count, final_line_count, result
            )
        } else {
            result
        }
    }
}

impl ToolHandler for CustomToolHandler {
    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        match Self::execute_command(&self.command, &self.name, arguments, allowed_base, self.timeout) {
            Ok(response) => response,
            Err(response) => response,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn get_definition(&self) -> ChatCompletionTool {
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: self.name.clone(),
                description: Some(self.description.clone()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "head_lines": {
                            "type": "integer",
                            "description": "Number of first lines to return (optional - returns all if not specified)",
                            "minimum": 1
                        },
                        "tail_lines": {
                            "type": "integer",
                            "description": "Number of last lines to return (optional - returns all if not specified)",
                            "minimum": 1
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to filter output (optional - returns all if not specified)"
                        }
                    },
                    "additionalProperties": false
                })),
            },
        }
    }

    fn reset_batch(&self) {
        // No state to reset for custom tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolStatus;

    #[test]
    fn test_post_process_head() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let result = CustomToolHandler::post_process_output(input, Some(2), None, &None);
        assert!(result.contains("[Output filtered: 5 lines → 2 lines]"));
        assert!(result.contains("line1"));
        assert!(result.contains("line2"));
        assert!(!result.contains("line3"));
    }

    #[test]
    fn test_post_process_tail() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let result = CustomToolHandler::post_process_output(input, None, Some(2), &None);
        assert!(result.contains("[Output filtered: 5 lines → 2 lines]"));
        assert!(result.contains("line4"));
        assert!(result.contains("line5"));
        assert!(!result.contains("line1"));
    }

    #[test]
    fn test_post_process_pattern() {
        let input = "error: foo\ninfo: bar\nerror: baz\ninfo: qux";
        let result = CustomToolHandler::post_process_output(input, None, None, &Some("error".to_string()));
        assert!(result.contains("error: foo"));
        assert!(result.contains("error: baz"));
        assert!(!result.contains("info:"));
    }

    #[test]
    fn test_post_process_combined() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let result = CustomToolHandler::post_process_output(input, Some(10), Some(2), &None);
        // Should apply head first (no change since 5 < 10), then tail (last 2)
        assert!(result.contains("line4"));
        assert!(result.contains("line5"));
        assert!(!result.contains("line1"));
    }

    #[test]
    fn test_execute_simple_command() {
        let args = serde_json::json!({});
        let result = CustomToolHandler::execute_command("echo 'Hello World'", "test_tool", &args, ".", 60).unwrap();
        assert_eq!(result.status, ToolStatus::Success);
        assert!(result.result.contains("Hello World"));
    }

    #[test]
    fn test_execute_with_head() {
        let args = serde_json::json!({
            "head_lines": 3
        });
        let result = CustomToolHandler::execute_command("seq 1 10", "test_tool", &args, ".", 60).unwrap();
        assert_eq!(result.status, ToolStatus::Success);
        assert!(result.result.contains("[Output filtered: 10 lines → 3 lines]"));
        // Check that we have lines 1, 2, 3
        assert!(result.result.contains("1"));
        assert!(result.result.contains("2"));
        assert!(result.result.contains("3"));
    }

    #[test]
    fn test_execute_with_tail() {
        let args = serde_json::json!({
            "tail_lines": 3
        });
        let result = CustomToolHandler::execute_command("seq 1 10", "test_tool", &args, ".", 60).unwrap();
        assert_eq!(result.status, ToolStatus::Success);
        assert!(result.result.contains("[Output filtered: 10 lines → 3 lines]"));
        // Check that we have the last 3 lines: 8, 9, 10
        assert!(result.result.contains("8"));
        assert!(result.result.contains("9"));
        assert!(result.result.contains("10"));
    }

    #[test]
    fn test_execute_with_pattern() {
        let args = serde_json::json!({
            "pattern": "error"
        });
        let result = CustomToolHandler::execute_command(
            "echo -e 'error: foo\\ninfo: bar\\nerror: baz'",
            "test_tool",
            &args,
            ".",
            60,
        ).unwrap();
        assert_eq!(result.status, ToolStatus::Success);
        assert!(result.result.contains("error: foo"));
        assert!(result.result.contains("error: baz"));
        assert!(!result.result.contains("info:"));
    }

    #[test]
    fn test_execute_combined_filters() {
        let args = serde_json::json!({
            "head_lines": 50,
            "tail_lines": 10,
            "pattern": "^[1-9]$"
        });
        let result = CustomToolHandler::execute_command("seq 1 100", "test_tool", &args, ".", 60).unwrap();
        assert_eq!(result.status, ToolStatus::Success);
        // After head(50) and tail(10), we get lines 41-50
        // Pattern filter keeps only single digits (none in 41-50)
        assert!(result.result.contains("[Output filtered"));
    }

    #[test]
    fn test_execute_invalid_regex() {
        let args = serde_json::json!({
            "pattern": "[invalid"
        });
        let result = CustomToolHandler::execute_command("echo 'test'", "test_tool", &args, ".", 60);
        // Should fail with validation error - invalid regex patterns are rejected
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("Invalid regex pattern"));
        assert!(err.result.contains("[invalid"));
    }

    #[test]
    fn test_execute_timeout() {
        let args = serde_json::json!({});
        // Use a very short timeout (1 second) with a command that would hang
        let result = CustomToolHandler::execute_command("sleep 5", "test_tool", &args, ".", 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("timed out"));
    }

    #[test]
    fn test_validate_negative_head_lines() {
        let args = serde_json::json!({
            "head_lines": -5
        });
        let result = CustomToolHandler::execute_command("echo 'test'", "test_tool", &args, ".", 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("head_lines must be positive"));
    }

    #[test]
    fn test_validate_invalid_head_lines_type() {
        let args = serde_json::json!({
            "head_lines": "not a number"
        });
        let result = CustomToolHandler::execute_command("echo 'test'", "test_tool", &args, ".", 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("head_lines must be an integer"));
    }

    #[test]
    fn test_validate_negative_tail_lines() {
        let args = serde_json::json!({
            "tail_lines": -10
        });
        let result = CustomToolHandler::execute_command("echo 'test'", "test_tool", &args, ".", 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("tail_lines must be positive"));
    }

    #[test]
    fn test_validate_invalid_pattern_type() {
        let args = serde_json::json!({
            "pattern": 12345
        });
        let result = CustomToolHandler::execute_command("echo 'test'", "test_tool", &args, ".", 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("pattern must be a string"));
    }

    #[test]
    fn test_validate_empty_pattern() {
        let args = serde_json::json!({
            "pattern": ""
        });
        let result = CustomToolHandler::execute_command("echo 'test'", "test_tool", &args, ".", 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, ToolStatus::Error);
        assert!(err.result.contains("pattern cannot be empty"));
    }
}
