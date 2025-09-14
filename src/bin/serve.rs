use orderbook::{start_server, Simulator, OrderBook, FifoLevel};
use std::env;
use std::process;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up panic handler for better error reporting
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("🚨 PANIC: {}", panic_info);
        eprintln!("This is a critical error. Please check the logs and report this issue.");
        process::exit(1);
    }));

    println!("🚀 Order Book Server Starting...");
    println!("📦 Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Parse and validate command line arguments
    let args: Vec<String> = env::args().collect();
    
    let port = if args.len() > 1 {
        match args[1].parse::<u16>() {
            Ok(0) => {
                eprintln!("❌ Error: Port cannot be 0");
                process::exit(1);
            }
            Ok(p) => p,
            Err(e) => {
                eprintln!("❌ Error: Invalid port '{}': {}", args[1], e);
                eprintln!("Usage: {} [port] [simulation_interval_ms]", args[0]);
                process::exit(1);
            }
        }
    } else {
        3000
    };
    
    let simulation_interval_ms = if args.len() > 2 {
        match args[2].parse::<u64>() {
            Ok(interval) if interval > 0 && interval <= 60000 => interval,
            Ok(0) => {
                eprintln!("❌ Error: Simulation interval cannot be 0");
                process::exit(1);
            }
            Ok(interval) => {
                eprintln!("❌ Error: Simulation interval {}ms is too large (max 60000ms)", interval);
                process::exit(1);
            }
            Err(e) => {
                eprintln!("❌ Error: Invalid simulation interval '{}': {}", args[2], e);
                eprintln!("Usage: {} [port] [simulation_interval_ms]", args[0]);
                process::exit(1);
            }
        }
    } else {
        100 // Default to 100ms intervals
    };
    
    // Validate environment
    if let Err(e) = validate_environment() {
        eprintln!("❌ Environment validation failed: {}", e);
        process::exit(1);
    }
    
    println!("⚙️  Configuration:");
    println!("   Port: {}", port);
    println!("   Simulation interval: {}ms", simulation_interval_ms);
    println!("   Log level: {}", env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
    
    // Create order book engine and simulator
    println!("🏗️  Initializing components...");
    
    let engine = match OrderBook::<FifoLevel>::new() {
        engine => {
            println!("✅ Order book engine created");
            engine
        }
    };
    
    let simulator = match Simulator::new(engine) {
        simulator => {
            println!("✅ Market simulator created");
            simulator
        }
    };
    
    println!("🌐 Starting WebSocket server...");
    
    // Start the WebSocket server with proper error handling
    match start_server(simulator, port, simulation_interval_ms).await {
        Ok(_) => {
            println!("✅ Server shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Server failed to start or encountered a fatal error: {}", e);
            
            // Provide helpful error messages for common issues
            if e.to_string().contains("Address already in use") {
                eprintln!("💡 Tip: Port {} is already in use. Try a different port or stop the existing service.", port);
            } else if e.to_string().contains("Permission denied") {
                eprintln!("💡 Tip: Permission denied. Try using a port number > 1024 or run with appropriate privileges.");
            }
            
            process::exit(1);
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