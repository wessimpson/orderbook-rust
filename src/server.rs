use crate::engine::DepthSnapshot;
use crate::sim::{Simulator, SimulationMode};
use crate::queue_fifo::FifoLevel;
use crate::engine::OrderBook;
use crate::error::{EngineResult, EngineError};
use crate::logging::{
    init_logging, log_websocket_event, log_engine_error, log_startup, 
    log_health_metric, log_connection_status, log_simulation_step,
    log_critical_error, log_recovery_event, current_timestamp
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde_json;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{interval, Duration};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

/// Application state shared between handlers
#[derive(Clone)]
pub struct AppState {
    /// Broadcast channel for sending snapshots to all connected clients
    pub snapshot_tx: broadcast::Sender<DepthSnapshot>,
    /// The market simulator wrapped in Arc<Mutex<>> for thread-safe access
    pub simulator: Arc<Mutex<Simulator<OrderBook<FifoLevel>>>>,
    /// System health metrics
    pub health_metrics: Arc<Mutex<SystemHealthMetrics>>,
}

/// System health monitoring metrics
#[derive(Debug, Clone)]
pub struct SystemHealthMetrics {
    pub server_start_time: u64,
    pub total_connections: u64,
    pub active_connections: usize,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub total_errors: u64,
    pub last_error_time: Option<u64>,
    pub simulation_steps: u64,
    pub total_trades: u64,
    pub avg_step_duration_ms: f64,
}

impl SystemHealthMetrics {
    pub fn new() -> Self {
        Self {
            server_start_time: current_timestamp(),
            total_connections: 0,
            active_connections: 0,
            total_messages_sent: 0,
            total_messages_received: 0,
            total_errors: 0,
            last_error_time: None,
            simulation_steps: 0,
            total_trades: 0,
            avg_step_duration_ms: 0.0,
        }
    }

    pub fn record_connection(&mut self) {
        self.total_connections += 1;
        self.active_connections += 1;
        log_connection_status(self.active_connections, Some(1000)); // Assume max 1000 connections
    }

    pub fn record_disconnection(&mut self) {
        self.active_connections = self.active_connections.saturating_sub(1);
        log_connection_status(self.active_connections, Some(1000));
    }

    pub fn record_message_sent(&mut self) {
        self.total_messages_sent += 1;
    }

    pub fn record_message_received(&mut self) {
        self.total_messages_received += 1;
    }

    pub fn record_error(&mut self) {
        self.total_errors += 1;
        self.last_error_time = Some(current_timestamp());
    }

    pub fn record_simulation_step(&mut self, duration_ms: f64, trades: usize) {
        self.simulation_steps += 1;
        self.total_trades += trades as u64;
        
        // Update rolling average of step duration
        let alpha = 0.1; // Exponential smoothing factor
        self.avg_step_duration_ms = alpha * duration_ms + (1.0 - alpha) * self.avg_step_duration_ms;
        
        log_simulation_step(duration_ms, trades, 1);
    }

    pub fn uptime_seconds(&self) -> u64 {
        (current_timestamp() - self.server_start_time) / 1000
    }
}

impl AppState {
    /// Create new application state with a simulator
    pub fn new(mut simulator: Simulator<OrderBook<FifoLevel>>) -> Self {
        let (snapshot_tx, _) = broadcast::channel(100); // Buffer up to 100 snapshots
        
        // Ensure simulator is in synthetic mode to avoid DataSource issues
        simulator.set_mode(SimulationMode::Synthetic);
        
        log_startup("AppState", Some("Initialized with synthetic simulation mode"));
        
        Self {
            snapshot_tx,
            simulator: Arc::new(Mutex::new(simulator)),
            health_metrics: Arc::new(Mutex::new(SystemHealthMetrics::new())),
        }
    }

    /// Get a receiver for snapshot broadcasts
    pub fn subscribe(&self) -> broadcast::Receiver<DepthSnapshot> {
        self.snapshot_tx.subscribe()
    }

    /// Get the number of active WebSocket connections
    pub fn active_connections(&self) -> usize {
        self.snapshot_tx.receiver_count()
    }

    /// Broadcast a snapshot to all connected clients
    pub async fn broadcast_snapshot(&self, snapshot: DepthSnapshot) {
        match self.snapshot_tx.send(snapshot) {
            Ok(receiver_count) => {
                if receiver_count > 0 {
                    tracing::debug!("Broadcast snapshot to {} clients", receiver_count);
                    // Update health metrics
                    {
                        let mut metrics = self.health_metrics.lock().await;
                        metrics.record_message_sent();
                    }
                }
                // If receiver_count is 0, no clients are connected - this is normal
            }
            Err(tokio::sync::broadcast::error::SendError(_)) => {
                // This only happens if all receivers have been dropped, which is normal
                // when no WebSocket clients are connected. We can safely ignore this.
                tracing::trace!("No WebSocket clients connected to receive snapshot");
            }
        }
    }

    /// Get current system health metrics
    pub async fn get_health_metrics(&self) -> SystemHealthMetrics {
        self.health_metrics.lock().await.clone()
    }

    /// Record an error in the system
    pub async fn record_error(&self, error: &EngineError, context: &str) {
        {
            let mut metrics = self.health_metrics.lock().await;
            metrics.record_error();
        }
        log_engine_error(error, Some(context));
    }
}

/// WebSocket handler for client connections
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    let active_connections = state.active_connections();
    
    // Record new connection in health metrics
    {
        let mut metrics = state.health_metrics.lock().await;
        metrics.record_connection();
    }
    
    log_websocket_event("connection_request", None, Some(&format!("Total connections will be: {}", active_connections + 1)));
    
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Handle individual WebSocket connection
async fn handle_websocket(socket: WebSocket, state: AppState) {
    let connection_id = format!("conn_{}", current_timestamp());
    log_websocket_event("connection_established", Some(&connection_id), None);
    
    let (mut sender, mut receiver) = socket.split();
    let mut snapshot_rx = state.subscribe();

    // Spawn task to handle incoming messages from client
    let state_clone = state.clone();
    let conn_id_clone = connection_id.clone();
    let incoming_task = tokio::spawn(async move {
        let mut message_count = 0;
        
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    message_count += 1;
                    
                    // Record message received
                    {
                        let mut metrics = state_clone.health_metrics.lock().await;
                        metrics.record_message_received();
                    }
                    
                    log_websocket_event("message_received", Some(&conn_id_clone), Some(&format!("Message #{}: {}", message_count, text)));
                    
                    // Handle client messages with proper error handling
                    if let Err(e) = handle_client_message(&text, &state_clone).await {
                        let error_msg = format!("Error handling client message: {}", e);
                        log_websocket_event("message_error", Some(&conn_id_clone), Some(&error_msg));
                        state_clone.record_error(&e, "WebSocket message handling").await;
                        
                        // Note: In a production system, you'd send error responses back to client
                        // via a channel to communicate with the outgoing task
                        let _error_response = serde_json::json!({
                            "type": "error",
                            "message": error_msg,
                            "timestamp": current_timestamp()
                        });
                    }
                }
                Ok(Message::Close(close_frame)) => {
                    let reason = close_frame.map(|f| f.reason.to_string()).unwrap_or_else(|| "No reason provided".to_string());
                    log_websocket_event("connection_close_requested", Some(&conn_id_clone), Some(&reason));
                    break;
                }
                Ok(Message::Ping(data)) => {
                    log_websocket_event("ping_received", Some(&conn_id_clone), Some(&format!("Ping data length: {}", data.len())));
                }
                Ok(Message::Pong(data)) => {
                    log_websocket_event("pong_received", Some(&conn_id_clone), Some(&format!("Pong data length: {}", data.len())));
                }
                Ok(Message::Binary(data)) => {
                    log_websocket_event("binary_message", Some(&conn_id_clone), Some(&format!("Binary data length: {}", data.len())));
                }
                Err(e) => {
                    let error_msg = format!("WebSocket protocol error: {}", e);
                    log_websocket_event("protocol_error", Some(&conn_id_clone), Some(&error_msg));
                    
                    // Record error in health metrics
                    {
                        let mut metrics = state_clone.health_metrics.lock().await;
                        metrics.record_error();
                    }
                    
                    break;
                }
            }
        }
        
        log_websocket_event("incoming_handler_completed", Some(&conn_id_clone), Some(&format!("Processed {} messages", message_count)));
    });

    // Handle outgoing messages to client
    let conn_id_clone2 = connection_id.clone();
    let state_clone2 = state.clone();
    let outgoing_task = tokio::spawn(async move {
        let mut snapshots_sent = 0;
        
        while let Ok(snapshot) = snapshot_rx.recv().await {
            match serde_json::to_string(&snapshot) {
                Ok(json) => {
                    match sender.send(Message::Text(json)).await {
                        Ok(_) => {
                            snapshots_sent += 1;
                            if snapshots_sent % 100 == 0 {
                                log_websocket_event("snapshots_milestone", Some(&conn_id_clone2), Some(&format!("Sent {} snapshots", snapshots_sent)));
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Failed to send snapshot to client: {}", e);
                            log_websocket_event("send_error", Some(&conn_id_clone2), Some(&error_msg));
                            
                            // Record error in health metrics
                            {
                                let mut metrics = state_clone2.health_metrics.lock().await;
                                metrics.record_error();
                            }
                            
                            break;
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to serialize snapshot: {}", e);
                    log_websocket_event("serialization_error", Some(&conn_id_clone2), Some(&error_msg));
                    
                    // Record error in health metrics
                    {
                        let mut metrics = state_clone2.health_metrics.lock().await;
                        metrics.record_error();
                    }
                    
                    // Continue trying to send other snapshots
                }
            }
        }
        
        log_websocket_event("outgoing_handler_completed", Some(&conn_id_clone2), Some(&format!("Sent {} snapshots", snapshots_sent)));
    });

    // Wait for either task to complete (connection closed or error)
    tokio::select! {
        _ = incoming_task => {
            log_websocket_event("incoming_task_finished", Some(&connection_id), None);
        }
        _ = outgoing_task => {
            log_websocket_event("outgoing_task_finished", Some(&connection_id), None);
        }
    }

    // Record disconnection in health metrics
    {
        let mut metrics = state.health_metrics.lock().await;
        metrics.record_disconnection();
    }

    let remaining_connections = state.active_connections();
    log_websocket_event("connection_closed", Some(&connection_id), Some(&format!("Remaining connections: {}", remaining_connections)));
}

/// Handle messages received from clients
async fn handle_client_message(message: &str, state: &AppState) -> EngineResult<()> {
    // Validate message is not empty
    if message.trim().is_empty() {
        return Err(EngineError::reject("Empty message received"));
    }

    // Validate message length (prevent DoS attacks)
    if message.len() > 10_000 {
        return Err(EngineError::reject("Message too large"));
    }

    // Try to parse as JSON for structured commands
    match serde_json::from_str::<serde_json::Value>(message) {
        Ok(json) => {
            handle_structured_message(&json, state).await
        }
        Err(_) => {
            // Handle as plain text command
            handle_text_command(message, state).await
        }
    }
}

/// Handle structured JSON messages from clients
async fn handle_structured_message(json: &serde_json::Value, state: &AppState) -> EngineResult<()> {
    let command = json.get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EngineError::reject("Missing 'command' field in JSON message"))?;

    match command {
        "get_health" => {
            let metrics = state.get_health_metrics().await;
            info!("Health check requested - Uptime: {}s, Active connections: {}, Total errors: {}", 
                  metrics.uptime_seconds(), metrics.active_connections, metrics.total_errors);
            Ok(())
        }
        "reset_metrics" => {
            // Reset simulation metrics (requires proper authorization in production)
            let mut simulator = state.simulator.lock().await;
            simulator.reset_metrics();
            info!("Simulation metrics reset by client request");
            Ok(())
        }
        "set_simulation_speed" => {
            let speed = json.get("speed")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| EngineError::reject("Missing or invalid 'speed' field"))?;
            
            if speed <= 0.0 || speed > 100.0 {
                return Err(EngineError::reject("Speed must be between 0.0 and 100.0"));
            }
            
            // Note: This would require implementing speed control in the simulator
            info!("Simulation speed change requested: {}x", speed);
            Ok(())
        }
        "place_test_order" => {
            // Handle test order placement (for debugging/testing)
            handle_test_order_placement(json, state).await
        }
        _ => {
            Err(EngineError::reject(format!("Unknown command: {}", command)))
        }
    }
}

