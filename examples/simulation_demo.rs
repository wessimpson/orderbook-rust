use orderbook::{
    Simulator, OrderBook, FifoLevel, NetModel, MarketMakerConfig, OrderGenerationConfig,
    SimulationMode, price_utils
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let _ = orderbook::init_logging();

    println!("=== Order Book Simulation Demo ===\n");

    // Create order book engine
    let engine = OrderBook::<FifoLevel>::new();

    // Configure network model for realistic latency simulation
    let net_model = NetModel::new(
        50_000,   // 50 microseconds base latency
        25_000,   // ±25 microseconds jitter
        0.001,    // 0.1% packet drop rate
        0.005,    // 0.5% reorder rate
    );

    // Configure market maker parameters
    let mm_config = MarketMakerConfig {
        target_spread: price_utils::from_f64(0.02),  // 2 cent spread
        max_inventory: 500,                          // Max 500 shares inventory
        order_size: 100,                            // 100 share orders
        mm_probability: 0.8,                        // 80% chance of market making
        inventory_skew: 0.002,                      // 0.2% price skew per inventory unit
    };

    // Configure order generation
    let order_config = OrderGenerationConfig {
        market_order_prob: 0.3,                    // 30% market orders
        mean_order_interval_ns: 500_000,           // 0.5ms between orders
        min_order_size: 10,
        max_order_size: 200,
        price_range_fraction: 0.015,               // ±1.5% price range
    };

    // Create simulator with configurations
    let mut simulator = Simulator::with_seed(engine, 12345)
        .with_network_model(net_model)
        .with_market_maker_config(mm_config)
        .with_order_generation_config(order_config);

    println!("Simulator configured:");
    println!("- Mode: {:?}", SimulationMode::Synthetic);
    println!("- Network latency: 50μs ±25μs");
    println!("- Target spread: 2 cents");
    println!("- Max inventory: ±500 shares");
    println!("- Order interval: ~0.5ms\n");

    // Run initial simulation steps to establish market
    println!("Establishing initial market...");
    let initial_trades = simulator.run_steps(50)?;
    println!("Generated {} trades during market establishment\n", initial_trades.len());

    // Display initial market state
    let snapshot = simulator.snapshot();
    println!("Initial Market State:");
    println!("- Best Bid: {:?}", snapshot.best_bid.map(price_utils::format));
    println!("- Best Ask: {:?}", snapshot.best_ask.map(price_utils::format));
    println!("- Spread: {:?}", snapshot.spread.map(|s| format!("{:.4}", s as f64 / 10000.0)));
    println!("- Mid Price: {:?}", snapshot.mid.map(|m| format!("{:.4}", m)));
    println!("- Bid Levels: {}", snapshot.bids.len());
    println!("- Ask Levels: {}", snapshot.asks.len());
    println!();

    // Run simulation and track metrics
    println!("Running simulation for 100 steps...");
    let mut total_trades = 0;
    let mut step_count = 0;

    for i in 0..10 {
        let trades = simulator.run_steps(10)?;
        total_trades += trades.len();
        step_count += 10;

        let snapshot = simulator.snapshot();
        let metrics = simulator.get_metrics();

        println!("Step {}: {} trades, Inventory: {}, Cash: {:.2}, PnL: {:.2}",
            step_count,
            trades.len(),
            metrics.inventory,
            metrics.cash_f64(),
            metrics.pnl_f64()
        );

        // Show market state every few iterations
        if i % 3 == 0 {
            println!("  Market: Bid={:?} Ask={:?} Spread={:?}",
                snapshot.best_bid.map(price_utils::format),
                snapshot.best_ask.map(price_utils::format),
                snapshot.spread.map(|s| format!("{:.4}", s as f64 / 10000.0))
            );
        }
    }

    println!("\n=== Final Simulation Results ===");
    let final_snapshot = simulator.snapshot();
    let final_metrics = simulator.get_metrics();

    println!("Total trades executed: {}", total_trades);
    println!("Final inventory: {} shares", final_metrics.inventory);
    println!("Final cash position: ${:.2}", final_metrics.cash_f64());
    println!("Final PnL: ${:.2}", final_metrics.pnl_f64());
    println!();

    println!("Final Market State:");
    println!("- Best Bid: {:?}", final_snapshot.best_bid.map(price_utils::format));
    println!("- Best Ask: {:?}", final_snapshot.best_ask.map(price_utils::format));
    println!("- Spread: {:?}", final_snapshot.spread.map(|s| format!("{:.4}", s as f64 / 10000.0)));
    println!("- Mid Price: {:?}", final_snapshot.mid.map(|m| format!("{:.4}", m)));
    println!("- Total bid levels: {}", final_snapshot.bids.len());
    println!("- Total ask levels: {}", final_snapshot.asks.len());
    println!("- Spread history entries: {}", final_snapshot.recent_spreads.len());

    // Show some recent spreads
    if !final_snapshot.recent_spreads.is_empty() {
        println!("\nRecent spread history (last 5 entries):");
        let recent = final_snapshot.recent_spreads.iter().rev().take(5);
        for (ts, spread) in recent {
            println!("  Time: {}, Spread: {:.4}", 
                ts, 
                *spread as f64 / 10000.0
            );
        }
    }

    // Show order book depth
    println!("\nOrder Book Depth (top 3 levels each side):");
    println!("Bids:");
    for (i, level) in final_snapshot.bids.iter().take(3).enumerate() {
        println!("  {}: {} @ {} (latency: {}ms)", 
            i + 1, 
            level.qty, 
            price_utils::format(level.price),
            level.latency_ms
        );
    }
    println!("Asks:");
    for (i, level) in final_snapshot.asks.iter().take(3).enumerate() {
        println!("  {}: {} @ {} (latency: {}ms)", 
            i + 1, 
            level.qty, 
            price_utils::format(level.price),
            level.latency_ms
        );
    }

    println!("\n=== Simulation Complete ===");
    Ok(())
}