mod client;
mod config;
mod flow;
mod logging;
mod tools;
mod tui;
mod types;
mod valid_path;

use crate::config::Config;
use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "myagent")]
#[command(about = "A CLI tool for executing configurable AI workflows")]
struct Args {
    /// Path to the YAML configuration file (overrides default locations, no merging)
    #[arg(short, long)]
    config: Option<String>,

    /// Allowed base directory for file operations (prevents path traversal)
    #[arg(long, default_value = ".")]
    allowed_base: String,

    /// List all available flows
    #[arg(long)]
    list_flows: bool,

    /// Check and validate configuration
    #[arg(long)]
    check_config: bool,

    /// Flow name to execute (followed by flow arguments)
    #[arg(long, num_args = 1..)]
    flow: Option<Vec<String>>,

    /// Print the rendered prompt with all substitutions and exit
    #[arg(long)]
    print_prompt: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load config: either from explicit path or with merging from default locations
    let config = if let Some(config_path) = &args.config {
        tracing::debug!("Loading config from explicit path: {:?}", config_path);
        Config::load(config_path)
            .with_context(|| format!("failed to load config from {}", config_path))?
    } else {
        tracing::debug!("Loading config with merge from default locations");
        Config::load_with_merge()
            .context("failed to load config from default locations")?
    };

    logging::init(config.logging.to_tracing_level());

    if args.check_config {
        let errors = config.validate()?;
        if errors.is_empty() {
            println!("Configuration is valid");
            return Ok(());
        } else {
            eprintln!("Configuration errors:");
            for error in errors {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        }
    }

    if args.list_flows || args.flow.is_none() {
        println!("Available flows:\n");
        for flow in config.list_flows() {
            println!("  {} - {}", flow.name, flow.description);
            if !flow.arguments.is_empty() {
                println!("    Arguments:");
                for arg in &flow.arguments {
                    let required = if arg.required { "(required)" } else { "(optional)" };
                    println!("      • {} {} - {}", arg.name, required, arg.description);
                }
            }
        }
        return Ok(());
    }

    let flow_args = match &args.flow {
        Some(args) => args,
        None => {
            println!("No flow specified. Use --list-flows to see available flows.\n");
            return Ok(());
        }
    };
    
    if flow_args.is_empty() {
        println!("No flow specified. Use --list-flows to see available flows.\n");
        return Ok(());
    }

    let flow_name = &flow_args[0];
    let flow = config.get_flow(flow_name)
        .with_context(|| format!("Flow '{}' not found in config", flow_name))?;

    println!("Executing flow: {} - {}", flow.name, flow.description);
    println!("Available tools: {}", flow.tools.join(", ")); 
    println!();

    let mut flow_arguments: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    
    for (i, arg_def) in flow.arguments.iter().enumerate() {
        if arg_def.required {
            if i + 1 < flow_args.len() {
                flow_arguments.insert(arg_def.name.clone(), flow_args[i + 1].clone());
            } else {
                anyhow::bail!("Missing required argument '{}' for flow '{}'", arg_def.name, flow.name);
            }
        } else if i + 1 < flow_args.len() {
            flow_arguments.insert(arg_def.name.clone(), flow_args[i + 1].clone());
        }
    }

    if args.print_prompt {
        let rendered_user_prompt = flow::render_prompt(&flow.user_prompt, &flow_arguments)
            .context("Failed to render user prompt template")?;
        
        let rendered_system_prompt = flow::render_prompt(&flow.system_prompt, &flow_arguments)
            .context("Failed to render system prompt template")?;
        
        println!("=== System Prompt ===");
        if rendered_system_prompt.trim().is_empty() {
            println!("(No system prompt content)");
        } else {
            println!("{}", rendered_system_prompt);
        }
        
        println!("\n=== User Prompt ===");
        println!("{}", rendered_user_prompt);
        
        return Ok(());
    }

    let paths_to_validate: Vec<(String, String)> = flow_arguments.iter()
        .filter(|(key, _)| key.contains("path") || key.contains("file"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    
    for (key, value) in paths_to_validate {
        let validated = validate_file_path(Path::new(&value), Path::new(&args.allowed_base))
            .with_context(|| format!("invalid path for argument '{}': {}", key, value))?;
        
        let allowed_base_path = Path::new(&args.allowed_base);
        let allowed_base_canonical = allowed_base_path.canonicalize()
            .unwrap_or_else(|_| allowed_base_path.to_path_buf());
        
        let relative_path = validated.strip_prefix(&allowed_base_canonical)
            .unwrap_or(Path::new(&value));
        flow_arguments.insert(key, relative_path.to_string_lossy().to_string());
    }

    let client = client::OpenAIClient::new(config.clone(), args.allowed_base)?;

    let user_prompt = flow::render_prompt(&flow.user_prompt, &flow_arguments)
        .context("Failed to render prompt template")?;
    
    println!("User prompt:");
    println!("{}", user_prompt);
    println!();

    let result = client.execute_flow(flow, &user_prompt, &flow.tools).await?;

    println!("Flow completed!");
    println!("{}", result);

    Ok(())
}

/// Validates a file path to prevent path traversal attacks
fn validate_file_path(file_path: &Path, allowed_base: &Path) -> Result<PathBuf> {
    let allowed_base = allowed_base.canonicalize()
        .with_context(|| format!("failed to canonicalize allowed base directory: {}", allowed_base.display()))?;

    let full_path = allowed_base.join(file_path);

    if !full_path.starts_with(&allowed_base) {
        anyhow::bail!(
            "Access denied: {} is outside allowed directory {}",
            full_path.display(),
            allowed_base.display()
        );
    }
    
    let relative_path = full_path.strip_prefix(&allowed_base)
        .with_context(|| "failed to strip allowed base prefix")?;
    
    for component in relative_path.components() {
        if let std::path::Component::ParentDir = component {
            anyhow::bail!("Path traversal detected: parent directory reference not allowed");
        }
    }
    
    Ok(full_path)
}