use crate::types::{Order, OrderId, Price, Qty, Side};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during data ingestion
#[derive(Error, Debug, Clone, PartialEq)]
pub enum DataError {
    /// File not found or cannot be opened
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    /// Invalid file format or corrupted data
    #[error("Invalid format in file {file}: {details}")]
    InvalidFormat { file: String, details: String },

    /// Parsing error for specific record
    #[error("Parse error at line {line} in {file}: {message}")]
    ParseError {
        file: String,
        line: usize,
        message: String,
    },

    /// Timestamp out of order or invalid
    #[error("Invalid timestamp {timestamp} at line {line}: {reason}")]
    InvalidTimestamp {
        timestamp: u128,
        line: usize,
        reason: String,
    },

    /// Seek operation failed
    #[error("Seek failed: {reason}")]
    SeekFailed { reason: String },

    /// End of data stream reached
    #[error("End of data stream")]
    EndOfStream,

    /// IO error during file operations
    #[error("IO error: {message}")]
    IoError { message: String },

    /// Data validation error
    #[error("Validation error: {message}")]
    ValidationError { message: String },

    /// Unsupported operation
    #[error("Unsupported operation: {operation}")]
    UnsupportedOperation { operation: String },
}

/// Result type for data operations
pub type DataResult<T> = Result<T, DataError>;

impl DataError {
    /// Create a file not found error
    pub fn file_not_found<S: Into<String>>(path: S) -> Self {
        Self::FileNotFound { path: path.into() }
    }

    /// Create an invalid format error
    pub fn invalid_format<S1: Into<String>, S2: Into<String>>(file: S1, details: S2) -> Self {
        Self::InvalidFormat {
            file: file.into(),
            details: details.into(),
        }
    }

    /// Create a parse error
    pub fn parse_error<S1: Into<String>, S2: Into<String>>(
        file: S1,
        line: usize,
        message: S2,
    ) -> Self {
        Self::ParseError {
            file: file.into(),
            line,
            message: message.into(),
        }
    }

    /// Create an invalid timestamp error
    pub fn invalid_timestamp<S: Into<String>>(
        timestamp: u128,
        line: usize,
        reason: S,
    ) -> Self {
        Self::InvalidTimestamp {
            timestamp,
            line,
            reason: reason.into(),
        }
    }

    /// Create a seek failed error
    pub fn seek_failed<S: Into<String>>(reason: S) -> Self {
        Self::SeekFailed {
            reason: reason.into(),
        }
    }

    /// Create a validation error
    pub fn validation<S: Into<String>>(message: S) -> Self {
        Self::ValidationError {
            message: message.into(),
        }
    }

    /// Create an unsupported operation error
    pub fn unsupported<S: Into<String>>(operation: S) -> Self {
        Self::UnsupportedOperation {
            operation: operation.into(),
        }
    }
}

/// Market events that can be ingested from external data sources
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketEvent {
    /// Trade execution event
    Trade {
        price: Price,
        qty: Qty,
        side: Side,
        timestamp: u128,
        /// Optional trade ID for tracking
        trade_id: Option<String>,
    },

    /// Quote update event (bid/ask prices)
    Quote {
        bid: Option<Price>,
        ask: Option<Price>,
        bid_qty: Option<Qty>,
        ask_qty: Option<Qty>,
        timestamp: u128,
    },

    /// Order placement event
    OrderPlacement(Order),

    /// Order cancellation event
    OrderCancellation {
        order_id: OrderId,
        timestamp: u128,
        /// Optional reason for cancellation
        reason: Option<String>,
    },

    /// Order modification event
    OrderModification {
        order_id: OrderId,
        new_qty: Option<Qty>,
        new_price: Option<Price>,
        timestamp: u128,
    },

    /// Market status change (open, close, halt, etc.)
    MarketStatus {
        status: MarketStatusType,
        timestamp: u128,
        /// Optional message describing the status change
        message: Option<String>,
    },

    /// Best bid/offer update
    BestBidOffer {
        best_bid: Option<Price>,
        best_ask: Option<Price>,
        bid_qty: Option<Qty>,
        ask_qty: Option<Qty>,
        timestamp: u128,
    },
}

/// Market status types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatusType {
    /// Market is open for trading
    Open,
    /// Market is closed
    Closed,
    /// Trading is halted
    Halted,
    /// Pre-market session
    PreMarket,
    /// After-hours session
    AfterHours,
    /// Auction period
    Auction,
}

impl MarketEvent {
    /// Get the timestamp of this event
    pub fn timestamp(&self) -> u128 {
        match self {
            Self::Trade { timestamp, .. } => *timestamp,
            Self::Quote { timestamp, .. } => *timestamp,
            Self::OrderPlacement(order) => order.ts,
            Self::OrderCancellation { timestamp, .. } => *timestamp,
            Self::OrderModification { timestamp, .. } => *timestamp,
            Self::MarketStatus { timestamp, .. } => *timestamp,
            Self::BestBidOffer { timestamp, .. } => *timestamp,
        }
    }