/// Handle plain text commands from clients
async fn handle_text_command(message: &str, _state: &AppState) -> EngineResult<()> {
    let command = message.trim().to_lowercase();
    
    match command.as_str() {
        "ping" => {
            info!("Ping command received from client");
            Ok(())
        }
        "status" => {
            info!("Status command received from client");
            Ok(())
        }
        "help" => {
            info!("Help command received from client");
            Ok(())
        }
        _ => {
            info!("Unknown text command received: {}", message);
            Ok(()) // Don't error on unknown text commands, just log them
        }
    }
}

/// Handle test order placement from clients
async fn handle_test_order_placement(json: &serde_json::Value, state: &AppState) -> EngineResult<()> {
    use crate::types::{Order, OrderType, Side};
    use crate::time::now_ns;
    
    // Extract order parameters
    let side_str = json.get("side")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EngineError::reject("Missing 'side' field"))?;
    
    let side = match side_str.to_lowercase().as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => return Err(EngineError::reject("Invalid side, must be 'buy' or 'sell'")),
    };
    
    let qty = json.get("qty")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| EngineError::reject("Missing or invalid 'qty' field"))?;
    
    if qty == 0 {
        return Err(EngineError::InvalidQty { qty });
    }
    
    let order_type = if let Some(price_val) = json.get("price") {
        let price = price_val.as_u64()
            .ok_or_else(|| EngineError::reject("Invalid 'price' field"))?;
        
        if price == 0 {
            return Err(EngineError::InvalidPrice { price });
        }
        
        OrderType::Limit { price }
    } else {
        OrderType::Market
    };
    
    // Generate order ID
    let order_id = (now_ns() % 1_000_000) as u64; // Simple ID generation for testing
    
    let order = Order {
        id: order_id,
        side,
        qty,
        order_type,
        ts: now_ns(),
    };
    
    // Place the order
    let mut simulator = state.simulator.lock().await;
    match simulator.place_order(order) {
        Ok(trades) => {
            info!("Test order {} placed successfully, generated {} trades", order_id, trades.len());
            Ok(())
        }
        Err(e) => {
            warn!("Test order {} failed: {}", order_id, e);
            Err(e)
        }
    }
}

