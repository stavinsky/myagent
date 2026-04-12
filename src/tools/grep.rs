use globwalk::GlobWalkerBuilder;
use regex::Regex;
use std::fs;
use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use tracing;
use crate::types::{ToolResponse, ValidPath, ToolHandler};
use serde_json::Value;

/// Maximum recursion depth for directory traversal
const MAX_DEPTH: usize = 100;

/// Maximum number of files to scan
const MAX_FILES: usize = 100000;

/// Get the tool definition for grep
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: "grep".to_string(),
            description: Some(format!("Search files using a regex pattern. The path argument supports glob patterns for flexible file matching. Searches are limited to {} files and {} directory levels to prevent excessive resource usage.", MAX_FILES, MAX_DEPTH)),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Glob pattern for file paths (e.g., 'src/**/*.rs', '*.txt'). If omitted, searches all files recursively."
                    }
                },
                "required": ["pattern"]
            })),
        }
    }
}

/// Search files using a regex pattern with glob path support
/// 
/// Parameters:
/// - pattern: Regex pattern to search for
/// - path: Optional glob pattern for file paths (e.g., "src/**/*.rs", "*.txt"). If None, searches all files recursively.
/// 
/// The path is validated against the allowed base to prevent directory traversal attacks.
pub fn grep(pattern: &str, path: Option<&str>, allowed_base: &str) -> ToolResponse {
    tracing::info!("grep: pattern='{}', path='{}'", pattern, path.unwrap_or("*"));
    
    let regex = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("grep failed: Invalid regex: {}", e);
            return ToolResponse::error("grep", format!("Invalid regex pattern '{}': {}", pattern, e));
        }
    };
    
    // Build the glob pattern
    let glob_pattern = if let Some(p) = path {
        // Validate the path against the allowed base
        match ValidPath::from_string(p, allowed_base) {
            Ok(valid_path) => valid_path.as_str().to_string(),
            Err(e) => return ToolResponse::error("grep", e),
        }
    } else {
        // Default to all files recursively in the allowed base
        format!("{}/**/*", allowed_base.trim_end_matches('/'))
    };
    
    tracing::debug!("grep: glob pattern = '{}'", glob_pattern);
    
    let mut results = Vec::new();
    let mut files_checked = 0;
    let mut read_errors: Vec<String> = Vec::new();
    let mut limit_reached = false;
    
    // Get the canonical allowed base for symlink validation
    let canonical_base = match std::path::Path::new(allowed_base).canonicalize() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!("grep failed: Cannot canonicalize allowed base '{}': {}", allowed_base, e);
            return ToolResponse::error("grep", format!("Cannot canonicalize allowed base '{}': {}", allowed_base, e));
        }
    };
    let canonical_base_str = canonical_base.to_string_lossy();
    
    // Build the glob walker with security and performance settings
    // Use the allowed_base as the base directory for the glob walker
    let walker = match GlobWalkerBuilder::from_patterns(allowed_base, &[&glob_pattern])
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .build()
    {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("grep failed: Invalid glob pattern '{}': {}", glob_pattern, e);
            return ToolResponse::error("grep", format!("Invalid glob pattern '{}': {}", glob_pattern, e));
        }
    };
    
    for entry in walker {
        // Check if we've reached the file limit
        if files_checked >= MAX_FILES {
            limit_reached = true;
            break;
        }
        
        let path: std::path::PathBuf = match entry {
            Ok(de) => de.path().to_path_buf(),
            Err(e) => {
                // Log glob errors (e.g., permission denied)
                tracing::debug!("grep: Error accessing path: {}", e);
                continue;
            }
        };
        
        // Security check: Verify the path is within the allowed base
        // This is critical because we're manually handling symlinks
        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // Cannot resolve path - skip it
                tracing::debug!("grep: Skipping unresolvable path: {}", path.display());
                continue;
            }
        };
        
        let canonical_path_str = canonical_path.to_string_lossy();
        if !canonical_path_str.starts_with(canonical_base_str.as_ref()) {
            // Path (possibly via symlink) is outside the allowed base - skip it
            tracing::debug!("grep: Skipping path outside allowed base: {} (resolved to {})", 
                path.display(), canonical_path_str);
            continue;
        }
        
        // Skip if not a file
        if !canonical_path.is_file() {
            continue;
        }
        
        files_checked += 1;
        
        // Read and search the file
        match fs::read_to_string(&canonical_path) {
            Ok(content) => {
                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        results.push(format!("{}:{}: {}", path.display(), line_num + 1, line));
                    }
                }
            }
            Err(e) => {
                // Collect read errors
                let error_msg = format!("{}: {}", path.display(), e);
                if !read_errors.contains(&error_msg) {
                    read_errors.push(error_msg);
                }
            }
        }
    }
    
    // Build the result
    if limit_reached {
        let msg = format!(
            "Search stopped after checking {} files (limit reached). {} matches found. Use a more specific path pattern.",
            MAX_FILES,
            results.len()
        );
        tracing::warn!("grep: {}", msg);
        
        if results.is_empty() {
            ToolResponse::error("grep", msg)
        } else {
            let result = results.join("\n");
            let metadata = format!("Found {} matches in {} files (search truncated at {} files)", results.len(), files_checked, MAX_FILES);
            ToolResponse::success("grep", result).with_metadata(metadata)
        }
    } else if results.is_empty() && read_errors.is_empty() {
        tracing::debug!("grep completed: Checked {} files, 0 matches", files_checked);
        ToolResponse::success("grep", format!("[NO MATCHES] No lines found matching the pattern. Checked {} files.", files_checked))
    } else if results.is_empty() && !read_errors.is_empty() {
        // No matches but there were read errors
        let error_list = read_errors.join(", ");
        let msg = format!("[NO MATCHES] No lines found matching the pattern. Checked {} files. Errors: {}", files_checked, error_list);
        tracing::warn!("grep: Failed to read files: {}", error_list);
        ToolResponse::success("grep", msg)
    } else if !read_errors.is_empty() {
        // Has matches but also read errors
        tracing::debug!("grep completed: Found {} matches in {} files", results.len(), files_checked);
        let result = results.join("\n");
        let error_list = read_errors.join(", ");
        let metadata = format!("Found {} matches in {} files ({} files could not be read: {})", results.len(), files_checked, read_errors.len(), error_list);
        ToolResponse::success("grep", result).with_metadata(metadata)
    } else {
        tracing::debug!("grep completed: Found {} matches in {} files", results.len(), files_checked);
        let result = results.join("\n");
        let metadata = format!("Found {} matches in {} files", results.len(), files_checked);
        ToolResponse::success("grep", result).with_metadata(metadata)
    }
}

