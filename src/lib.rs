pub mod types;
pub mod time;
pub mod error;
pub mod logging;

// Re-export core types for convenience
pub use types::{Order, OrderId, OrderType, Price, Qty, Side, Trade};

// Re-export price utilities
pub use types::price_utils;

// Re-export error types
pub use error::{EngineError, EngineResult, ErrorSeverity};

// Re-export time utilities
pub use time::{now_ns, ms_to_ns, ns_to_ms, ns_to_secs, secs_to_ns, elapsed_ns, format_ns};

// Re-export logging functions
pub use logging::{init_logging, init_test_logging, log_engine_error, log_order_operation, log_trade};