/// Health check endpoint with detailed system status
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = state.get_health_metrics().await;
    
    // Determine health status based on metrics
    let status = if metrics.total_errors > 100 {
        "DEGRADED"
    } else if metrics.active_connections > 900 {
        "OVERLOADED"
    } else {
        "HEALTHY"
    };
    
    let health_response = serde_json::json!({
        "status": status,
        "timestamp": current_timestamp(),
        "uptime_seconds": metrics.uptime_seconds(),
        "active_connections": metrics.active_connections,
        "total_connections": metrics.total_connections,
        "total_messages_sent": metrics.total_messages_sent,
        "total_messages_received": metrics.total_messages_received,
        "total_errors": metrics.total_errors,
        "last_error_time": metrics.last_error_time,
        "simulation_steps": metrics.simulation_steps,
        "total_trades": metrics.total_trades,
        "avg_step_duration_ms": metrics.avg_step_duration_ms,
        "version": env!("CARGO_PKG_VERSION")
    });
    
    log_health_metric("system_status", metrics.total_errors as f64, Some(100.0), status);
    
    let status_code = match status {
        "HEALTHY" => StatusCode::OK,
        "DEGRADED" => StatusCode::OK, // Still operational
        "OVERLOADED" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    
    (status_code, serde_json::to_string(&health_response).unwrap_or_else(|_| "{}".to_string()))
}

