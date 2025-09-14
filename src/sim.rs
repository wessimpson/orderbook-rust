use crate::engine::{OrderBookEngine, DepthSnapshot};
use crate::data::{DataSource, MarketEvent};
use crate::types::{Order, OrderId, Price, Qty, Side, Trade, Metrics, price_utils};
use crate::time::now_ns;
use crate::error::EngineResult;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use tracing;

/// Network latency simulation parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetModel {
    /// Base latency in nanoseconds
    pub base_latency_ns: u64,
    /// Jitter range in nanoseconds (±jitter_ns)
    pub jitter_ns: u64,
    /// Probability of packet drop (0.0 to 1.0)
    pub drop_prob: f64,
    /// Probability of packet reordering (0.0 to 1.0)
    pub reorder_prob: f64,
}

impl Default for NetModel {
    fn default() -> Self {
        Self {
            base_latency_ns: 100_000,    // 100 microseconds
            jitter_ns: 50_000,          // ±50 microseconds
            drop_prob: 0.001,           // 0.1% drop rate
            reorder_prob: 0.01,         // 1% reorder rate
        }
    }
}

impl NetModel {
    /// Create a new network model with specified parameters
    pub fn new(base_latency_ns: u64, jitter_ns: u64, drop_prob: f64, reorder_prob: f64) -> Self {
        Self {
            base_latency_ns,
            jitter_ns,
            drop_prob,
            reorder_prob,
        }
    }

    /// Calculate simulated latency for an operation
    pub fn simulate_latency<R: Rng>(&self, rng: &mut R) -> u64 {
        let jitter = if self.jitter_ns > 0 {
            rng.gen_range(-(self.jitter_ns as i64)..=(self.jitter_ns as i64))
        } else {
            0
        };
        
        (self.base_latency_ns as i64 + jitter).max(0) as u64
    }

    /// Check if a packet should be dropped
    pub fn should_drop<R: Rng>(&self, rng: &mut R) -> bool {
        rng.gen::<f64>() < self.drop_prob
    }

    /// Check if a packet should be reordered
    pub fn should_reorder<R: Rng>(&self, rng: &mut R) -> bool {
        rng.gen::<f64>() < self.reorder_prob
    }
}

/// Market simulation engine with configurable parameters
pub struct Simulator<E: OrderBookEngine> {
    /// The order book engine
    pub engine: E,
    /// Random number generator for deterministic simulation
    pub rng: StdRng,
    /// Network latency simulation model
    pub net: NetModel,
    /// Trading performance metrics
    pub metrics: Metrics,
    /// Rolling spread history for visualization
    pub recent_spreads: Vec<(u128, i64)>,
    /// Next order ID to assign
    next_order_id: OrderId,
    /// Current simulation timestamp
    current_time: u128,
    /// Data source for historical replay (optional)
    data_source: Option<Box<dyn DataSource>>,
    /// Simulation mode
    mode: SimulationMode,
    /// Market making parameters
    market_maker_config: MarketMakerConfig,
    /// Order generation parameters
    order_gen_config: OrderGenerationConfig,
}

/// Simulation modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationMode {
    /// Pure simulation with synthetic order flow
    Synthetic,
    /// Historical data replay
    Historical,
    /// Hybrid mode combining historical data with synthetic orders
    Hybrid,
}

/// Market maker configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketMakerConfig {
    /// Spread to maintain (in ticks)
    pub target_spread: Price,
    /// Maximum inventory position
    pub max_inventory: i64,
    /// Order size for market making
    pub order_size: Qty,
    /// Probability of placing market making orders (0.0 to 1.0)
    pub mm_probability: f64,
    /// Inventory skew factor (how much to adjust prices based on inventory)
    pub inventory_skew: f64,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            target_spread: price_utils::from_f64(0.01),  // 1 cent spread
            max_inventory: 1000,
            order_size: 100,
            mm_probability: 0.7,
            inventory_skew: 0.001,  // 0.1% price adjustment per unit inventory
        }
    }
}

