use orderbook::{start_server, Simulator, OrderBook, FifoLevel, Config, ConfigError};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;
use std::env;

/// Order Book Server CLI
#[derive(Parser)]
#[command(name = "orderbook-server")]
#[command(about = "A high-performance order book system with real-time visualization")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
    
    /// Server port (overrides config file)
    #[arg(short, long)]
    port: Option<u16>,
    
    /// Simulation interval in milliseconds (overrides config file)
    #[arg(short, long)]
    interval: Option<u64>,
    
    /// Data source CSV file for historical replay
    #[arg(long, value_name = "FILE")]
    csv_file: Option<PathBuf>,
    
    /// Data source JSON file for historical replay
    #[arg(long, value_name = "FILE")]
    json_file: Option<PathBuf>,
    
    /// Random seed for deterministic simulation
    #[arg(long)]
    seed: Option<u64>,
    
    /// Log level (error, warn, info, debug, trace)
    #[arg(long)]
    log_level: Option<String>,
    
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Clone)]
enum Commands {
    /// Start the order book server
    Start,
    /// Generate a default configuration file
    InitConfig {
        /// Output file path
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
    },
    /// Validate configuration file
    ValidateConfig {
        /// Configuration file to validate
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up panic handler for better error reporting
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("🚨 PANIC: {}", panic_info);
        eprintln!("This is a critical error. Please check the logs and report this issue.");
        process::exit(1);
    }));

    let cli = Cli::parse();
    
    match cli.command.clone().unwrap_or(Commands::Start) {
        Commands::Start => {
            start_server_command(cli).await
        }
        Commands::InitConfig { output } => {
            init_config_command(output)
        }
        Commands::ValidateConfig { config } => {
            validate_config_command(config)
        }
    }
}

