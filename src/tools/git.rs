//! Git operations for version control
//! 
//! Provides tools for:
//! - Getting git status
//! - Getting git diff
//! - Staging changes
//! - Creating commits

use async_openai::types::chat::ChatCompletionTool;
use serde_json::{json, Value};
use std::process::Command;
use tracing;
use crate::types::{ToolResponse, ToolHandler};

/// Helper function to extract error message from git command output
fn extract_git_error(output: &std::process::Output, operation: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    if !stderr.is_empty() {
        stderr.to_string()
    } else if !stdout.is_empty() {
        stdout.to_string()
    } else {
        format!("{} failed with exit code: {}", operation, output.status)
    }
}

/// Get the current git status
pub fn git_status(path: Option<&str>, allowed_base: &str) -> ToolResponse {
    tracing::info!("git_status: path={:?}, allowed_base={:?}", path, allowed_base);
    
    // Validate path against allowed base if provided
    if let Some(p) = path {
        if !p.is_empty() {
            // Validate the path is within allowed base
            let valid_path = match crate::types::ValidPath::from_string(p, allowed_base) {
                Ok(vp) => vp,
                Err(e) => {
                    tracing::warn!("git_status path validation failed: {}", e);
                    return ToolResponse::error("git_status", format!("Path validation failed: {}", e));
                }
            };
            // Use the validated path
            let args = vec!["status", "--porcelain", "--", valid_path.as_str()];
            
            let output = match Command::new("git")
                .args(&args)
                .output() {
                    Ok(o) => o,
                    Err(e) => {
                        tracing::warn!("git status failed: {}", e);
                        return ToolResponse::error("git_status", format!("Failed to execute git status: {}", e));
                    }
            };

            if !output.status.success() {
                let error_msg = extract_git_error(&output, "git status");
                tracing::warn!("git status failed: {}", error_msg);
                return ToolResponse::error("git_status", format!("git status failed: {}", error_msg));
            }

            let status = match String::from_utf8(output.stdout) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("git status invalid UTF-8: {}", e);
                    return ToolResponse::error("git_status", format!("Invalid UTF-8 in git status output: {}", e));
                }
            };

            let result = if status.trim().is_empty() {
                "No changes detected".to_string()
            } else {
                status
            };
            
            let change_count = result.lines().count();
            tracing::debug!("git_status completed: {} changes", change_count);
            
            let metadata = if change_count == 0 {
                "No changes".to_string()
            } else {
                format!("{} changes", change_count)
            };
            
            return ToolResponse::success("git_status", result).with_metadata(metadata);
        }
    }
    
    // No path provided, get status for entire repository
    let output = match Command::new("git")
        .args(["status", "--porcelain"])
        .output() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("git status failed: {}", e);
                return ToolResponse::error("git_status", format!("Failed to execute git status: {}", e));
            }
    };

    if !output.status.success() {
        let error_msg = extract_git_error(&output, "git status");
        tracing::warn!("git status failed: {}", error_msg);
        return ToolResponse::error("git_status", format!("git status failed: {}", error_msg));
    }

    let status = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("git status invalid UTF-8: {}", e);
            return ToolResponse::error("git_status", format!("Invalid UTF-8 in git status output: {}", e));
        }
    };

    let result = if status.trim().is_empty() {
        "No changes detected".to_string()
    } else {
        status
    };
    
    let change_count = result.lines().count();
    tracing::debug!("git_status completed: {} changes", change_count);
    
    let metadata = if change_count == 0 {
        "No changes".to_string()
    } else {
        format!("{} changes", change_count)
    };
    
    ToolResponse::success("git_status", result).with_metadata(metadata)
}