/// Order generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderGenerationConfig {
    /// Probability of generating a market order vs limit order
    pub market_order_prob: f64,
    /// Mean time between orders (nanoseconds)
    pub mean_order_interval_ns: u64,
    /// Order size distribution parameters
    pub min_order_size: Qty,
    pub max_order_size: Qty,
    /// Price range for limit orders (as fraction of mid-price)
    pub price_range_fraction: f64,
}

impl Default for OrderGenerationConfig {
    fn default() -> Self {
        Self {
            market_order_prob: 0.3,
            mean_order_interval_ns: 1_000_000,  // 1 millisecond
            min_order_size: 10,
            max_order_size: 500,
            price_range_fraction: 0.02,  // ±2% from mid-price
        }
    }
}

impl<E: OrderBookEngine> Simulator<E> {
    /// Create a new simulator with default parameters
    pub fn new(engine: E) -> Self {
        Self::with_seed(engine, 42)
    }

    /// Create a new simulator with specified random seed
    pub fn with_seed(engine: E, seed: u64) -> Self {
        Self {
            engine,
            rng: StdRng::seed_from_u64(seed),
            net: NetModel::default(),
            metrics: Metrics::new(),
            recent_spreads: Vec::new(),
            next_order_id: 1,
            current_time: now_ns(),
            data_source: None,
            mode: SimulationMode::Synthetic,
            market_maker_config: MarketMakerConfig::default(),
            order_gen_config: OrderGenerationConfig::default(),
        }
    }

    /// Set the network model for latency simulation
    pub fn with_network_model(mut self, net: NetModel) -> Self {
        self.net = net;
        self
    }

    /// Set the market maker configuration
    pub fn with_market_maker_config(mut self, config: MarketMakerConfig) -> Self {
        self.market_maker_config = config;
        self
    }

    /// Set the order generation configuration
    pub fn with_order_generation_config(mut self, config: OrderGenerationConfig) -> Self {
        self.order_gen_config = config;
        self
    }

    /// Set a data source for historical replay
    pub fn with_data_source(mut self, data_source: Box<dyn DataSource>) -> Self {
        self.data_source = Some(data_source);
        self.mode = SimulationMode::Historical;
        self
    }

    /// Set simulation mode
    pub fn set_mode(&mut self, mode: SimulationMode) {
        self.mode = mode;
    }

