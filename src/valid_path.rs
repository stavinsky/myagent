use std::path::Path;

/// A validated path that is guaranteed to be within the allowed base directory
/// This type ensures path validation cannot be forgotten - it must be constructed
/// through the `from_string` method which performs validation
#[derive(Debug, Clone)]
pub struct ValidPath {
    inner: String, // Keep the original relative path as string
}

impl ValidPath {
    /// Create a new ValidPath by validating the input string against the allowed base.
    /// Returns an error if the path contains ".." or resolves to a path outside the allowed base.
    /// 
    /// Note: This function validates the path structure but does not check if the file/directory
    /// actually exists. File existence should be checked separately when the path is used.
    pub fn from_string(path: &str, allowed_base: &str) -> Result<Self, String> {
        // Check for .. in the path BEFORE any other processing
        if path.contains("..") {
            return Err(format!("Path '{}' contains '..' which is not allowed", path));
        }
        
        // For paths that don't exist, we can still validate they're within the base
        // by canonicalizing the base and checking if the path would be within it
        let canonical_base = Path::new(allowed_base).canonicalize()
            .map_err(|e| format!("Cannot canonicalize allowed base '{}': {}", allowed_base, e))?;
        
        // Try to canonicalize the path, but if it doesn't exist, construct the expected path
        let canonical_path = if Path::new(path).exists() {
            Path::new(path).canonicalize()
                .map_err(|e| format!("Cannot canonicalize path '{}': {}", path, e))?
        } else {
            // For non-existent paths, construct what the canonical path would be
            // by joining the canonical base with the relative path
            canonical_base.join(path)
        };
        
        let path_str = canonical_path.to_string_lossy();
        let base_str = canonical_base.to_string_lossy();
        
        // Verify the path is within the allowed base
        if !path_str.starts_with(base_str.as_ref()) {
            return Err(format!("Path '{}' is outside allowed base '{}'", path, allowed_base));
        }
        
        Ok(Self {
            inner: path.to_string(), // Store the original relative path
        })
    }
    
    /// Get the inner string reference
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_valid_path_valid() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_dir_name = "test_validate";
        let test_dir = temp_dir.path().join(test_dir_name);
        std::fs::create_dir_all(test_dir.join("subdir")).unwrap();
        
        // Create the actual file
        std::fs::write(test_dir.join("file.txt"), "test content").unwrap();
        
        // Valid path within allowed base should succeed
        // Use relative path from the temp directory
        let result = ValidPath::from_string(
            &format!("{}/file.txt", test_dir_name),
            temp_dir.path().to_str().unwrap()
        );
        assert!(result.is_ok());
        
        // Temp directory is automatically cleaned up when it goes out of scope
    }
    
    #[test]
    fn test_valid_path_with_double_dots() {
        let result = ValidPath::from_string("../other/file.txt", "test_validate");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'..'"));
    }
    
    #[test]
    fn test_valid_path_outside_base() {
        let result = ValidPath::from_string("test_validate/../other/file.txt", "test_validate");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("'..'"));
    }
    
    #[test]
    fn test_valid_path_to_string() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_dir_name = "test_validate2";
        let test_dir = temp_dir.path().join(test_dir_name);
        std::fs::create_dir_all(&test_dir).unwrap();
        
        // Create the actual file
        std::fs::write(test_dir.join("file.txt"), "test content").unwrap();
        
        let valid_path = ValidPath::from_string(
            &format!("{}/file.txt", test_dir_name),
            temp_dir.path().to_str().unwrap()
        ).unwrap();
        assert!(valid_path.as_str().contains("file.txt"));
        // Temp directory is automatically cleaned up
    }
}