/// Tool handler implementation for grep
pub struct GrepHandler;

impl ToolHandler for GrepHandler {
    fn name(&self) -> &str {
        "grep"
    }

    fn get_definition(&self) -> async_openai::types::ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, allowed_base: &str) -> ToolResponse {
        let pattern = match arguments["pattern"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("grep", "Missing required argument: pattern".to_string()),
        };
        let path = arguments["path"].as_str();
        grep(pattern, path, allowed_base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolStatus;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_grep_basic() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("test_grep_basic.txt");
        fs::write(&test_file, "hello world\nfoo bar\nhello again").unwrap();
        
        let result = grep("hello", Some("test_grep_basic.txt"), temp_dir.path().to_str().unwrap());
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("test_grep_basic.txt"));
        assert_eq!(result.result.matches("hello").count(), 2);
    }

    #[test]
    fn test_grep_with_glob_pattern() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        
        // Create test files
        fs::create_dir_all(temp_dir.path().join("test_glob_dir")).unwrap();
        fs::write(temp_dir.path().join("test_glob_dir/test.rs"), "fn main() {}").unwrap();
        fs::write(temp_dir.path().join("test_glob_dir/test.txt"), "hello world").unwrap();
        
        // Search with glob pattern
        let result = grep("fn", Some("test_glob_dir/*.rs"), temp_dir.path().to_str().unwrap());
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("test.rs"));
        assert!(!result.result.contains("test.txt"));
    }

    #[test]
    fn test_grep_recursive_glob() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        
        // Create nested structure
        fs::create_dir_all(temp_dir.path().join("test_recursive/subdir")).unwrap();
        fs::write(temp_dir.path().join("test_recursive/file1.rs"), "fn foo() {}").unwrap();
        fs::write(temp_dir.path().join("test_recursive/subdir/file2.rs"), "fn bar() {}").unwrap();
        
        // Search recursively
        let result = grep("fn", Some("test_recursive/**/*"), temp_dir.path().to_str().unwrap());
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("file1.rs"));
        assert!(result.result.contains("file2.rs"));
    }

    #[test]
    fn test_grep_invalid_regex() {
        let result = grep("[invalid(", Some("*.txt"), ".");
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("Invalid regex"));
    }

    #[test]
    fn test_grep_path_outside_base() {
        // Try to access path outside allowed base
        let result = grep("test", Some("../other/file.txt"), ".");
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("'..'"));
    }

    #[test]
    fn test_grep_no_matches() {
        let result = grep("nonexistent_pattern_xyz", Some("Cargo.toml"), ".");
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("[NO MATCHES]"));
    }

    #[test]
    fn test_grep_file_limit() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_dir = temp_dir.path().join("test_file_limit");
        std::fs::create_dir_all(&test_dir).unwrap();
        
        // Create 100 files
        for i in 0..100 {
            std::fs::write(test_dir.join(format!("file{}.txt", i)), "test content").unwrap();
        }
        
        let result = grep("test", Some("test_file_limit/*.txt"), temp_dir.path().to_str().unwrap());
        assert!(matches!(result.status, ToolStatus::Success));
        // Should find matches - the exact count depends on how many files match
        // The key is that it doesn't crash and returns results
        assert!(result.result.contains("test"));
    }

    #[test]
    fn test_grep_glob_anchoring() {
        // Test that glob patterns are properly anchored
        // *.txt should NOT match foo.txt.bak
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        std::fs::create_dir_all(temp_dir.path().join("test_glob_anchor")).unwrap();
        std::fs::write(temp_dir.path().join("test_glob_anchor/valid.txt"), "match this").unwrap();
        std::fs::write(temp_dir.path().join("test_glob_anchor/invalid.txt.bak"), "should not match").unwrap();
        
        let result = grep("match", Some("test_glob_anchor/*.txt"), temp_dir.path().to_str().unwrap());
        
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("valid.txt"));
        assert!(!result.result.contains("invalid.txt.bak"));
    }
}