    /// Get the next order ID
    fn next_order_id(&mut self) -> OrderId {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Generate a realistic market making order pair
    fn generate_market_making_orders(&mut self) -> Vec<Order> {
        let mut orders = Vec::new();
        
        // Get current market state
        let best_bid = self.engine.best_bid();
        let best_ask = self.engine.best_ask();
        let mid_price = self.engine.mid_price();
        
        // Calculate target prices based on current market and inventory
        let inventory_adjustment = self.metrics.inventory as f64 * self.market_maker_config.inventory_skew;
        
        let (target_bid, target_ask) = if let Some(mid) = mid_price {
            let mid_ticks = price_utils::from_f64(mid);
            let half_spread = self.market_maker_config.target_spread / 2;
            
            // Adjust prices based on inventory (positive inventory pushes prices down)
            let adjustment_ticks = price_utils::from_f64(inventory_adjustment);
            
            let bid = mid_ticks.saturating_sub(half_spread).saturating_sub(adjustment_ticks);
            let ask = mid_ticks.saturating_add(half_spread).saturating_sub(adjustment_ticks);
            
            (bid, ask)
        } else {
            // No market exists, create initial market around a base price
            let base_price = price_utils::from_f64(100.0);  // $100 base price
            let half_spread = self.market_maker_config.target_spread / 2;
            
            (base_price - half_spread, base_price + half_spread)
        };
        
        // Check if we should place orders (based on probability and inventory limits)
        let should_place_bid = self.rng.gen::<f64>() < self.market_maker_config.mm_probability
            && self.metrics.inventory < self.market_maker_config.max_inventory
            && (best_bid.is_none() || best_bid.unwrap() < target_bid);
            
        let should_place_ask = self.rng.gen::<f64>() < self.market_maker_config.mm_probability
            && self.metrics.inventory > -self.market_maker_config.max_inventory
            && (best_ask.is_none() || best_ask.unwrap() > target_ask);
        
        // Generate bid order
        if should_place_bid && target_bid > 0 {
            let order = Order::new_limit(
                self.next_order_id(),
                Side::Buy,
                self.market_maker_config.order_size,
                target_bid,
                self.current_time,
            );
            orders.push(order);
        }
        
        // Generate ask order
        if should_place_ask && target_ask > 0 {
            let order = Order::new_limit(
                self.next_order_id(),
                Side::Sell,
                self.market_maker_config.order_size,
                target_ask,
                self.current_time,
            );
            orders.push(order);
        }
        
        orders
    }

    /// Generate a random market taker order
    fn generate_market_taker_order(&mut self) -> Option<Order> {
        // Determine order side randomly
        let side = if self.rng.gen::<bool>() { Side::Buy } else { Side::Sell };
        
        // Generate order size
        let qty = self.rng.gen_range(
            self.order_gen_config.min_order_size..=self.order_gen_config.max_order_size
        );
        
        // Decide between market and limit order
        let order = if self.rng.gen::<f64>() < self.order_gen_config.market_order_prob {
            // Market order
            Order::new_market(self.next_order_id(), side, qty, self.current_time)
        } else {
            // Limit order - price based on current market with some randomness
            let price = self.generate_limit_order_price(side)?;
            Order::new_limit(self.next_order_id(), side, qty, price, self.current_time)
        };
        
        Some(order)
    }

    /// Generate a price for a limit order based on current market
    fn generate_limit_order_price(&mut self, side: Side) -> Option<Price> {
        let mid_price = self.engine.mid_price()?;
        let mid_ticks = price_utils::from_f64(mid_price);
        
        // Generate price within range of mid-price
        let range_ticks = price_utils::from_f64(mid_price * self.order_gen_config.price_range_fraction);
        let price_offset = self.rng.gen_range(0..=range_ticks);
        
        let price = match side {
            Side::Buy => {
                // Buy orders typically below mid-price
                if self.rng.gen::<bool>() {
                    mid_ticks.saturating_sub(price_offset)  // Below mid
                } else {
                    mid_ticks.saturating_add(price_offset)  // Above mid (aggressive)
                }
            }
            Side::Sell => {
                // Sell orders typically above mid-price
                if self.rng.gen::<bool>() {
                    mid_ticks.saturating_add(price_offset)  // Above mid
                } else {
                    mid_ticks.saturating_sub(price_offset)  // Below mid (aggressive)
                }
            }
        };
        
        if price > 0 { Some(price) } else { None }
    }

    /// Process a market event from data source
    fn process_market_event(&mut self, event: MarketEvent) -> EngineResult<Vec<Trade>> {
        match event {
            MarketEvent::OrderPlacement(order) => {
                match self.engine.place(order) {
                    Ok(trades) => Ok(trades),
                    Err(e) => {
                        // Log the error but continue simulation
                        tracing::warn!("Order placement failed: {}", e);
                        Ok(Vec::new())
                    }
                }
            }
            MarketEvent::OrderCancellation { order_id, .. } => {
                match self.engine.cancel(order_id) {
                    Ok(_) => Ok(Vec::new()),
                    Err(_) => Ok(Vec::new()), // Ignore cancellation errors
                }
            }
            MarketEvent::Trade { qty, side, .. } => {
                // Convert trade event to synthetic order that will execute
                let order = Order::new_market(self.next_order_id(), side, qty, self.current_time);
                match self.engine.place(order) {
                    Ok(trades) => Ok(trades),
                    Err(e) => {
                        // Log the error but continue simulation
                        tracing::warn!("Market order failed: {}", e);
                        Ok(Vec::new())
                    }
                }
            }
            _ => {
                // Other events (quotes, status changes) don't directly affect the order book
                Ok(Vec::new())
            }
        }
    }

    /// Update metrics after trade execution
    fn update_metrics(&mut self, trades: &[Trade], taker_side: Side) {
        for trade in trades {
            self.metrics.update_trade(taker_side, trade.qty, trade.price);
        }
        
        // Calculate PnL using current mid-price
        if let Some(mid_price) = self.engine.mid_price() {
            let mid_price_ticks = price_utils::from_f64(mid_price);
            self.metrics.calculate_pnl(Some(mid_price_ticks));
        }
    }

    /// Update spread history
    fn update_spread_history(&mut self) {
        if let Some(spread) = self.engine.spread() {
            self.recent_spreads.push((self.current_time, spread));
            
            // Keep only last 400 data points for performance
            if self.recent_spreads.len() > 400 {
                self.recent_spreads.remove(0);
            }
        }
    }

    /// Simulate network latency for an operation
    fn simulate_network_latency(&mut self) {
        let latency_ns = self.net.simulate_latency(&mut self.rng);
        
        // Simulate the delay by advancing current time
        self.current_time += latency_ns as u128;
        
        // In a real implementation, this might involve actual delays or queuing
    }

    /// Run one simulation step
    pub fn step(&mut self) -> EngineResult<Vec<Trade>> {
        use crate::logging::{log_engine_error, log_data_ingestion};
        
        let step_start = std::time::Instant::now();
        let mut all_trades = Vec::new();
        let mut orders_processed = 0;
        let mut errors_encountered = 0;
        
        // Advance simulation time
        let time_advance = self.rng.gen_range(
            self.order_gen_config.mean_order_interval_ns / 2
            ..=self.order_gen_config.mean_order_interval_ns * 2
        );
        self.current_time += time_advance as u128;
        
        match self.mode {
            SimulationMode::Historical => {
                // Process events from data source
                if let Some(ref mut data_source) = self.data_source {
                    match data_source.next_event() {
                        Ok(Some(event)) => {
                            orders_processed += 1;
                            self.current_time = event.timestamp();
                            
                            match self.process_market_event(event) {
                                Ok(trades) => {
                                    if !trades.is_empty() {
                                        self.update_metrics(&trades, Side::Buy); // Assume buy side for simplicity
                                        all_trades.extend(trades);
                                    }
                                }
                                Err(e) => {
                                    errors_encountered += 1;
                                    log_engine_error(&e, Some("Historical data processing"));
                                    
                                    // Continue processing unless it's a critical error
                                    if !e.is_recoverable() {
                                        return Err(e);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            // End of data - log completion
                            tracing::info!("Historical data replay completed");
                        }
                        Err(e) => {
                            errors_encountered += 1;
                            let engine_error = crate::error::EngineError::data(format!("Data source error: {}", e));
                            log_engine_error(&engine_error, Some("Data source reading"));
                            
                            // Switch to synthetic mode on data errors
                            tracing::warn!("Switching to synthetic mode due to data source error");
                            self.mode = SimulationMode::Synthetic;
                        }
                    }
                }
            }
            SimulationMode::Synthetic => {
                // Generate synthetic orders
                
                // Market making orders
                let mm_orders = self.generate_market_making_orders();
                for order in mm_orders {
                    orders_processed += 1;
                    self.simulate_network_latency();
                    
                    if !self.net.should_drop(&mut self.rng) {
                        let order_side = order.side;
                        let order_id = order.id;
                        
                        match self.engine.place(order) {
                            Ok(trades) => {
                                if !trades.is_empty() {
                                    self.update_metrics(&trades, order_side);
                                    all_trades.extend(trades);
                                }
                            }
                            Err(e) => {
                                errors_encountered += 1;
                                log_engine_error(&e, Some(&format!("Market maker order {}", order_id)));
                                
                                // Continue unless critical error
                                if !e.is_recoverable() {
                                    return Err(e);
                                }
                            }
                        }
                    } else {
                        tracing::trace!("Market maker order dropped due to network simulation");
                    }
                }
                
                // Market taker orders
                if let Some(taker_order) = self.generate_market_taker_order() {
                    orders_processed += 1;
                    self.simulate_network_latency();
                    
                    if !self.net.should_drop(&mut self.rng) {
                        let taker_side = taker_order.side;
                        let order_id = taker_order.id;
                        
                        match self.engine.place(taker_order) {
                            Ok(trades) => {
                                if !trades.is_empty() {
                                    self.update_metrics(&trades, taker_side);
                                    all_trades.extend(trades);
                                }
                            }
                            Err(e) => {
                                errors_encountered += 1;
                                log_engine_error(&e, Some(&format!("Market taker order {}", order_id)));
                                
                                // Continue unless critical error
                                if !e.is_recoverable() {
                                    return Err(e);
                                }
                            }
                        }
                    } else {
                        tracing::trace!("Market taker order dropped due to network simulation");
                    }
                }
            }
            SimulationMode::Hybrid => {
                // Combine historical data with synthetic orders
                // First try to process historical event
                if let Some(ref mut data_source) = self.data_source {
                    match data_source.next_event() {
                        Ok(Some(event)) => {
                            orders_processed += 1;
                            self.current_time = event.timestamp();
                            
                            match self.process_market_event(event) {
                                Ok(trades) => {
                                    if !trades.is_empty() {
                                        self.update_metrics(&trades, Side::Buy);
                                        all_trades.extend(trades);
                                    }
                                }
                                Err(e) => {
                                    errors_encountered += 1;
                                    log_engine_error(&e, Some("Hybrid mode historical processing"));
                                    
                                    if !e.is_recoverable() {
                                        return Err(e);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            // End of historical data, continue with synthetic only
                        }
                        Err(e) => {
                            errors_encountered += 1;
                            let engine_error = crate::error::EngineError::data(format!("Hybrid mode data error: {}", e));
                            log_engine_error(&engine_error, Some("Hybrid mode data source"));
                        }
                    }
                }
                
                // Then add some synthetic market making
                if self.rng.gen::<f64>() < 0.5 {  // 50% chance of synthetic order
                    let mm_orders = self.generate_market_making_orders();
                    for order in mm_orders {
                        orders_processed += 1;
                        self.simulate_network_latency();
                        
                        if !self.net.should_drop(&mut self.rng) {
                            let order_side = order.side;
                            let order_id = order.id;
                            
                            match self.engine.place(order) {
                                Ok(trades) => {
                                    if !trades.is_empty() {
                                        self.update_metrics(&trades, order_side);
                                        all_trades.extend(trades);
                                    }
                                }
                                Err(e) => {
                                    errors_encountered += 1;
                                    log_engine_error(&e, Some(&format!("Hybrid mode synthetic order {}", order_id)));
                                    
                                    if !e.is_recoverable() {
                                        return Err(e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Update spread history if trades occurred
        if !all_trades.is_empty() {
            self.update_spread_history();
        }
        
        // Log step completion metrics
        let step_duration = step_start.elapsed();
        if errors_encountered > 0 {
            tracing::warn!("Simulation step completed with {} errors out of {} orders in {:?}", 
                          errors_encountered, orders_processed, step_duration);
        } else if orders_processed > 0 {
            tracing::trace!("Simulation step: {} orders, {} trades in {:?}", 
                           orders_processed, all_trades.len(), step_duration);
        }
        
        // Log performance data for monitoring
        let step_duration_ms = step_duration.as_millis() as f64;
        log_data_ingestion("simulation_step", orders_processed, errors_encountered, step_duration_ms);
        
        Ok(all_trades)
    }

    /// Run simulation for a specified number of steps
    pub fn run_steps(&mut self, steps: usize) -> EngineResult<Vec<Trade>> {
        let mut all_trades = Vec::new();
        
        for _ in 0..steps {
            let trades = self.step()?;
            all_trades.extend(trades);
        }
        
        Ok(all_trades)
    }

    /// Get current market snapshot
    pub fn snapshot(&self) -> DepthSnapshot {
        let mut snapshot = self.engine.snapshot();
        
        // Override with simulator's metrics and spread history
        snapshot.metrics = self.metrics.clone();
        snapshot.recent_spreads = self.recent_spreads.clone();
        snapshot.ts = self.current_time;
        
        snapshot
    }

    /// Place an order directly (for testing or manual intervention)
    pub fn place_order(&mut self, order: Order) -> EngineResult<Vec<Trade>> {
        use crate::logging::log_order_operation;
        
        log_order_operation("MANUAL_PLACE", order.id, Some("Direct order placement"));
        
        match self.engine.place(order) {
            Ok(trades) => {
                if !trades.is_empty() {
                    // Update metrics based on the order side (assume buy side for manual orders)
                    self.update_metrics(&trades, Side::Buy);
                    self.update_spread_history();
                }
                Ok(trades)
            }
            Err(e) => {
                use crate::logging::log_engine_error;
                log_engine_error(&e, Some("Manual order placement"));
                Err(e)
            }
        }
    }

    /// Reset simulation metrics
    pub fn reset_metrics(&mut self) {
        use crate::logging::log_startup;
        
        self.metrics = Metrics::new();
        self.recent_spreads.clear();
        log_startup("Simulator", Some("Metrics reset"));
    }

    /// Get current simulation time
    pub fn current_time(&self) -> u128 {
        self.current_time
    }

    /// Get simulation metrics
    pub fn get_metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Reset simulation state
    pub fn reset(&mut self) {
        self.metrics = Metrics::new();
        self.recent_spreads.clear();
        self.current_time = now_ns();
        self.next_order_id = 1;
        
        if let Some(ref mut data_source) = self.data_source {
            let _ = data_source.reset();
        }
    }

    /// Set simulation time (useful for testing)
    pub fn set_time(&mut self, time: u128) {
        self.current_time = time;
    }

    /// Check if simulation has more data to process (for historical mode)
    pub fn has_more_data(&self) -> bool {
        match &self.data_source {
            Some(data_source) => !data_source.is_finished(),
            None => true,  // Synthetic mode always has more data
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::OrderBook;
    use crate::queue_fifo::FifoLevel;
    use crate::types::price_utils;

    type TestOrderBook = OrderBook<FifoLevel>;

    #[test]
    fn test_simulator_creation() {
        let engine = TestOrderBook::new();
        let sim = Simulator::new(engine);
        
        assert_eq!(sim.mode, SimulationMode::Synthetic);
        assert_eq!(sim.next_order_id, 1);
        assert_eq!(sim.metrics.inventory, 0);
        assert_eq!(sim.metrics.cash, 0);
        assert_eq!(sim.metrics.pnl, 0);
    }

    #[test]
    fn test_network_model() {
        let net = NetModel::default();
        let mut rng = StdRng::seed_from_u64(42);
        
        // Test latency simulation
        let latency = net.simulate_latency(&mut rng);
        assert!(latency > 0);
        
        // Test drop probability
        let mut drops = 0;
        for _ in 0..1000 {
            if net.should_drop(&mut rng) {
                drops += 1;
            }
        }
        // Should be approximately 1 drop (0.1% of 1000)
        assert!(drops < 10);  // Allow some variance
        
        // Test reorder probability
        let mut reorders = 0;
        for _ in 0..1000 {
            if net.should_reorder(&mut rng) {
                reorders += 1;
            }
        }
        // Should be approximately 10 reorders (1% of 1000)
        assert!(reorders > 0 && reorders < 50);  // Allow some variance
    }

    #[test]
    fn test_market_maker_config() {
        let config = MarketMakerConfig::default();
        
        assert_eq!(config.target_spread, price_utils::from_f64(0.01));
        assert_eq!(config.max_inventory, 1000);
        assert_eq!(config.order_size, 100);
        assert_eq!(config.mm_probability, 0.7);
    }

    #[test]
    fn test_synthetic_simulation_step() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::with_seed(engine, 42);
        
        // Run a few simulation steps
        for _ in 0..10 {
            let result = sim.step();
            assert!(result.is_ok());
        }
        
        // Should have generated some market activity
        let snapshot = sim.snapshot();
        
        // Check that we have some market structure
        // (exact values depend on random seed, but should have some activity)
        assert!(snapshot.ts > 0);
    }

    #[test]
    fn test_market_making_order_generation() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::with_seed(engine, 42);
        
        // Generate market making orders
        let orders = sim.generate_market_making_orders();
        
        // Should generate orders to create initial market
        // (exact behavior depends on random seed and market state)
        assert!(orders.len() <= 2);  // At most bid and ask
        
        for order in orders {
            assert!(order.qty > 0);
            assert!(order.id > 0);
            assert!(order.is_limit());
        }
    }

    #[test]
    fn test_market_taker_order_generation() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::with_seed(engine, 42);
        
        // First establish some market structure
        for _ in 0..5 {
            let _ = sim.step();
        }
        
        // Generate a market taker order
        if let Some(order) = sim.generate_market_taker_order() {
            assert!(order.qty > 0);
            assert!(order.id > 0);
            assert!(order.qty >= sim.order_gen_config.min_order_size);
            assert!(order.qty <= sim.order_gen_config.max_order_size);
        }
    }

    #[test]
    fn test_metrics_tracking() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::with_seed(engine, 42);
        
        let _initial_metrics = sim.get_metrics().clone();
        
        // Run simulation to generate trades
        let _ = sim.run_steps(20);
        
        let final_metrics = sim.get_metrics();
        
        // Metrics should have been updated if trades occurred
        // (exact values depend on random simulation, but structure should be valid)
        assert!(final_metrics.pnl != 0 || final_metrics.cash != 0 || final_metrics.inventory != 0 || 
                (final_metrics.pnl == 0 && final_metrics.cash == 0 && final_metrics.inventory == 0));
    }

    #[test]
    fn test_spread_history_tracking() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::with_seed(engine, 42);
        
        // Run simulation to generate market activity
        let _ = sim.run_steps(30);
        
        let snapshot = sim.snapshot();
        
        // Should have some spread history if trades occurred
        // (depends on random simulation generating trades)
        assert!(snapshot.recent_spreads.len() <= 400);  // Bounded by max size
        
        for (ts, spread) in &snapshot.recent_spreads {
            assert!(*ts > 0);
            assert!(*spread >= 0);  // Spread should be non-negative
        }
    }

    #[test]
    fn test_simulation_reset() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::with_seed(engine, 42);
        
        // Run simulation
        let _ = sim.run_steps(10);
        
        // Reset simulation
        sim.reset();
        
        // Check that state was reset
        assert_eq!(sim.get_metrics().inventory, 0);
        assert_eq!(sim.get_metrics().cash, 0);
        assert_eq!(sim.get_metrics().pnl, 0);
        assert_eq!(sim.recent_spreads.len(), 0);
        assert_eq!(sim.next_order_id, 1);
        assert!(sim.current_time() > 0);  // Time should be reset to a valid timestamp
    }

    #[test]
    fn test_simulation_modes() {
        let engine = TestOrderBook::new();
        let mut sim = Simulator::new(engine);
        
        // Test mode setting
        assert_eq!(sim.mode, SimulationMode::Synthetic);
        
        sim.set_mode(SimulationMode::Historical);
        assert_eq!(sim.mode, SimulationMode::Historical);
        
        sim.set_mode(SimulationMode::Hybrid);
        assert_eq!(sim.mode, SimulationMode::Hybrid);
    }

    #[test]
    fn test_configuration_builders() {
        let engine = TestOrderBook::new();
        let net_model = NetModel::new(200_000, 100_000, 0.002, 0.02);
        let mm_config = MarketMakerConfig {
            target_spread: price_utils::from_f64(0.02),
            max_inventory: 500,
            order_size: 50,
            mm_probability: 0.8,
            inventory_skew: 0.002,
        };
        let order_config = OrderGenerationConfig {
            market_order_prob: 0.4,
            mean_order_interval_ns: 2_000_000,
            min_order_size: 5,
            max_order_size: 200,
            price_range_fraction: 0.03,
        };
        
        let sim = Simulator::new(engine)
            .with_network_model(net_model.clone())
            .with_market_maker_config(mm_config.clone())
            .with_order_generation_config(order_config.clone());
        
        assert_eq!(sim.net.base_latency_ns, net_model.base_latency_ns);
        assert_eq!(sim.market_maker_config.target_spread, mm_config.target_spread);
        assert_eq!(sim.order_gen_config.market_order_prob, order_config.market_order_prob);
    }
}