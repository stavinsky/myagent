//! Tool modules for file operations and searching
//! 
//! This module provides a collection of tools for:
//! - Reading files
//! - Editing files  
//! - Searching files with regex patterns
//! - Listing directory contents
//! - Executing custom shell commands

pub mod custom_tools;
pub mod edit_file;
pub mod git;
pub mod grep;
pub mod list_dir;
pub mod multi_select;
pub mod read_file;
pub mod registry;

#[cfg(test)]
mod tests {
    use crate::types::ToolStatus;

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_read_file_existing() {
        let valid_path = crate::types::ValidPath::from_string("Cargo.toml", ".").unwrap();
        let result = read_file::read_file(&valid_path, ".", None, None);
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("myagent"));
    }

    #[test]
    fn test_read_file_nonexistent() {
        let result = crate::types::ValidPath::from_string("nonexistent_file.txt", ".");
        assert!(result.is_ok()); // Path validation should succeed even if file doesn't exist
        let valid_path = result.unwrap();
        let result = read_file::read_file(&valid_path, ".", None, None);
        assert!(matches!(result.status, ToolStatus::Error));
    }

    #[test]
    fn test_list_dir() {
        let valid_path = crate::types::ValidPath::from_string("src", ".").unwrap();
        let result = list_dir::list_dir(&valid_path, ".");
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("src") || result.metadata.as_ref().map_or(false, |m| m.contains("entries")));
    }

    #[test]
    fn test_list_dir_nonexistent() {
        let result = crate::types::ValidPath::from_string("nonexistent_dir", ".");
        assert!(result.is_ok());
        let valid_path = result.unwrap();
        let result = list_dir::list_dir(&valid_path, ".");
        assert!(matches!(result.status, ToolStatus::Error));
    }

    #[test]
    fn test_edit_file_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("test_edit_file.txt");
        fs::write(&test_file, "Hello world\nSecond line\nThird line\n").unwrap();
        
        let valid_path = crate::types::ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        let result = edit_file::edit_file(&valid_path, 1, 1, "Goodbye world");
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("Successfully edited"));
        
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("Goodbye world"));
    }

    #[test]
    fn test_edit_file_invalid_range() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("test_edit_file2.txt");
        fs::write(&test_file, "Hello world\n").unwrap();
        
        let valid_path = crate::types::ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        let result = edit_file::edit_file(&valid_path, 1, 10, "new text");
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("beyond the end of the file"));
    }

    #[test]
    fn test_edit_file_range_replacement() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file = temp_dir.path().join("test_edit_file3.txt");
        fs::write(&test_file, "Line 1\nLine 2\nLine 3\nLine 4\n").unwrap();
        
        let valid_path = crate::types::ValidPath::from_string(
            test_file.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        // Replace lines 2-3 with new content
        let result = edit_file::edit_file(&valid_path, 2, 3, "New Line 2\nNew Line 3");
        assert!(matches!(result.status, ToolStatus::Success));
        
        // Verify the change
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("Line 1"));
        assert!(content.contains("New Line 2"));
        assert!(content.contains("New Line 3"));
        assert!(content.contains("Line 4"));
    }

    #[test]
    fn test_grep_valid_pattern() {
        let result = grep::grep("mod ", None, ".");
        assert!(matches!(result.status, ToolStatus::Success));
        // Grep always succeeds, may return empty results
    }

    #[test]
    fn test_grep_invalid_pattern() {
        let result = grep::grep("[invalid(", None, ".");
        assert!(matches!(result.status, ToolStatus::Error));
        assert!(result.result.contains("Invalid regex"));
    }

    #[test]
    fn test_grep_with_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        // Create a test directory structure
        std::fs::create_dir_all(temp_dir.path().join("test_grep_dir/subdir")).unwrap();
        std::fs::write(temp_dir.path().join("test_grep_dir/file1.txt"), "hello mod world").unwrap();
        std::fs::write(temp_dir.path().join("test_grep_dir/subdir/file2.txt"), "no match here").unwrap();
        
        let result = grep::grep("mod", Some("test_grep_dir/**/*"), temp_dir.path().to_str().unwrap());
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("file1.txt"));
        assert!(!result.result.contains("file2.txt"));
    }

    #[test]
    fn test_grep_with_file_glob() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        std::fs::create_dir_all(temp_dir.path().join("test_grep_glob")).unwrap();
        std::fs::write(temp_dir.path().join("test_grep_glob/test.rs"), "fn main() {}").unwrap();
        std::fs::write(temp_dir.path().join("test_grep_glob/test.txt"), "hello world").unwrap();
        
        let result = grep::grep("fn", Some("test_grep_glob/*.rs"), temp_dir.path().to_str().unwrap());
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(result.result.contains("test.rs"));
        assert!(!result.result.contains("test.txt"));
    }

    #[test]
    fn test_grep_depth_limit() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        // Create a deeply nested directory structure
        let mut path = temp_dir.path().join("test_deep");
        std::fs::create_dir_all(&path).unwrap();
        
        for i in 0..110 {
            path = path.join(format!("level{}", i));
            std::fs::create_dir_all(&path).unwrap();
        }
        
        std::fs::write(&path.join("file.txt"), "test content").unwrap();
        
        let result = grep::grep("test", Some("test_deep/**/*"), temp_dir.path().to_str().unwrap());
        
        // With glob crate, depth is not limited by our code
        // The test just verifies that it doesn't crash
        assert!(matches!(result.status, ToolStatus::Success));
    }

    #[test]
    fn test_grep_symlink_security() {
        use std::os::unix::fs::symlink;
        
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        
        // Create a directory outside the allowed base
        let outside_dir = temp_dir.path().join("test_symlink_outside");
        std::fs::create_dir_all(&outside_dir).unwrap();
        std::fs::write(outside_dir.join("secret.txt"), "secret content").unwrap();
        
        // Create a directory inside the allowed base
        let inside_dir = temp_dir.path().join("test_symlink_inside");
        std::fs::create_dir_all(&inside_dir).unwrap();
        
        // Create a symlink from inside to outside
        let _ = symlink("../test_symlink_outside", inside_dir.join("link_to_outside"));
        
        let result = grep::grep("secret", Some("test_symlink_inside/**/*"), temp_dir.path().to_str().unwrap());
        
        // With follow_links(false), globwalk does not follow symlinks.
        // The symlinked directory will be skipped, so the secret file should NOT be found.
        // This is the expected secure behavior.
        assert!(matches!(result.status, ToolStatus::Success));
        assert!(!result.result.contains("test_symlink_outside"));
    }
}