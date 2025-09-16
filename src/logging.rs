use tracing::{info, warn, error, debug, trace};
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use crate::error::EngineError;
use std::time::{SystemTime, UNIX_EPOCH};

/// Initialize the logging system with appropriate filters and formatting
pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Create a filter that respects RUST_LOG environment variable
    // Default to "info" level if not set
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Set up the subscriber with JSON formatting for structured logging
    match tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .compact()
        )
        .try_init() {
        Ok(_) => {
            info!("Logging system initialized");
            Ok(())
        }
        Err(e) => {
            // Check if the error is due to already being initialized
            if e.to_string().contains("a global default trace dispatcher has already been set") {
                // Logging is already initialized, this is fine
                Ok(())
            } else {
                // Some other error occurred
                Err(Box::new(e))
            }
        }
    }
}

/// Initialize logging with custom configuration for testing
pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("debug")
        .try_init();
}

/// Log an engine error with appropriate severity level
pub fn log_engine_error(error: &EngineError, context: Option<&str>) {
    let level = error.severity().to_tracing_level();
    let message = if let Some(ctx) = context {
        format!("{}: {}", ctx, error)
    } else {
        error.to_string()
    };

    match level {
        tracing::Level::INFO => info!("{}", message),
        tracing::Level::WARN => warn!("{}", message),
        tracing::Level::ERROR => error!("{}", message),
        _ => error!("{}", message),
    }
}

/// Log order book operations for audit trail
pub fn log_order_operation(operation: &str, order_id: u64, details: Option<&str>) {
    if let Some(details) = details {
        info!(
            operation = operation,
            order_id = order_id,
            details = details,
            "Order operation executed"
        );
    } else {
        info!(
            operation = operation,
            order_id = order_id,
            "Order operation executed"
        );
    }
}

/// Log trade execution for audit trail
pub fn log_trade(maker_id: u64, taker_id: u64, price: u64, qty: u64, timestamp: u128) {
    info!(
        maker_id = maker_id,
        taker_id = taker_id,
        price = price,
        qty = qty,
        timestamp = timestamp,
        "Trade executed"
    );
}

/// Log system startup information
pub fn log_startup(component: &str, config: Option<&str>) {
    if let Some(config) = config {
        info!(
            component = component,
            config = config,
            "Component started"
        );
    } else {
        info!(
            component = component,
            "Component started"
        );
    }
}

/// Log performance metrics
pub fn log_performance_metric(metric_name: &str, value: f64, unit: &str) {
    info!(
        metric = metric_name,
        value = value,
        unit = unit,
        "Performance metric"
    );
}

/// Log WebSocket connection events
pub fn log_websocket_event(event: &str, client_id: Option<&str>, details: Option<&str>) {
    match (client_id, details) {
        (Some(id), Some(details)) => {
            info!(
                event = event,
                client_id = id,
                details = details,
                "WebSocket event"
            );
        }
        (Some(id), None) => {
            info!(
                event = event,
                client_id = id,
                "WebSocket event"
            );
        }
        (None, Some(details)) => {
            info!(
                event = event,
                details = details,
                "WebSocket event"
            );
        }
        (None, None) => {
            info!(
                event = event,
                "WebSocket event"
            );
        }
    }
}

/// Log system health metrics
pub fn log_health_metric(metric_name: &str, value: f64, threshold: Option<f64>, status: &str) {
    if let Some(threshold) = threshold {
        info!(
            metric = metric_name,
            value = value,
            threshold = threshold,
            status = status,
            "Health metric"
        );
    } else {
        info!(
            metric = metric_name,
            value = value,
            status = status,
            "Health metric"
        );
    }
}

/// Log connection pool status
pub fn log_connection_status(active_connections: usize, max_connections: Option<usize>) {
    if let Some(max) = max_connections {
        info!(
            active_connections = active_connections,
            max_connections = max,
            utilization = (active_connections as f64 / max as f64) * 100.0,
            "Connection pool status"
        );
    } else {
        info!(
            active_connections = active_connections,
            "Connection pool status"
        );
    }
}

/// Log simulation step performance
pub fn log_simulation_step(step_duration_ms: f64, trades_generated: usize, orders_processed: usize) {
    debug!(
        step_duration_ms = step_duration_ms,
        trades_generated = trades_generated,
        orders_processed = orders_processed,
        "Simulation step completed"
    );
}

/// Log order book state changes
pub fn log_order_book_state(best_bid: Option<u64>, best_ask: Option<u64>, spread: Option<i64>, total_orders: usize) {
    trace!(
        best_bid = best_bid,
        best_ask = best_ask,
        spread = spread,
        total_orders = total_orders,
        "Order book state"
    );
}

/// Log data ingestion events
pub fn log_data_ingestion(source: &str, events_processed: usize, errors: usize, duration_ms: f64) {
    info!(
        source = source,
        events_processed = events_processed,
        errors = errors,
        duration_ms = duration_ms,
        processing_rate = events_processed as f64 / (duration_ms / 1000.0),
        "Data ingestion completed"
    );
}

/// Log critical system errors that require immediate attention
pub fn log_critical_error(component: &str, error: &str, context: Option<&str>) {
    if let Some(ctx) = context {
        error!(
            component = component,
            error = error,
            context = ctx,
            severity = "CRITICAL",
            "Critical system error - immediate attention required"
        );
    } else {
        error!(
            component = component,
            error = error,
            severity = "CRITICAL",
            "Critical system error - immediate attention required"
        );
    }
}

/// Log system recovery events
pub fn log_recovery_event(component: &str, action: &str, success: bool, duration_ms: Option<f64>) {
    if let Some(duration) = duration_ms {
        if success {
            info!(
                component = component,
                action = action,
                success = success,
                duration_ms = duration,
                "System recovery completed"
            );
        } else {
            warn!(
                component = component,
                action = action,
                success = success,
                duration_ms = duration,
                "System recovery failed"
            );
        }
    } else {
        if success {
            info!(
                component = component,
                action = action,
                success = success,
                "System recovery completed"
            );
        } else {
            warn!(
                component = component,
                action = action,
                success = success,
                "System recovery failed"
            );
        }
    }
}

/// Get current timestamp for logging
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorSeverity;


    #[test]
    fn test_logging_initialization() {
        // This test just ensures the logging functions don't panic
        init_test_logging();
        
        let error = EngineError::UnknownOrder { order_id: 123 };
        log_engine_error(&error, Some("Test context"));
        
        log_order_operation("PLACE", 123, Some("Limit order"));
        log_trade(1, 2, 10000, 100, 1234567890);
        log_startup("OrderBook", Some("FIFO"));
        log_performance_metric("latency", 1.5, "ms");
        log_websocket_event("connect", Some("client-123"), None);
    }

    #[test]
    fn test_error_severity_mapping() {
        let info_error = EngineError::reject("Test");
        assert_eq!(info_error.severity(), ErrorSeverity::Info);
        
        let critical_error = EngineError::internal("System failure");
        assert_eq!(critical_error.severity(), ErrorSeverity::Critical);
    }
}