    /// Check if this event affects the order book
    pub fn affects_book(&self) -> bool {
        matches!(
            self,
            Self::OrderPlacement(_)
                | Self::OrderCancellation { .. }
                | Self::OrderModification { .. }
                | Self::Trade { .. }
        )
    }

    /// Check if this is a market data event
    pub fn is_market_data(&self) -> bool {
        matches!(
            self,
            Self::Quote { .. } | Self::BestBidOffer { .. } | Self::MarketStatus { .. }
        )
    }

    /// Validate the event data
    pub fn validate(&self) -> DataResult<()> {
        match self {
            Self::Trade { price, qty, .. } => {
                if *price == 0 {
                    return Err(DataError::validation("Trade price cannot be zero"));
                }
                if *qty == 0 {
                    return Err(DataError::validation("Trade quantity cannot be zero"));
                }
            }
            Self::Quote {
                bid, ask, bid_qty, ask_qty, ..
            } => {
                if let (Some(bid), Some(ask)) = (bid, ask) {
                    if bid >= ask {
                        return Err(DataError::validation("Bid price must be less than ask price"));
                    }
                }
                if let Some(qty) = bid_qty {
                    if *qty == 0 {
                        return Err(DataError::validation("Bid quantity cannot be zero"));
                    }
                }
                if let Some(qty) = ask_qty {
                    if *qty == 0 {
                        return Err(DataError::validation("Ask quantity cannot be zero"));
                    }
                }
            }
            Self::OrderPlacement(order) => {
                if order.qty == 0 {
                    return Err(DataError::validation("Order quantity cannot be zero"));
                }
                if let Some(price) = order.price() {
                    if price == 0 {
                        return Err(DataError::validation("Order price cannot be zero"));
                    }
                }
            }
            Self::OrderModification { new_qty, new_price, .. } => {
                if let Some(qty) = new_qty {
                    if *qty == 0 {
                        return Err(DataError::validation("Modified quantity cannot be zero"));
                    }
                }
                if let Some(price) = new_price {
                    if *price == 0 {
                        return Err(DataError::validation("Modified price cannot be zero"));
                    }
                }
            }
            Self::BestBidOffer {
                best_bid, best_ask, bid_qty, ask_qty, ..
            } => {
                if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
                    if bid >= ask {
                        return Err(DataError::validation("Best bid must be less than best ask"));
                    }
                }
                if let Some(qty) = bid_qty {
                    if *qty == 0 {
                        return Err(DataError::validation("Bid quantity cannot be zero"));
                    }
                }
                if let Some(qty) = ask_qty {
                    if *qty == 0 {
                        return Err(DataError::validation("Ask quantity cannot be zero"));
                    }
                }
            }
            _ => {} // Other events don't need validation
        }
        Ok(())
    }
}

/// Trait for pluggable data sources that can feed market events into the system
pub trait DataSource {
    /// Get the next market event from the data source
    /// Returns None when the end of data is reached
    fn next_event(&mut self) -> DataResult<Option<MarketEvent>>;

    /// Seek to a specific timestamp in the data stream
    /// This allows jumping to different points in historical data
    fn seek_to_time(&mut self, timestamp: u128) -> DataResult<()>;

    /// Set the playback speed multiplier
    /// 1.0 = real-time, 2.0 = 2x speed, 0.5 = half speed
    /// This affects the timing between events during replay
    fn set_playback_speed(&mut self, multiplier: f64) -> DataResult<()>;

    /// Check if the data source has reached the end
    fn is_finished(&self) -> bool;

    /// Get the current position/timestamp in the data stream
    fn current_position(&self) -> Option<u128>;

    /// Get the total duration/range of the data source if known
    fn duration(&self) -> Option<(u128, u128)>; // (start_time, end_time)

    /// Reset the data source to the beginning
    fn reset(&mut self) -> DataResult<()>;

    /// Get metadata about the data source
    fn metadata(&self) -> DataSourceMetadata;

    /// Pause/resume playback (for real-time sources)
    fn set_paused(&mut self, paused: bool) -> DataResult<()>;

    /// Check if playback is currently paused
    fn is_paused(&self) -> bool;
}

/// Metadata about a data source
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSourceMetadata {
    /// Name or identifier of the data source
    pub name: String,
    /// Type of data source (CSV, JSON, Binary, Live, etc.)
    pub source_type: String,
    /// Total number of events if known
    pub event_count: Option<usize>,
    /// Time range covered by the data
    pub time_range: Option<(u128, u128)>,
    /// File size in bytes if applicable
    pub file_size: Option<u64>,
    /// Additional properties
    pub properties: std::collections::HashMap<String, String>,
}

impl DataSourceMetadata {
    /// Create new metadata with basic information
    pub fn new<S1: Into<String>, S2: Into<String>>(name: S1, source_type: S2) -> Self {
        Self {
            name: name.into(),
            source_type: source_type.into(),
            event_count: None,
            time_range: None,
            file_size: None,
            properties: std::collections::HashMap::new(),
        }
    }