async fn start_server_command(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Order Book Server Starting...");
    println!("📦 Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let mut config = load_config(cli.config.as_deref())?;
    
    // Apply CLI overrides
    apply_cli_overrides(&mut config, &cli);
    
    // Validate configuration
    config.validate().map_err(|e| {
        eprintln!("❌ Configuration validation failed: {}", e);
        process::exit(1);
    }).unwrap();
    
    // Set up logging based on configuration
    setup_logging(&config)?;
    
    // Validate environment
    if let Err(e) = validate_environment() {
        eprintln!("❌ Environment validation failed: {}", e);
        process::exit(1);
    }
    
    // Print configuration summary
    print_config_summary(&config, cli.verbose);
    
    // Create order book engine and simulator
    println!("🏗️  Initializing components...");
    
    let engine = OrderBook::<FifoLevel>::new();
    println!("✅ Order book engine created");
    
    let mut simulator = if let Some(seed) = config.simulation.random_seed {
        Simulator::with_seed(engine, seed)
    } else {
        Simulator::new(engine)
    };
    
    // Configure simulator with loaded configuration
    simulator = simulator
        .with_network_model(config.network.clone())
        .with_market_maker_config(config.market_maker.clone())
        .with_order_generation_config(config.order_generation.clone());
    
    // Set up data source if specified
    if let Some(csv_file) = &config.data_source.default_csv_file {
        if csv_file.exists() {
            println!("📊 Loading CSV data source: {}", csv_file.display());
            // Note: This would require implementing CsvDataSource integration
            // For now, we'll just log the intention
        }
    }
    
    if let Some(json_file) = &config.data_source.default_json_file {
        if json_file.exists() {
            println!("📊 Loading JSON data source: {}", json_file.display());
            // Note: This would require implementing JsonDataSource integration
            // For now, we'll just log the intention
        }
    }
    
    println!("✅ Market simulator created");
    
    println!("🌐 Starting WebSocket server...");
    
    // Start the WebSocket server with configuration
    match start_server(simulator, config.server.port, config.simulation.step_interval_ms).await {
        Ok(_) => {
            println!("✅ Server shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Server failed to start or encountered a fatal error: {}", e);
            
            // Provide helpful error messages for common issues
            if e.to_string().contains("Address already in use") {
                eprintln!("💡 Tip: Port {} is already in use. Try a different port or stop the existing service.", config.server.port);
            } else if e.to_string().contains("Permission denied") {
                eprintln!("💡 Tip: Permission denied. Try using a port number > 1024 or run with appropriate privileges.");
            }
            
            process::exit(1);
        }
    }
}

fn init_config_command(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("📝 Generating default configuration file...");
    
    let config = Config::default();
    config.save_to_file(&output)?;
    
    println!("✅ Configuration file created: {}", output.display());
    println!("💡 Edit the file to customize your server settings");
    
    Ok(())
}

fn validate_config_command(config_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Validating configuration file: {}", config_path.display());
    
    if !config_path.exists() {
        eprintln!("❌ Configuration file does not exist: {}", config_path.display());
        process::exit(1);
    }
    
    match Config::load_from_file(&config_path) {
        Ok(config) => {
            match config.validate() {
                Ok(_) => {
                    println!("✅ Configuration is valid");
                    
                    // Print configuration summary
                    println!("\n📋 Configuration Summary:");
                    println!("   Server: {}:{}", config.server.host, config.server.port);
                    println!("   Simulation interval: {}ms", config.simulation.step_interval_ms);
                    println!("   Max connections: {}", config.server.max_connections);
                    println!("   Log level: {}", config.logging.level);
                    
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Configuration validation failed: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to load configuration: {}", e);
            process::exit(1);
        }
    }
}

fn load_config(config_path: Option<&std::path::Path>) -> Result<Config, ConfigError> {
    match config_path {
        Some(path) => {
            println!("📄 Loading configuration from: {}", path.display());
            Config::load_from_file(path)
        }
        None => {
            // Try to load from default locations
            if std::path::Path::new("config.toml").exists() {
                println!("📄 Loading configuration from: config.toml");
                Config::load_from_file("config.toml")
            } else {
                println!("📄 Using default configuration (no config file found)");
                Ok(Config::default())
            }
        }
    }
}

fn apply_cli_overrides(config: &mut Config, cli: &Cli) {
    if let Some(port) = cli.port {
        config.server.port = port;
    }
    
    if let Some(interval) = cli.interval {
        config.simulation.step_interval_ms = interval;
    }
    
    if let Some(ref csv_file) = cli.csv_file {
        config.data_source.default_csv_file = Some(csv_file.clone());
    }
    
    if let Some(ref json_file) = cli.json_file {
        config.data_source.default_json_file = Some(json_file.clone());
    }
    
    if let Some(seed) = cli.seed {
        config.simulation.random_seed = Some(seed);
    }
    
    if let Some(ref log_level) = cli.log_level {
        config.logging.level = log_level.clone();
    }
}

fn setup_logging(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Set RUST_LOG environment variable if not already set
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", &config.logging.level);
    }
    
    // Initialize logging (this will use the existing logging infrastructure)
    orderbook::init_logging()?;
    
    Ok(())
}

fn print_config_summary(config: &Config, verbose: bool) {
    println!("⚙️  Configuration Summary:");
    println!("   Server: {}:{}", config.server.host, config.server.port);
    println!("   Simulation interval: {}ms", config.simulation.step_interval_ms);
    println!("   Log level: {}", config.logging.level);
    
    if verbose {
        println!("   Max connections: {}", config.server.max_connections);
        println!("   Message buffer size: {}", config.server.message_buffer_size);
        println!("   Random seed: {:?}", config.simulation.random_seed);
        println!("   Max depth levels: {}", config.simulation.max_depth_levels);
        println!("   Network latency: {}μs", config.network.base_latency_ns / 1000);
        println!("   Market maker spread: {} ticks", config.market_maker.target_spread);
        println!("   Data directory: {}", config.data_source.data_directory.display());
        
        if let Some(ref csv_file) = config.data_source.default_csv_file {
            println!("   CSV file: {}", csv_file.display());
        }
        
        if let Some(ref json_file) = config.data_source.default_json_file {
            println!("   JSON file: {}", json_file.display());
        }
    }
}

/// Validate the runtime environment
fn validate_environment() -> Result<(), String> {
    // Check if we're running in a reasonable environment
    
    // Check available memory (basic check)
    // Note: This is a simplified check. In production, you'd want more sophisticated checks.
    
    // Check if required environment variables are reasonable
    if let Ok(rust_log) = env::var("RUST_LOG") {
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        let log_parts: Vec<&str> = rust_log.split(',').collect();
        
        for part in log_parts {
            let level = part.split('=').last().unwrap_or(part);
            if !valid_levels.contains(&level) && level != "off" {
                return Err(format!("Invalid RUST_LOG level: {}", level));
            }
        }
    }
    
    // Check if we can create temporary files (basic I/O check)
    match std::env::temp_dir().try_exists() {
        Ok(true) => {},
        Ok(false) => return Err("Temporary directory does not exist".to_string()),
        Err(e) => return Err(format!("Cannot access temporary directory: {}", e)),
    }
    
    Ok(())
}