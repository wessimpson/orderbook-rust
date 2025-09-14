use crate::engine::DepthSnapshot;
use crate::sim::{Simulator, SimulationMode};
use crate::queue_fifo::FifoLevel;
use crate::engine::OrderBook;
use crate::error::EngineResult;
use crate::logging::init_logging;

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
use tracing::{error, info, warn};

/// Application state shared between handlers
#[derive(Clone)]
pub struct AppState {
    /// Broadcast channel for sending snapshots to all connected clients
    pub snapshot_tx: broadcast::Sender<DepthSnapshot>,
    /// The market simulator wrapped in Arc<Mutex<>> for thread-safe access
    pub simulator: Arc<Mutex<Simulator<OrderBook<FifoLevel>>>>,
}

impl AppState {
    /// Create new application state with a simulator
    pub fn new(mut simulator: Simulator<OrderBook<FifoLevel>>) -> Self {
        let (snapshot_tx, _) = broadcast::channel(100); // Buffer up to 100 snapshots
        
        // Ensure simulator is in synthetic mode to avoid DataSource issues
        simulator.set_mode(SimulationMode::Synthetic);
        
        Self {
            snapshot_tx,
            simulator: Arc::new(Mutex::new(simulator)),
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
}

/// WebSocket handler for client connections
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    let active_connections = state.active_connections();
    info!("New WebSocket connection established (total connections will be: {})", active_connections + 1);
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Handle individual WebSocket connection
async fn handle_websocket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut snapshot_rx = state.subscribe();

    // Spawn task to handle incoming messages from client
    let state_clone = state.clone();
    let incoming_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    info!("Received message from client: {}", text);
                    // Handle client messages (e.g., configuration changes, commands)
                    if let Err(e) = handle_client_message(&text, &state_clone).await {
                        warn!("Error handling client message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Client closed WebSocket connection");
                    break;
                }
                Ok(_) => {
                    // Handle other message types (binary, ping, pong) if needed
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });

    // Handle outgoing messages to client
    let outgoing_task = tokio::spawn(async move {
        while let Ok(snapshot) = snapshot_rx.recv().await {
            match serde_json::to_string(&snapshot) {
                Ok(json) => {
                    if let Err(e) = sender.send(Message::Text(json)).await {
                        error!("Failed to send snapshot to client: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize snapshot: {}", e);
                }
            }
        }
    });

    // Wait for either task to complete (connection closed or error)
    tokio::select! {
        _ = incoming_task => {
            info!("Incoming message handler completed");
        }
        _ = outgoing_task => {
            info!("Outgoing message handler completed");
        }
    }

    let remaining_connections = state.active_connections();
    info!("WebSocket connection closed (remaining connections: {})", remaining_connections);
}

/// Handle messages received from clients
async fn handle_client_message(message: &str, _state: &AppState) -> EngineResult<()> {
    // Parse client message and handle different command types
    // For now, just log the message
    info!("Processing client message: {}", message);
    
    // Future implementation could handle:
    // - Simulation parameter changes
    // - Data source switching
    // - Playback controls (play/pause/speed)
    // - Manual order placement for testing
    
    Ok(())
}

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "Order Book Server is running")
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
    
    info!("Starting simulation loop with {}ms interval", interval_ms);
    
    loop {
        interval.tick().await;
        
        // Run one simulation step and generate snapshot
        {
            let mut simulator = state.simulator.lock().await;
            match simulator.step() {
                Ok(trades) => {
                    let active_connections = state.active_connections();
                    if !trades.is_empty() {
                        if active_connections > 0 {
                            info!("Simulation step generated {} trades (broadcasting to {} clients)", 
                                  trades.len(), active_connections);
                        } else {
                            tracing::debug!("Simulation step generated {} trades (no clients connected)", 
                                           trades.len());
                        }
                    }
                }
                Err(e) => {
                    error!("Simulation step failed: {}", e);
                    continue;
                }
            }
            
            // Generate and broadcast snapshot while holding the lock
            let snapshot = simulator.snapshot();
            drop(simulator); // Release lock before broadcasting
            state.broadcast_snapshot(snapshot).await;
        }
    }
}

/// Start the WebSocket server
pub async fn start_server(
    simulator: Simulator<OrderBook<FifoLevel>>,
    port: u16,
    simulation_interval_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let _ = init_logging();
    
    info!("Initializing Order Book WebSocket Server");
    
    // Create application state
    let state = AppState::new(simulator);
    
    // Create router
    let app = create_router(state.clone());
    
    // Start simulation loop in background
    let simulation_state = state.clone();
    tokio::spawn(async move {
        start_simulation_loop(simulation_state, simulation_interval_ms).await;
    });
    
    // Start server
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    info!("WebSocket server listening on {}", addr);
    info!("WebSocket endpoint: ws://localhost:{}/ws", port);
    info!("Health check endpoint: http://localhost:{}/health", port);
    
    axum::serve(listener, app).await?;
    
    Ok(())
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