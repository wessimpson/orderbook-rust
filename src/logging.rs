use tracing::{info, warn, error};
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use crate::error::EngineError;

/// Initialize the logging system with appropriate filters and formatting
pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Create a filter that respects RUST_LOG environment variable
    // Default to "info" level if not set
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Set up the subscriber with JSON formatting for structured logging
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .compact()
        )
        .try_init()?;

    info!("Logging system initialized");
    Ok(())
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