    /// Add a property to the metadata
    pub fn with_property<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Set the event count
    pub fn with_event_count(mut self, count: usize) -> Self {
        self.event_count = Some(count);
        self
    }

    /// Set the time range
    pub fn with_time_range(mut self, start: u128, end: u128) -> Self {
        self.time_range = Some((start, end));
        self
    }

    /// Set the file size
    pub fn with_file_size(mut self, size: u64) -> Self {
        self.file_size = Some(size);
        self
    }
}

// Conversion from std::io::Error to DataError
impl From<std::io::Error> for DataError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string(),
        }
    }
}

// Conversion from serde_json::Error to DataError
impl From<serde_json::Error> for DataError {
    fn from(err: serde_json::Error) -> Self {
        Self::InvalidFormat {
            file: "unknown".to_string(),
            details: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::price_utils;

    #[test]
    fn test_market_event_timestamp() {
        let trade = MarketEvent::Trade {
            price: price_utils::from_f64(100.0),
            qty: 100,
            side: Side::Buy,
            timestamp: 1000,
            trade_id: None,
        };
        assert_eq!(trade.timestamp(), 1000);

        let order = Order::new_limit(1, Side::Buy, 100, price_utils::from_f64(99.0), 2000);
        let placement = MarketEvent::OrderPlacement(order);
        assert_eq!(placement.timestamp(), 2000);
    }

    #[test]
    fn test_market_event_classification() {
        let trade = MarketEvent::Trade {
            price: price_utils::from_f64(100.0),
            qty: 100,
            side: Side::Buy,
            timestamp: 1000,
            trade_id: None,
        };
        assert!(trade.affects_book());
        assert!(!trade.is_market_data());

        let quote = MarketEvent::Quote {
            bid: Some(price_utils::from_f64(99.0)),
            ask: Some(price_utils::from_f64(101.0)),
            bid_qty: Some(100),
            ask_qty: Some(200),
            timestamp: 1000,
        };
        assert!(!quote.affects_book());
        assert!(quote.is_market_data());
    }

    #[test]
    fn test_market_event_validation() {
        // Valid trade
        let trade = MarketEvent::Trade {
            price: price_utils::from_f64(100.0),
            qty: 100,
            side: Side::Buy,
            timestamp: 1000,
            trade_id: None,
        };
        assert!(trade.validate().is_ok());

        // Invalid trade - zero price
        let invalid_trade = MarketEvent::Trade {
            price: 0,
            qty: 100,
            side: Side::Buy,
            timestamp: 1000,
            trade_id: None,
        };
        assert!(invalid_trade.validate().is_err());

        // Valid quote
        let quote = MarketEvent::Quote {
            bid: Some(price_utils::from_f64(99.0)),
            ask: Some(price_utils::from_f64(101.0)),
            bid_qty: Some(100),
            ask_qty: Some(200),
            timestamp: 1000,
        };
        assert!(quote.validate().is_ok());

        // Invalid quote - bid >= ask
        let invalid_quote = MarketEvent::Quote {
            bid: Some(price_utils::from_f64(101.0)),
            ask: Some(price_utils::from_f64(99.0)),
            bid_qty: Some(100),
            ask_qty: Some(200),
            timestamp: 1000,
        };
        assert!(invalid_quote.validate().is_err());
    }

    #[test]
    fn test_data_error_creation() {
        let err = DataError::file_not_found("test.csv");
        assert_eq!(err.to_string(), "File not found: test.csv");

        let err = DataError::parse_error("test.csv", 5, "Invalid number");
        assert_eq!(err.to_string(), "Parse error at line 5 in test.csv: Invalid number");

        let err = DataError::validation("Price cannot be negative");
        assert_eq!(err.to_string(), "Validation error: Price cannot be negative");
    }

    #[test]
    fn test_data_source_metadata() {
        let metadata = DataSourceMetadata::new("test.csv", "CSV")
            .with_property("symbol", "AAPL")
            .with_event_count(1000)
            .with_time_range(1000, 2000)
            .with_file_size(1024);

        assert_eq!(metadata.name, "test.csv");
        assert_eq!(metadata.source_type, "CSV");
        assert_eq!(metadata.event_count, Some(1000));
        assert_eq!(metadata.time_range, Some((1000, 2000)));
        assert_eq!(metadata.file_size, Some(1024));
        assert_eq!(metadata.properties.get("symbol"), Some(&"AAPL".to_string()));
    }

    #[test]
    fn test_market_status_serialization() {
        let status = MarketEvent::MarketStatus {
            status: MarketStatusType::Open,
            timestamp: 1000,
            message: Some("Market opened".to_string()),
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: MarketEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_error_conversions() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let data_err: DataError = io_err.into();
        assert!(matches!(data_err, DataError::IoError { .. }));

        let json_err = serde_json::from_str::<i32>("invalid json").unwrap_err();
        let data_err: DataError = json_err.into();
        assert!(matches!(data_err, DataError::InvalidFormat { .. }));
    }
}