use orderbook::{start_server, Simulator, OrderBook, FifoLevel};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let port = if args.len() > 1 {
        args[1].parse().unwrap_or(3000)
    } else {
        3000
    };
    
    let simulation_interval_ms = if args.len() > 2 {
        args[2].parse().unwrap_or(100)
    } else {
        100 // Default to 100ms intervals
    };
    
    println!("Order Book Server Starting...");
    println!("Port: {}", port);
    println!("Simulation interval: {}ms", simulation_interval_ms);
    
    // Create order book engine and simulator
    let engine = OrderBook::<FifoLevel>::new();
    let simulator = Simulator::new(engine);
    
    // Start the WebSocket server
    start_server(simulator, port, simulation_interval_ms).await?;
    
    Ok(())
}