/// Get the git diff for staged or unstaged changes
pub fn git_diff(path: Option<&str>, staged: bool, allowed_base: &str) -> ToolResponse {
    tracing::info!("git_diff: path={:?}, staged={}, allowed_base={:?}", path, staged, allowed_base);
    
    // Validate path against allowed base if provided
    if let Some(p) = path {
        if !p.is_empty() {
            // Validate the path is within allowed base
            let valid_path = match crate::types::ValidPath::from_string(p, allowed_base) {
                Ok(vp) => vp,
                Err(e) => {
                    tracing::warn!("git_diff path validation failed: {}", e);
                    return ToolResponse::error("git_diff", format!("Path validation failed: {}", e));
                }
            };
            // Use the validated path
            let mut args = vec!["diff"];
            if staged {
                args.push("--staged");
            }
            args.push("--");
            args.push(valid_path.as_str());
            
            let output = match Command::new("git")
                .args(&args)
                .output() {
                    Ok(o) => o,
                    Err(e) => {
                        tracing::warn!("git diff failed: {}", e);
                        return ToolResponse::error("git_diff", format!("Failed to execute git diff: {}", e));
                    }
            };

            if !output.status.success() {
                let error_msg = extract_git_error(&output, "git diff");
                tracing::warn!("git diff failed: {}", error_msg);
                return ToolResponse::error("git_diff", format!("git diff failed: {}", error_msg));
            }

            let diff = match String::from_utf8(output.stdout) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("git diff invalid UTF-8 failed: {}", e);
                    return ToolResponse::error("git_diff", format!("Invalid UTF-8 in git diff output: {}", e));
                }
            };

            let result = if diff.trim().is_empty() {
                "No changes to show".to_string()
            } else {
                diff
            };
            
            let line_count = result.lines().count();
            tracing::debug!("git_diff completed: {} lines", line_count);
            
            let metadata = if line_count == 0 {
                "No changes".to_string()
            } else {
                format!("{} lines", line_count)
            };
            
            return ToolResponse::success("git_diff", result).with_metadata(metadata);
        }
    }
    
    // No path provided, get diff for entire repository
    let mut args = vec!["diff"];
    if staged {
        args.push("--staged");
    }
    
    let output = match Command::new("git")
        .args(&args)
        .output() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("git diff failed: {}", e);
                return ToolResponse::error("git_diff", format!("Failed to execute git diff: {}", e));
            }
    };

    if !output.status.success() {
        let error_msg = extract_git_error(&output, "git diff");
        tracing::warn!("git diff failed: {}", error_msg);
        return ToolResponse::error("git_diff", format!("git diff failed: {}", error_msg));
    }

    let diff = match String::from_utf8(output.stdout) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("git diff invalid UTF-8 failed: {}", e);
            return ToolResponse::error("git_diff", format!("Invalid UTF-8 in git diff output: {}", e));
        }
    };

    let result = if diff.trim().is_empty() {
        "No changes to show".to_string()
    } else {
        diff
    };
    
    let line_count = result.lines().count();
    tracing::debug!("git_diff completed: {} lines", line_count);
    
    let metadata = if line_count == 0 {
        "No changes".to_string()
    } else {
        format!("{} lines", line_count)
    };
    
    ToolResponse::success("git_diff", result).with_metadata(metadata)
}

/// Stage specific files or all changes
/// Handles both file additions (git add) and deletions (git rm)
pub fn git_stage(file_path: &str) -> ToolResponse {
    tracing::info!("git_stage: {}", file_path);
    
    let file_exists = std::path::Path::new(file_path).exists();
    
    let args = if file_exists {
        vec!["add", file_path]
    } else {
        vec!["rm", "--cached", file_path]
    };
    
    let output = match Command::new("git")
        .args(&args)
        .output() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("git {} failed: {}", if file_exists { "add" } else { "rm" }, e);
                return ToolResponse::error("git_stage", format!("Failed to execute git {}: {}", if file_exists { "add" } else { "rm" }, e));
            }
    };

    if !output.status.success() {
        let action = if file_exists { "add" } else { "rm" };
        let error_msg = extract_git_error(&output, &format!("git {}", action));
        tracing::warn!("git {} failed: {}", action, error_msg);
        
        // Special handling: if file doesn't exist and git rm also fails (e.g., file already staged for deletion),
        // we can consider it a success since the end result is what we want
        if !file_exists && error_msg.contains("did not match any file") {
            tracing::info!("File {} is already deleted or staged for deletion", file_path);
            return ToolResponse::success("git_stage", format!("File {} is already deleted or staged for deletion", file_path));
        }
        
        return ToolResponse::error("git_stage", format!("git {} failed: {}", action, error_msg));
    }

    let action = if file_exists { "Staged" } else { "Staged deletion of" };
    tracing::debug!("git_stage completed: {}", file_path);
    
    ToolResponse::success("git_stage", format!("{}: {}", action, file_path))
}

