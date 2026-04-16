use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::Level;
use crate::types::Flow;
use std::collections::HashMap;

const APP_NAME: &str = "myagent";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomTool {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub prompts: Option<Prompts>,
    #[serde(default)]
    pub flows: std::collections::HashMap<String, Flow>,
    #[serde(default)]
    pub logging: Logging,
    #[serde(default)]
    pub common_system_prompt: Option<String>,
    #[serde(default)]
    pub custom_tools: HashMap<String, CustomTool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Prompts {
    pub review_system: String,
    pub review_user: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Logging {
    #[serde(default = "default_log_level")]
    pub level: LogLevel,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

impl Logging {
    pub fn to_tracing_level(&self) -> Level {
        match self.level {
            LogLevel::Trace => Level::TRACE,
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Info => Level::INFO,
            LogLevel::Warn => Level::WARN,
            LogLevel::Error => Level::ERROR,
        }
    }
}

impl Default for Logging {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: String::new(),
            api_key: None,
            base_url: None,
            prompts: None,
            flows: std::collections::HashMap::new(),
            logging: Logging::default(),
            common_system_prompt: None,
            custom_tools: std::collections::HashMap::new(),
        }
    }
}

impl Config {
    /// Get default config paths (user, then local)
    pub fn get_default_paths() -> (PathBuf, PathBuf) {
        let home_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        
        let user_config_dir = home_dir.join(".config").join(APP_NAME);
        let user_config_path = user_config_dir.join("config.yaml");
        let local_config_path = PathBuf::from(".").join(format!("{}.yaml", APP_NAME));
        
        (user_config_path, local_config_path)
    }

    /// Load config from a specific path (no merging)
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read config file: {:?}", path.as_ref()))?;
        
        let config: Config = serde_yaml::from_str(&content)
            .with_context(|| "failed to parse YAML config")?;
        
        Ok(config)
    }

    /// Load config with merging: user config + local config override
    /// Priority: local config overrides user config
    pub fn load_with_merge() -> Result<Self> {
        let (user_config_path, local_config_path) = Config::get_default_paths();
        
        // Start with default config
        let mut config = Config::default();
        
        // Load user config if exists
        if user_config_path.exists() {
            tracing::debug!("Loading user config from: {:?}", user_config_path);
            let user_config = Config::load(&user_config_path)
                .with_context(|| "failed to load user config")?;
            config = user_config;
        } else {
            tracing::debug!("No user config found at: {:?}", user_config_path);
        }
        
        // Load local config if exists and merge (override)
        if local_config_path.exists() {
            tracing::debug!("Loading local config from: {:?}", local_config_path);
            let local_config = Config::load(&local_config_path)
                .with_context(|| "failed to load local config")?;
            config = config.merge(local_config);
        } else {
            tracing::debug!("No local config found at: {:?}", local_config_path);
        }
        
        Ok(config)
    }

    /// Merge another config into this one, with the other config taking priority
    /// Custom tools from 'other' override those from 'self' if both have the same name
    fn merge(self, other: Config) -> Self {
        Config {
            model: if other.model.is_empty() { self.model } else { other.model },
            api_key: other.api_key.or(self.api_key),
            base_url: other.base_url.or(self.base_url),
            prompts: other.prompts.or(self.prompts),
            flows: if other.flows.is_empty() { self.flows } else { other.flows },
            logging: if other.logging.level == default_log_level() && self.logging.level != default_log_level() {
                self.logging
            } else {
                other.logging
            },
            common_system_prompt: other.common_system_prompt.or(self.common_system_prompt),
            // Merge custom_tools: other's tools override self's tools with the same name
            custom_tools: if other.custom_tools.is_empty() {
                self.custom_tools
            } else if self.custom_tools.is_empty() {
                other.custom_tools
            } else {
                // Merge both, with 'other' taking priority for duplicate names
                let mut merged = self.custom_tools;
                merged.extend(other.custom_tools);
                merged
            },
        }
    }

    pub fn get_api_key(&self) -> Result<String> {
        self.api_key.clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .context("API key not found in config or OPENAI_API_KEY environment variable")
    }

    pub fn get_flow(&self, name: &str) -> Option<&Flow> {
        self.flows.get(name)
    }

    pub fn validate(&self) -> Result<Vec<String>> {
        let mut errors = Vec::new();
        
        if self.model.is_empty() {
            errors.push("Model is required".to_string());
        }
        
        if self.flows.is_empty() {
            errors.push("At least one flow must be defined".to_string());
        }
        
        for (name, flow) in &self.flows {
            // Validate flow key is snake_case
            if !name.is_empty() && !name.chars().all(|c| c.is_lowercase() || c == '_' || c.is_ascii_digit()) {
                errors.push(format!("Flow key '{}' must be snake_case (lowercase, numbers, underscores only)", name));
            }
            if flow.system_prompt.is_empty() {
                errors.push(format!("Flow '{}' has empty system_prompt", name));
            }
            if flow.user_prompt.is_empty() {
                errors.push(format!("Flow '{}' has empty user_prompt", name));
            }
            if flow.tools.is_empty() {
                errors.push(format!("Flow '{}' has no tools defined", name));
            }
        }
        
        Ok(errors)
    }

    /// Get the combined system prompt for a flow
    /// Combines common system prompt (if configured) with flow-specific system prompt
    pub fn get_combined_system_prompt(&self, flow: &Flow) -> String {
        let mut combined = String::new();
        
        // Add common system prompt if available
        if let Some(ref common) = self.common_system_prompt {
            combined.push_str(common);
            if !common.ends_with('\n') {
                combined.push('\n');
            }
            combined.push('\n');
        }
        
        // Add flow-specific system prompt
        combined.push_str(&flow.system_prompt);
        
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load() {
        let config_content = r#"
model: gpt-4o
api_key: test-key
prompts:
  review_system: "You are a code reviewer."
  review_user: "Review this code: {file_content}"
  fix_system: "You are a code fixer."
  fix_user: "Fix this code: {file_content}"
"#;
        let config: Config = serde_yaml::from_str(config_content).unwrap();
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_custom_tools_config() {
        let config_content = r#"
model: gpt-4o
api_key: test-key
logging:
  level: info

custom_tools:
  cargo_test:
    name: "cargo_test"
    command: "cargo test"
    description: "Run cargo tests"
    timeout: 60
  cargo_build:
    name: "cargo_build"
    command: "cargo build"
    description: "Build the project"
    timeout: 120
"#;
        let config: Config = serde_yaml::from_str(config_content).unwrap();
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.custom_tools.len(), 2);
        
        let cargo_test = config.custom_tools.get("cargo_test").unwrap();
        assert_eq!(cargo_test.name, "cargo_test");
        assert_eq!(cargo_test.command, "cargo test");
        assert_eq!(cargo_test.timeout, 60);
        assert_eq!(cargo_test.description, Some("Run cargo tests".to_string()));
        
        let cargo_build = config.custom_tools.get("cargo_build").unwrap();
        assert_eq!(cargo_build.name, "cargo_build");
        assert_eq!(cargo_build.command, "cargo build");
        assert_eq!(cargo_build.timeout, 120);
    }
}