/// Create the Axum router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(health_check))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive()) // Allow CORS for frontend
        )
        .with_state(state)
}

/// Start the simulation loop that periodically generates snapshots
pub async fn start_simulation_loop(state: AppState, interval_ms: u64) {
    let mut interval = interval(Duration::from_millis(interval_ms));
    let mut consecutive_errors = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;
    
    log_startup("SimulationLoop", Some(&format!("Starting with {}ms interval", interval_ms)));
    
    loop {
        let step_start = std::time::Instant::now();
        interval.tick().await;
        
        // Run one simulation step and generate snapshot
        let step_result = {
            let mut simulator = state.simulator.lock().await;
            simulator.step()
        };
        
        match step_result {
            Ok(trades) => {
                consecutive_errors = 0; // Reset error counter on success
                let step_duration = step_start.elapsed().as_millis() as f64;
                
                // Update health metrics
                {
                    let mut metrics = state.health_metrics.lock().await;
                    metrics.record_simulation_step(step_duration, trades.len());
                }
                
                let active_connections = state.active_connections();
                if !trades.is_empty() {
                    if active_connections > 0 {
                        tracing::debug!("Simulation step generated {} trades (broadcasting to {} clients)", 
                                      trades.len(), active_connections);
                    } else {
                        tracing::trace!("Simulation step generated {} trades (no clients connected)", 
                                       trades.len());
                    }
                }
                
                // Log performance warning if step takes too long
                if step_duration > interval_ms as f64 * 0.8 {
                    warn!("Simulation step took {}ms, approaching interval limit of {}ms", 
                          step_duration, interval_ms);
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                state.record_error(&e, "Simulation step").await;
                
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    log_critical_error("SimulationLoop", 
                                     &format!("Too many consecutive errors ({}): {}", consecutive_errors, e),
                                     Some("Simulation may be unstable"));
                    
                    // Attempt recovery
                    log_recovery_event("SimulationLoop", "reset_simulator", false, None);
                    
                    // In a production system, you might want to restart the simulator
                    // or implement other recovery mechanisms
                    
                    // Reset error counter to prevent spam
                    consecutive_errors = 0;
                }
                
                // Continue with next iteration after error
                continue;
            }
        }
        
        // Generate and broadcast snapshot
        let snapshot = {
            let simulator = state.simulator.lock().await;
            simulator.snapshot()
        };
        
        state.broadcast_snapshot(snapshot).await;
        
        // Periodic health logging (every 100 steps)
        {
            let metrics = state.health_metrics.lock().await;
            if metrics.simulation_steps % 100 == 0 && metrics.simulation_steps > 0 {
                log_health_metric("simulation_performance", 
                                metrics.avg_step_duration_ms, 
                                Some(interval_ms as f64 * 0.5), 
                                if metrics.avg_step_duration_ms < interval_ms as f64 * 0.5 { "GOOD" } else { "SLOW" });
            }
        }
    }
}