/// Create a git commit
pub fn git_commit(title: &str, message: &str) -> ToolResponse {
    tracing::info!("git_commit: title='{}'", title);
    
    let mut cmd = Command::new("git");
    cmd.arg("commit");
    
    cmd.args(["-m", title]);
    if !message.is_empty() && message != title {
        cmd.args(["-m", message]);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("git commit failed: {}", e);
            return ToolResponse::error("git_commit", format!("Failed to execute git commit: {}", e));
        }
    };

    if !output.status.success() {
        let error_msg = extract_git_error(&output, "git commit");
        tracing::warn!("git commit failed: {}", error_msg);
        return ToolResponse::error("git_commit", format!("git commit failed: {}", error_msg));
    }

    tracing::info!("git_commit completed: {}", title);
    
    ToolResponse::success("git_commit", format!("Committed: {}", title))
}

/// Get git log (recent commits)
pub fn git_log(count: Option<usize>) -> ToolResponse {
    tracing::info!("git_log: count={:?}", count);
    
    let mut cmd = Command::new("git");
    cmd.args(["log", "--oneline", "--graph", "-10"]);
    
    if let Some(c) = count {
        cmd.args(["-n", &c.to_string()]);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("git log failed: {}", e);
            return ToolResponse::error("git_log", format!("Failed to execute git log: {}", e));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("git log failed: {}", stderr);
        return ToolResponse::error("git_log", format!("git log failed: {}", stderr));
    }

    let log = match String::from_utf8(output.stdout) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("git log invalid UTF-8: {}", e);
            return ToolResponse::error("git_log", format!("Invalid UTF-8 in git log output: {}", e));
        }
    };

    let commit_count = log.lines().count();
    tracing::debug!("git_log completed: {} commits", commit_count);
    
    ToolResponse::success("git_log", log).with_metadata(format!("{} commits", commit_count))
}

/// Tool handler for git_status
pub struct GitStatusHandler;

impl ToolHandler for GitStatusHandler {
    fn name(&self) -> &str {
        "git_status"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        serde_json::from_value(get_git_status_tool_definition()).unwrap()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let path = arguments["path"].as_str();
        git_status(path, allowed_base)
    }
}

/// Tool handler for git_diff
pub struct GitDiffHandler;

impl ToolHandler for GitDiffHandler {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        serde_json::from_value(get_git_diff_tool_definition()).unwrap()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let path = arguments["path"].as_str();
        let staged = arguments["staged"].as_bool().unwrap_or(false);
        git_diff(path, staged, allowed_base)
    }
}

/// Tool handler for git_stage
pub struct GitStageHandler;

impl ToolHandler for GitStageHandler {
    fn name(&self) -> &str {
        "git_stage"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        serde_json::from_value(get_git_stage_tool_definition()).unwrap()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let file_path = match arguments["file_path"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("git_stage", "Missing required argument: file_path".to_string()),
        };
        let valid_path = match crate::types::ValidPath::from_string(file_path, allowed_base) {
            Ok(vp) => vp,
            Err(e) => return ToolResponse::error("git_stage", e),
        };
        git_stage(valid_path.as_str())
    }
}

/// Tool handler for git_commit
pub struct GitCommitHandler;

impl ToolHandler for GitCommitHandler {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        serde_json::from_value(get_git_commit_tool_definition()).unwrap()
    }

    fn execute(&self, arguments: &Value, _allowed_base: &str) -> ToolResponse {
        let title = match arguments["title"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("git_commit", "Missing required argument: title".to_string()),
        };
        let message = arguments["message"].as_str().unwrap_or("");
        git_commit(title, message)
    }
}

