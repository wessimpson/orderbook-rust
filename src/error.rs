use thiserror::Error;
use crate::types::{OrderId, Price, Qty};

/// Comprehensive error types for the order book engine
#[derive(Error, Debug, Clone, PartialEq)]
pub enum EngineError {
    /// Order not found for cancellation or modification
    #[error("Order with ID {order_id} not found")]
    UnknownOrder { order_id: OrderId },

    /// Invalid price value (e.g., zero or negative for limit orders)
    #[error("Invalid price: {price}. Price must be positive for limit orders")]
    InvalidPrice { price: Price },

    /// Invalid quantity value (e.g., zero)
    #[error("Invalid quantity: {qty}. Quantity must be positive")]
    InvalidQty { qty: Qty },

    /// Business logic rejection with custom reason
    #[error("Order rejected: {reason}")]
    Reject { reason: String },

    /// Market order cannot be placed when no opposite side exists
    #[error("Market order cannot be executed: no liquidity on opposite side")]
    NoLiquidity,

    /// Order would cross with own order (self-trade prevention)
    #[error("Self-trade detected for order {order_id}")]
    SelfTrade { order_id: OrderId },

    /// Order size exceeds maximum allowed
    #[error("Order quantity {qty} exceeds maximum allowed {max_qty}")]
    QtyTooLarge { qty: Qty, max_qty: Qty },

    /// Price is outside allowed range
    #[error("Price {price} is outside allowed range [{min_price}, {max_price}]")]
    PriceOutOfRange {
        price: Price,
        min_price: Price,
        max_price: Price,
    },

    /// Order book is in an invalid state
    #[error("Order book internal error: {details}")]
    InternalError { details: String },

    /// Data ingestion errors
    #[error("Data ingestion error: {message}")]
    DataError { message: String },

    /// Network or communication errors
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// Serialization/deserialization errors
    #[error("Serialization error: {message}")]
    SerializationError { message: String },
}

/// Result type alias for engine operations
pub type EngineResult<T> = Result<T, EngineError>;

impl EngineError {
    /// Create a rejection error with a custom reason
    pub fn reject<S: Into<String>>(reason: S) -> Self {
        Self::Reject {
            reason: reason.into(),
        }
    }

    /// Create an internal error with details
    pub fn internal<S: Into<String>>(details: S) -> Self {
        Self::InternalError {
            details: details.into(),
        }
    }

    /// Create a data error with message
    pub fn data<S: Into<String>>(message: S) -> Self {
        Self::DataError {
            message: message.into(),
        }
    }

    /// Create a network error with message
    pub fn network<S: Into<String>>(message: S) -> Self {
        Self::NetworkError {
            message: message.into(),
        }
    }

    /// Create a serialization error with message
    pub fn serialization<S: Into<String>>(message: S) -> Self {
        Self::SerializationError {
            message: message.into(),
        }
    }

    /// Check if this is a recoverable error
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::UnknownOrder { .. } => true,
            Self::InvalidPrice { .. } => false,
            Self::InvalidQty { .. } => false,
            Self::Reject { .. } => true,
            Self::NoLiquidity => true,
            Self::SelfTrade { .. } => true,
            Self::QtyTooLarge { .. } => false,
            Self::PriceOutOfRange { .. } => false,
            Self::InternalError { .. } => false,
            Self::DataError { .. } => true,
            Self::NetworkError { .. } => true,
            Self::SerializationError { .. } => false,
        }
    }

    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::UnknownOrder { .. } => ErrorSeverity::Warning,
            Self::InvalidPrice { .. } => ErrorSeverity::Error,
            Self::InvalidQty { .. } => ErrorSeverity::Error,
            Self::Reject { .. } => ErrorSeverity::Info,
            Self::NoLiquidity => ErrorSeverity::Warning,
            Self::SelfTrade { .. } => ErrorSeverity::Warning,
            Self::QtyTooLarge { .. } => ErrorSeverity::Error,
            Self::PriceOutOfRange { .. } => ErrorSeverity::Error,
            Self::InternalError { .. } => ErrorSeverity::Critical,
            Self::DataError { .. } => ErrorSeverity::Warning,
            Self::NetworkError { .. } => ErrorSeverity::Warning,
            Self::SerializationError { .. } => ErrorSeverity::Error,
        }
    }
}

/// Error severity levels for logging and monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl ErrorSeverity {
    /// Convert to tracing level
    pub fn to_tracing_level(&self) -> tracing::Level {
        match self {
            Self::Info => tracing::Level::INFO,
            Self::Warning => tracing::Level::WARN,
            Self::Error => tracing::Level::ERROR,
            Self::Critical => tracing::Level::ERROR,
        }
    }
}

// Conversion from standard library errors
impl From<serde_json::Error> for EngineError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError {
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(err: std::io::Error) -> Self {
        Self::NetworkError {
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = EngineError::UnknownOrder { order_id: 123 };
        assert_eq!(err.to_string(), "Order with ID 123 not found");
        assert!(err.is_recoverable());
        assert_eq!(err.severity(), ErrorSeverity::Warning);

        let err = EngineError::InvalidPrice { price: 0 };
        assert!(err.to_string().contains("Invalid price: 0"));
        assert!(!err.is_recoverable());
        assert_eq!(err.severity(), ErrorSeverity::Error);
    }

    #[test]
    fn test_error_helpers() {
        let err = EngineError::reject("Test rejection");
        assert_eq!(err.to_string(), "Order rejected: Test rejection");

        let err = EngineError::internal("Internal issue");
        assert_eq!(err.to_string(), "Order book internal error: Internal issue");
        assert_eq!(err.severity(), ErrorSeverity::Critical);
    }

    #[test]
    fn test_error_conversions() {
        let json_err = serde_json::from_str::<i32>("invalid json");
        assert!(json_err.is_err());
        let engine_err: EngineError = json_err.unwrap_err().into();
        assert!(matches!(engine_err, EngineError::SerializationError { .. }));
    }

    #[test]
    fn test_severity_levels() {
        assert_eq!(
            ErrorSeverity::Info.to_tracing_level(),
            tracing::Level::INFO
        );
        assert_eq!(
            ErrorSeverity::Warning.to_tracing_level(),
            tracing::Level::WARN
        );
        assert_eq!(
            ErrorSeverity::Error.to_tracing_level(),
            tracing::Level::ERROR
        );
        assert_eq!(
            ErrorSeverity::Critical.to_tracing_level(),
            tracing::Level::ERROR
        );
    }
}