/// Start the WebSocket server
pub async fn start_server(
    simulator: Simulator<OrderBook<FifoLevel>>,
    port: u16,
    simulation_interval_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging first
    match init_logging() {
        Ok(_) => log_startup("Logging", Some("Successfully initialized")),
        Err(e) => {
            eprintln!("Failed to initialize logging: {}", e);
            return Err(e);
        }
    }
    
    log_startup("OrderBookServer", Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    info!("Initializing Order Book WebSocket Server on port {}", port);
    
    // Validate configuration
    if port == 0 {
        let error = "Invalid port number: 0";
        log_critical_error("ServerStartup", error, None);
        return Err(error.into());
    }
    
    if simulation_interval_ms == 0 {
        let error = "Invalid simulation interval: 0ms";
        log_critical_error("ServerStartup", error, None);
        return Err(error.into());
    }
    
    if simulation_interval_ms > 10000 {
        warn!("Large simulation interval ({}ms) may result in poor user experience", simulation_interval_ms);
    }
    
    // Create application state
    let state = AppState::new(simulator);
    log_startup("AppState", Some("Application state initialized"));
    
    // Create router
    let app = create_router(state.clone());
    log_startup("Router", Some("HTTP router configured"));
    
    // Start simulation loop in background
    let simulation_state = state.clone();
    let simulation_handle = tokio::spawn(async move {
        start_simulation_loop(simulation_state, simulation_interval_ms).await;
    });
    
    log_startup("SimulationLoop", Some(&format!("Background task started with {}ms interval", simulation_interval_ms)));
    
    // Start server
    let addr = format!("0.0.0.0:{}", port);
    
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            log_startup("TcpListener", Some(&format!("Bound to {}", addr)));
            listener
        }
        Err(e) => {
            let error_msg = format!("Failed to bind to {}: {}", addr, e);
            log_critical_error("ServerStartup", &error_msg, None);
            return Err(Box::new(e));
        }
    };
    
    // Log all endpoints
    info!("🚀 Order Book Server is ready!");
    info!("📡 WebSocket endpoint: ws://localhost:{}/ws", port);
    info!("🏥 Health check endpoint: http://localhost:{}/health", port);
    info!("⚡ Simulation interval: {}ms", simulation_interval_ms);
    info!("📊 Logging level: {}", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
    
    // Start serving requests
    let server_result = axum::serve(listener, app).await;
    
    // If we reach here, the server has stopped
    simulation_handle.abort(); // Stop the simulation loop
    
    match server_result {
        Ok(_) => {
            info!("Server shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Server error: {}", e);
            log_critical_error("ServerRuntime", &error_msg, None);
            Err(Box::new(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::OrderBook;
    use crate::queue_fifo::FifoLevel;
    use crate::sim::Simulator;
    use tokio::time::Duration;

    type TestOrderBook = OrderBook<FifoLevel>;

    #[tokio::test]
    async fn test_app_state_creation() {
        let engine = TestOrderBook::new();
        let simulator = Simulator::new(engine);
        let state = AppState::new(simulator);
        
        // Test that we can subscribe to snapshots
        let _rx = state.subscribe();
        
        // Test that simulator is accessible
        {
            let sim = state.simulator.lock().await;
            let snapshot = sim.snapshot();
            assert!(snapshot.ts > 0);
        }
    }

    #[tokio::test]
    async fn test_snapshot_broadcasting() {
        let engine = TestOrderBook::new();
        let simulator = Simulator::new(engine);
        let state = AppState::new(simulator);
        
        let mut rx = state.subscribe();
        
        // Generate a snapshot
        let snapshot = {
            let sim = state.simulator.lock().await;
            sim.snapshot()
        };
        
        // Broadcast it
        state.broadcast_snapshot(snapshot.clone()).await;
        
        // Receive it
        let received = rx.recv().await.unwrap();
        assert_eq!(received.ts, snapshot.ts);
    }

    #[tokio::test]
    async fn test_simulation_loop_step() {
        let engine = TestOrderBook::new();
        let simulator = Simulator::new(engine);
        let state = AppState::new(simulator);
        
        let mut rx = state.subscribe();
        
        // Start simulation loop for a short time
        let simulation_state = state.clone();
        let simulation_task = tokio::spawn(async move {
            start_simulation_loop(simulation_state, 10).await;
        });
        
        // Wait for a few snapshots
        let mut snapshots_received = 0;
        let timeout = tokio::time::timeout(Duration::from_millis(100), async {
            while snapshots_received < 3 {
                if rx.recv().await.is_ok() {
                    snapshots_received += 1;
                }
            }
        });
        
        let _ = timeout.await;
        simulation_task.abort();
        
        assert!(snapshots_received > 0, "Should have received at least one snapshot");
    }

    #[tokio::test]
    async fn test_router_creation() {
        let engine = TestOrderBook::new();
        let simulator = Simulator::new(engine);
        let state = AppState::new(simulator);
        
        let _router = create_router(state);
        // If this compiles and runs without panic, the router is created successfully
    }

    #[tokio::test]
    async fn test_handle_client_message() {
        let engine = TestOrderBook::new();
        let simulator = Simulator::new(engine);
        let state = AppState::new(simulator);
        
        let result = handle_client_message("test message", &state).await;
        assert!(result.is_ok());
    }
}