/// Tool handler for git_log
pub struct GitLogHandler;

impl ToolHandler for GitLogHandler {
    fn name(&self) -> &str {
        "git_log"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        serde_json::from_value(get_git_log_tool_definition()).unwrap()
    }

    fn execute(&self, arguments: &Value, _allowed_base: &str) -> ToolResponse {
        let limit = arguments["limit"].as_u64().map(|l| l as usize);
        git_log(limit)
    }
}

// Tool definitions for async-openai

pub fn get_git_status_tool_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "git_status",
            "description": "Get git status for repository or specific path",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional path to check status for"
                    }
                },
                "required": []
            }
        }
    })
}

pub fn get_git_diff_tool_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "git_diff",
            "description": "Get git diff (staged or unstaged changes)",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional path to get diff for"
                    },
                    "staged": {
                        "type": "boolean",
                        "description": "Get staged changes if true, unstaged if false",
                        "default": false
                    }
                },
                "required": []
            }
        }
    })
}

pub fn get_git_stage_tool_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "git_stage",
            "description": "Stage file(s) for commit",
            "parameters": {
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to file(s) to stage"
                    }
                },
                "required": ["file_path"]
            }
        }
    })
}

pub fn get_git_commit_tool_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "git_commit",
            "description": "Create a git commit with title and message",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Commit title (first line, max 50 chars)"
                    },
                    "message": {
                        "type": "string",
                        "description": "Full commit message"
                    }
                },
                "required": ["title"]
            }
        }
    })
}

pub fn get_git_log_tool_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "git_log",
            "description": "Get git log (recent commits)",
            "parameters": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of commits to show"
                    }
                },
                "required": []
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolStatus;

    #[test]
    fn test_git_status_with_path_outside_base() {
        // Test that path outside allowed base is rejected
        let allowed_base = ".";
        let result = git_status(Some("../other/file.txt"), allowed_base);
        assert!(matches!(result.status, ToolStatus::Error), "Path outside base should be rejected");
    }

    #[test]
    fn test_git_status_with_double_dots() {
        // Test that paths with .. are rejected
        let allowed_base = ".";
        let result = git_status(Some("src/../other/file.txt"), allowed_base);
        assert!(matches!(result.status, ToolStatus::Error), "Path with .. should be rejected");
    }

    #[test]
    fn test_git_diff_with_path_outside_base() {
        // Test that path outside allowed base is rejected
        let allowed_base = ".";
        let result = git_diff(Some("../other/file.txt"), false, allowed_base);
        assert!(matches!(result.status, ToolStatus::Error), "Path outside base should be rejected");
    }

    #[test]
    fn test_git_diff_with_double_dots() {
        // Test that paths with .. are rejected
        let allowed_base = ".";
        let result = git_diff(Some("src/../other/file.txt"), false, allowed_base);
        assert!(matches!(result.status, ToolStatus::Error), "Path with .. should be rejected");
    }

    #[test]
    fn test_git_stage_with_path_outside_base() {
        // Test that path outside allowed base is rejected
        let handler = GitStageHandler;
        let args = serde_json::json!({
            "file_path": "../other/file.txt"
        });
        
        let result = handler.execute(&args, ".");
        assert!(matches!(result.status, ToolStatus::Error), "Path outside base should be rejected");
    }

    #[test]
    fn test_git_status_handler_with_valid_path() {
        let handler = GitStatusHandler;
        let args = serde_json::json!({
            "path": "Cargo.toml"
        });
        
        let result = handler.execute(&args, ".");
        assert!(matches!(result.status, ToolStatus::Success));
    }

    #[test]
    fn test_git_diff_handler_with_valid_path() {
        let handler = GitDiffHandler;
        let args = serde_json::json!({
            "path": "Cargo.toml",
            "staged": false
        });
        
        let result = handler.execute(&args, ".");
        assert!(matches!(result.status, ToolStatus::Success));
    }
}
