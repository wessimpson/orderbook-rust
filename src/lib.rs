pub mod types;
pub mod time;
pub mod error;
pub mod logging;
pub mod queue;
pub mod queue_fifo;
pub mod engine;
pub mod data;
pub mod sim;
pub mod server;
pub mod config;

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

// Re-export queue discipline trait and implementations
pub use queue::QueueDiscipline;
pub use queue_fifo::FifoLevel;

// Re-export engine types and traits
pub use engine::{OrderBookEngine, OrderBook, DepthSnapshot, BookLevelPoint};

// Re-export data ingestion types and traits
pub use data::{DataSource, MarketEvent, MarketStatusType, DataError, DataResult, DataSourceMetadata};

// Re-export simulation types and traits
pub use sim::{Simulator, NetModel, SimulationMode, MarketMakerConfig, OrderGenerationConfig};

// Re-export server types and functions
pub use server::{AppState, start_server, create_router, start_simulation_loop};

// Re-export configuration types
pub use config::{Config, ServerConfig, SimulationConfig, DataSourceConfig, LoggingConfig, ConfigError};
