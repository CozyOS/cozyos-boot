use clap::Parser;
use colorize::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use tokio;
use std::path::PathBuf;
use dirs;
use std::env;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration file (default: ~/.config/cozyboot/cozyboot.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Enable debug mode
    #[arg(short, long)]
    debug: bool,

    /// Boot string to pass directly to CozyOS
    #[arg(short, long)]
    boot_string: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CozyBootConfig {
    main: MainConfig,
    bootargs: std::collections::HashMap<String, String>,
    bin: BinSettings,
}

#[derive(Debug, Serialize, Deserialize)]
struct MainConfig {
    kern_root: String,
    user_root: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BinSettings {
    allow_32bit: Option<bool>,
    allow_universal: Option<bool>,
    allow_64bit: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MountPoint {
    host_path: String,
    guest_path: String,
    readonly: Option<bool>,
}

// Add this function to handle config path resolution
fn get_config_path(config_arg: Option<String>) -> PathBuf {
    if let Some(path) = config_arg {
        PathBuf::from(path)
    } else {
        let home = dirs::home_dir().expect("Could not find home directory");
        home.join(".config/cozyboot/cozyboot.toml")
    }
}

// Add this function to ensure config directory exists
fn ensure_config_dir() -> Result<PathBuf, std::io::Error> {
    let config_dir = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".config/cozyboot");

    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
        
        // Create default config file
        let default_config = include_str!("../default_config.toml");
        std::fs::write(config_dir.join("cozyboot.toml"), default_config)?;
    }

    Ok(config_dir)
}

// Fix the expand_variables function
fn expand_variables(path: &str) -> String {
    let mut result = path.to_string();
    
    // Handle $(devroot) variable - default to current directory if not set
    let devroot = env::var("DEVROOT").unwrap_or_else(|_| ".".to_string());
    result = result.replace("$(devroot)", &devroot);
    
    // Convert to absolute path if it's relative
    if let Ok(absolute_path) = std::fs::canonicalize(&result) {
        absolute_path.to_string_lossy().to_string()
    } else {
        result
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    // Ensure config directory exists
    let _config_dir = ensure_config_dir()?;
    
    // Get config path
    let config_path = get_config_path(args.config);
    
    if !config_path.exists() {
        eprintln!("{}", format!("Error: Configuration file not found at {}", config_path.display()).red());
        std::process::exit(1);
    }

    if args.verbose {
        println!("{}", format!("Reading configuration from {}", config_path.display()).blue());
    }

    let config_content = fs::read_to_string(&config_path)?;
    if args.verbose {
        println!("Config content:\n{}", config_content);
    }
    let config: CozyBootConfig = match toml::from_str(&config_content) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to parse config file: {}", e);
            eprintln!("Config content:\n{}", config_content);
            std::process::exit(1);
        }
    };

    // Validate OS path
    let expanded_kern_root = expand_variables(&config.main.kern_root);
    if !Path::new(&expanded_kern_root).exists() {
        eprintln!("{}", format!("Error: OS path '{}' does not exist (expanded from '{}')", 
            expanded_kern_root, config.main.kern_root).red());
        std::process::exit(1);
    }

    if args.verbose {
        println!("{}", "Building CozyOS command...".blue());
    }

    // Build command arguments
    let mut command = Command::new("cozy-os");

    // Set kernel and user roots with expanded paths
    command.arg("--kern-root").arg(expand_variables(&config.main.kern_root))
           .arg("--user-root").arg(expand_variables(&config.main.user_root));

    // Add boot arguments
    for (key, value) in config.bootargs {
        command.arg(format!("--{}={}", key, value));
    }

    // Configure binary settings
    if let Some(allow_32bit) = config.bin.allow_32bit {
        command.arg("--allow-32bit").arg(allow_32bit.to_string());
    }
    if let Some(allow_universal) = config.bin.allow_universal {
        command.arg("--allow-universal").arg(allow_universal.to_string());
    }
    if let Some(allow_64bit) = config.bin.allow_64bit {
        command.arg("--allow-64bit").arg(allow_64bit.to_string());
    }

    // Add boot string if provided
    if let Some(boot_string) = args.boot_string {
        command.arg("--boot-string").arg(boot_string);
    }

    // Add debug mode if enabled
    if args.debug {
        command.arg("--debug");
        if args.verbose {
            println!("{}", "Debug mode enabled".blue());
        }
    }

    if args.verbose {
        println!("{}", "Starting CozyOS...".green());
    }

    // Execute the command
    let status = command.status()?;

    if !status.success() {
        eprintln!("{}", "Error: CozyOS failed to start".red());
        std::process::exit(status.code().unwrap_or(1));
    }

    if args.verbose {
        println!("{}", "CozyOS started successfully!".green());
    }

    Ok(())
}
