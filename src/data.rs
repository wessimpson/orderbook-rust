use crate::types::{Order, OrderId, Price, Qty, Side, price_utils};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use csv::{Reader, StringRecord};

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

// Conversion from csv::Error to DataError
impl From<csv::Error> for DataError {
    fn from(err: csv::Error) -> Self {
        Self::InvalidFormat {
            file: "unknown".to_string(),
            details: err.to_string(),
        }
    }
}

/// CSV data source for historical market data replay
#[derive(Debug)]
pub struct CsvDataSource {
    /// CSV reader for the data file
    reader: Reader<File>,
    /// Path to the CSV file
    file_path: PathBuf,
    /// Current line number for error reporting
    current_line: usize,
    /// Playback speed multiplier (1.0 = real-time)
    playback_speed: f64,
    /// Whether playback is paused
    paused: bool,
    /// Last event timestamp for timing control
    last_timestamp: Option<u128>,
    /// Start time for playback timing
    playback_start: Option<Instant>,
    /// Current position in the data stream
    current_position: Option<u128>,
    /// Metadata about the data source
    metadata: DataSourceMetadata,
    /// Whether we've reached the end of the file
    finished: bool,
    /// Buffer for the next record
    record_buffer: StringRecord,
}

impl CsvDataSource {
    /// Create a new CSV data source from a file path
    pub fn new<P: AsRef<Path>>(file_path: P) -> DataResult<Self> {
        let path = file_path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|_| DataError::file_not_found(path.display().to_string()))?;
        
        let reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true) // Allow records with different numbers of fields
            .from_reader(file);

        // Get file metadata
        let file_size = std::fs::metadata(&path)?.len();
        let metadata = DataSourceMetadata::new(
            path.file_name().unwrap_or_default().to_string_lossy(),
            "CSV"
        ).with_file_size(file_size);

        Ok(Self {
            reader,
            file_path: path,
            current_line: 1, // Start at 1 since we have headers
            playback_speed: 1.0,
            paused: false,
            last_timestamp: None,
            playback_start: None,
            current_position: None,
            metadata,
            finished: false,
            record_buffer: StringRecord::new(),
        })
    }

    /// Parse a CSV record into a MarketEvent
    fn parse_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 3 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Insufficient columns in CSV record"
            ));
        }

        // First column should be event type
        let event_type = record.get(0).ok_or_else(|| {
            DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Missing event type column"
            )
        })?;

        match event_type.to_lowercase().as_str() {
            "trade" => self.parse_trade_record(record),
            "quote" => self.parse_quote_record(record),
            "order" => self.parse_order_record(record),
            "cancel" => self.parse_cancel_record(record),
            "modify" => self.parse_modify_record(record),
            "status" => self.parse_status_record(record),
            "bbo" => self.parse_bbo_record(record),
            _ => Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Unknown event type: {}", event_type)
            ))
        }
    }

    /// Parse a trade record: trade,timestamp,price,qty,side[,trade_id]
    fn parse_trade_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 5 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Trade record requires at least 5 columns: type,timestamp,price,qty,side"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let price = self.parse_price(record.get(2).unwrap())?;
        let qty = self.parse_qty(record.get(3).unwrap())?;
        let side = self.parse_side(record.get(4).unwrap())?;
        let trade_id = record.get(5).map(|s| s.to_string()).filter(|s| !s.is_empty());

        Ok(MarketEvent::Trade {
            price,
            qty,
            side,
            timestamp,
            trade_id,
        })
    }

    /// Parse a quote record: quote,timestamp,bid,ask,bid_qty,ask_qty
    fn parse_quote_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 6 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Quote record requires 6 columns: type,timestamp,bid,ask,bid_qty,ask_qty"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let bid = self.parse_optional_price(record.get(2).unwrap())?;
        let ask = self.parse_optional_price(record.get(3).unwrap())?;
        let bid_qty = self.parse_optional_qty(record.get(4).unwrap())?;
        let ask_qty = self.parse_optional_qty(record.get(5).unwrap())?;

        Ok(MarketEvent::Quote {
            bid,
            ask,
            bid_qty,
            ask_qty,
            timestamp,
        })
    }

    /// Parse an order record: order,timestamp,order_id,side,qty,price,order_type
    fn parse_order_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 7 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Order record requires 7 columns: type,timestamp,order_id,side,qty,price,order_type"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let order_id = self.parse_order_id(record.get(2).unwrap())?;
        let side = self.parse_side(record.get(3).unwrap())?;
        let qty = self.parse_qty(record.get(4).unwrap())?;
        let price_str = record.get(5).unwrap();
        let order_type_str = record.get(6).unwrap();

        let order = match order_type_str.to_lowercase().as_str() {
            "limit" => {
                let price = self.parse_price(price_str)?;
                Order::new_limit(order_id, side, qty, price, timestamp)
            }
            "market" => Order::new_market(order_id, side, qty, timestamp),
            _ => return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Unknown order type: {}", order_type_str)
            ))
        };

        Ok(MarketEvent::OrderPlacement(order))
    }

    /// Parse a cancel record: cancel,timestamp,order_id[,reason]
    fn parse_cancel_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 3 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Cancel record requires at least 3 columns: type,timestamp,order_id"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let order_id = self.parse_order_id(record.get(2).unwrap())?;
        let reason = record.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty());

        Ok(MarketEvent::OrderCancellation {
            order_id,
            timestamp,
            reason,
        })
    }

    /// Parse a modify record: modify,timestamp,order_id,new_qty,new_price
    fn parse_modify_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 5 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Modify record requires 5 columns: type,timestamp,order_id,new_qty,new_price"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let order_id = self.parse_order_id(record.get(2).unwrap())?;
        let new_qty = self.parse_optional_qty(record.get(3).unwrap())?;
        let new_price = self.parse_optional_price(record.get(4).unwrap())?;

        Ok(MarketEvent::OrderModification {
            order_id,
            new_qty,
            new_price,
            timestamp,
        })
    }

    /// Parse a status record: status,timestamp,status[,message]
    fn parse_status_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 3 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Status record requires at least 3 columns: type,timestamp,status"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let status_str = record.get(2).unwrap();
        let message = record.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty());

        let status = match status_str.to_lowercase().as_str() {
            "open" => MarketStatusType::Open,
            "closed" => MarketStatusType::Closed,
            "halted" => MarketStatusType::Halted,
            "premarket" => MarketStatusType::PreMarket,
            "afterhours" => MarketStatusType::AfterHours,
            "auction" => MarketStatusType::Auction,
            _ => return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Unknown market status: {}", status_str)
            ))
        };

        Ok(MarketEvent::MarketStatus {
            status,
            timestamp,
            message,
        })
    }

    /// Parse a BBO record: bbo,timestamp,best_bid,best_ask,bid_qty,ask_qty
    fn parse_bbo_record(&self, record: &StringRecord) -> DataResult<MarketEvent> {
        if record.len() < 6 {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "BBO record requires 6 columns: type,timestamp,best_bid,best_ask,bid_qty,ask_qty"
            ));
        }

        let timestamp = self.parse_timestamp(record.get(1).unwrap())?;
        let best_bid = self.parse_optional_price(record.get(2).unwrap())?;
        let best_ask = self.parse_optional_price(record.get(3).unwrap())?;
        let bid_qty = self.parse_optional_qty(record.get(4).unwrap())?;
        let ask_qty = self.parse_optional_qty(record.get(5).unwrap())?;

        Ok(MarketEvent::BestBidOffer {
            best_bid,
            best_ask,
            bid_qty,
            ask_qty,
            timestamp,
        })
    }

    /// Parse timestamp from string (nanoseconds since epoch)
    fn parse_timestamp(&self, s: &str) -> DataResult<u128> {
        s.parse::<u128>().map_err(|_| {
            DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Invalid timestamp: {}", s)
            )
        })
    }

    /// Parse price from string (converts to ticks)
    fn parse_price(&self, s: &str) -> DataResult<Price> {
        if s.is_empty() {
            return Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                "Empty price field"
            ));
        }

        s.parse::<f64>()
            .map(price_utils::from_f64)
            .map_err(|_| {
                DataError::parse_error(
                    &self.file_path.display().to_string(),
                    self.current_line,
                    format!("Invalid price: {}", s)
                )
            })
    }

    /// Parse optional price from string (empty string = None)
    fn parse_optional_price(&self, s: &str) -> DataResult<Option<Price>> {
        if s.is_empty() || s == "null" || s == "NULL" {
            Ok(None)
        } else {
            self.parse_price(s).map(Some)
        }
    }

    /// Parse quantity from string
    fn parse_qty(&self, s: &str) -> DataResult<Qty> {
        s.parse::<Qty>().map_err(|_| {
            DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Invalid quantity: {}", s)
            )
        })
    }

    /// Parse optional quantity from string (empty string = None)
    fn parse_optional_qty(&self, s: &str) -> DataResult<Option<Qty>> {
        if s.is_empty() || s == "null" || s == "NULL" {
            Ok(None)
        } else {
            self.parse_qty(s).map(Some)
        }
    }

    /// Parse side from string
    fn parse_side(&self, s: &str) -> DataResult<Side> {
        match s.to_lowercase().as_str() {
            "buy" | "b" => Ok(Side::Buy),
            "sell" | "s" => Ok(Side::Sell),
            _ => Err(DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Invalid side: {}", s)
            ))
        }
    }

    /// Parse order ID from string
    fn parse_order_id(&self, s: &str) -> DataResult<OrderId> {
        s.parse::<OrderId>().map_err(|_| {
            DataError::parse_error(
                &self.file_path.display().to_string(),
                self.current_line,
                format!("Invalid order ID: {}", s)
            )
        })
    }

    /// Handle timing for playback speed control
    fn handle_timing(&mut self, event_timestamp: u128) -> DataResult<()> {
        if self.paused {
            return Ok(());
        }

        if let Some(last_ts) = self.last_timestamp {
            if event_timestamp > last_ts {
                let time_diff_ns = event_timestamp - last_ts;
                let real_time_diff = Duration::from_nanos(time_diff_ns as u64);
                let adjusted_diff = real_time_diff.div_f64(self.playback_speed);

                if let Some(start_time) = self.playback_start {
                    let elapsed = start_time.elapsed();
                    let target_elapsed = Duration::from_nanos((event_timestamp - self.last_timestamp.unwrap_or(0)) as u64);
                    
                    if elapsed < target_elapsed {
                        std::thread::sleep(target_elapsed - elapsed);
                    }
                } else {
                    std::thread::sleep(adjusted_diff);
                }
            }
        } else {
            // First event, record start time
            self.playback_start = Some(Instant::now());
        }

        self.last_timestamp = Some(event_timestamp);
        Ok(())
    }
}

impl DataSource for CsvDataSource {
    fn next_event(&mut self) -> DataResult<Option<MarketEvent>> {
        if self.finished {
            return Ok(None);
        }

        // Read next record
        if !self.reader.read_record(&mut self.record_buffer)? {
            self.finished = true;
            return Ok(None);
        }

        self.current_line += 1;

        // Parse the record
        let event = self.parse_record(&self.record_buffer)?;
        
        // Validate the event
        event.validate()?;

        // Update current position
        self.current_position = Some(event.timestamp());

        // Handle timing for playback speed
        self.handle_timing(event.timestamp())?;

        Ok(Some(event))
    }

    fn seek_to_time(&mut self, timestamp: u128) -> DataResult<()> {
        // Reset to beginning and scan for the target timestamp
        self.reset()?;
        
        loop {
            let position = self.reader.position().clone();
            
            if !self.reader.read_record(&mut self.record_buffer)? {
                // Reached end of file
                break;
            }

            self.current_line += 1;
            
            // Parse just to get the timestamp
            if let Ok(event) = self.parse_record(&self.record_buffer) {
                if event.timestamp() >= timestamp {
                    // Found target, seek back to this position
                    self.reader.seek(position)?;
                    self.current_line -= 1;
                    self.current_position = Some(event.timestamp());
                    return Ok(());
                }
            }
        }

        Err(DataError::seek_failed(format!("Timestamp {} not found in data", timestamp)))
    }

    fn set_playback_speed(&mut self, multiplier: f64) -> DataResult<()> {
        if multiplier <= 0.0 {
            return Err(DataError::validation("Playback speed must be positive"));
        }
        self.playback_speed = multiplier;
        Ok(())
    }

    fn is_finished(&self) -> bool {
        self.finished
    }

    fn current_position(&self) -> Option<u128> {
        self.current_position
    }

    fn duration(&self) -> Option<(u128, u128)> {
        // This would require scanning the entire file, which is expensive
        // For now, return None - could be implemented as an optimization
        None
    }

    fn reset(&mut self) -> DataResult<()> {
        // Reopen the file and create a new reader
        let file = File::open(&self.file_path)
            .map_err(|_| DataError::file_not_found(self.file_path.display().to_string()))?;
        
        self.reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true) // Allow records with different numbers of fields
            .from_reader(file);
        
        self.current_line = 1;
        self.finished = false;
        self.last_timestamp = None;
        self.playback_start = None;
        self.current_position = None;
        
        Ok(())
    }

    fn metadata(&self) -> DataSourceMetadata {
        self.metadata.clone()
    }

    fn set_paused(&mut self, paused: bool) -> DataResult<()> {
        self.paused = paused;
        if !paused {
            // Reset timing when resuming
            self.playback_start = Some(Instant::now());
        }
        Ok(())
    }

    fn is_paused(&self) -> bool {
        self.paused
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

    #[test]
    fn test_csv_data_source_creation() {
        // Test with non-existent file
        let result = CsvDataSource::new("non_existent.csv");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DataError::FileNotFound { .. }));
    }

    #[test]
    fn test_csv_parsing_trade_record() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temporary CSV file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side,trade_id").unwrap();
        writeln!(temp_file, "trade,1000000000,100.25,500,buy,T123").unwrap();
        writeln!(temp_file, "trade,1000000001,100.30,200,sell,").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test first trade
        let event1 = csv_source.next_event().unwrap().unwrap();
        match event1 {
            MarketEvent::Trade { price, qty, side, timestamp, trade_id } => {
                assert_eq!(price, price_utils::from_f64(100.25));
                assert_eq!(qty, 500);
                assert_eq!(side, Side::Buy);
                assert_eq!(timestamp, 1000000000);
                assert_eq!(trade_id, Some("T123".to_string()));
            }
            _ => panic!("Expected Trade event"),
        }

        // Test second trade
        let event2 = csv_source.next_event().unwrap().unwrap();
        match event2 {
            MarketEvent::Trade { price, qty, side, timestamp, trade_id } => {
                assert_eq!(price, price_utils::from_f64(100.30));
                assert_eq!(qty, 200);
                assert_eq!(side, Side::Sell);
                assert_eq!(timestamp, 1000000001);
                assert_eq!(trade_id, None);
            }
            _ => panic!("Expected Trade event"),
        }

        // Test end of file
        let event3 = csv_source.next_event().unwrap();
        assert!(event3.is_none());
        assert!(csv_source.is_finished());
    }

    #[test]
    fn test_csv_parsing_quote_record() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,bid,ask,bid_qty,ask_qty").unwrap();
        writeln!(temp_file, "quote,2000000000,99.95,100.05,1000,1500").unwrap();
        writeln!(temp_file, "quote,2000000001,,100.10,,2000").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test first quote
        let event1 = csv_source.next_event().unwrap().unwrap();
        match event1 {
            MarketEvent::Quote { bid, ask, bid_qty, ask_qty, timestamp } => {
                assert_eq!(bid, Some(price_utils::from_f64(99.95)));
                assert_eq!(ask, Some(price_utils::from_f64(100.05)));
                assert_eq!(bid_qty, Some(1000));
                assert_eq!(ask_qty, Some(1500));
                assert_eq!(timestamp, 2000000000);
            }
            _ => panic!("Expected Quote event"),
        }

        // Test second quote with missing values
        let event2 = csv_source.next_event().unwrap().unwrap();
        match event2 {
            MarketEvent::Quote { bid, ask, bid_qty, ask_qty, timestamp } => {
                assert_eq!(bid, None);
                assert_eq!(ask, Some(price_utils::from_f64(100.10)));
                assert_eq!(bid_qty, None);
                assert_eq!(ask_qty, Some(2000));
                assert_eq!(timestamp, 2000000001);
            }
            _ => panic!("Expected Quote event"),
        }
    }

    #[test]
    fn test_csv_parsing_order_record() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,order_id,side,qty,price,order_type").unwrap();
        writeln!(temp_file, "order,3000000000,12345,buy,100,99.50,limit").unwrap();
        writeln!(temp_file, "order,3000000001,12346,sell,200,,market").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test limit order
        let event1 = csv_source.next_event().unwrap().unwrap();
        match event1 {
            MarketEvent::OrderPlacement(order) => {
                assert_eq!(order.id, 12345);
                assert_eq!(order.side, Side::Buy);
                assert_eq!(order.qty, 100);
                assert_eq!(order.price(), Some(price_utils::from_f64(99.50)));
                assert_eq!(order.ts, 3000000000);
                assert!(order.is_limit());
            }
            _ => panic!("Expected OrderPlacement event"),
        }

        // Test market order
        let event2 = csv_source.next_event().unwrap().unwrap();
        match event2 {
            MarketEvent::OrderPlacement(order) => {
                assert_eq!(order.id, 12346);
                assert_eq!(order.side, Side::Sell);
                assert_eq!(order.qty, 200);
                assert_eq!(order.price(), None);
                assert_eq!(order.ts, 3000000001);
                assert!(order.is_market());
            }
            _ => panic!("Expected OrderPlacement event"),
        }
    }

    #[test]
    fn test_csv_parsing_cancel_record() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,order_id,reason").unwrap();
        writeln!(temp_file, "cancel,4000000000,12345,User requested").unwrap();
        writeln!(temp_file, "cancel,4000000001,12346,").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test cancel with reason
        let event1 = csv_source.next_event().unwrap().unwrap();
        match event1 {
            MarketEvent::OrderCancellation { order_id, timestamp, reason } => {
                assert_eq!(order_id, 12345);
                assert_eq!(timestamp, 4000000000);
                assert_eq!(reason, Some("User requested".to_string()));
            }
            _ => panic!("Expected OrderCancellation event"),
        }

        // Test cancel without reason
        let event2 = csv_source.next_event().unwrap().unwrap();
        match event2 {
            MarketEvent::OrderCancellation { order_id, timestamp, reason } => {
                assert_eq!(order_id, 12346);
                assert_eq!(timestamp, 4000000001);
                assert_eq!(reason, None);
            }
            _ => panic!("Expected OrderCancellation event"),
        }
    }

    #[test]
    fn test_csv_parsing_status_record() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,status,message").unwrap();
        writeln!(temp_file, "status,5000000000,open,Market opened").unwrap();
        writeln!(temp_file, "status,5000000001,halted,").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test status with message
        let event1 = csv_source.next_event().unwrap().unwrap();
        match event1 {
            MarketEvent::MarketStatus { status, timestamp, message } => {
                assert_eq!(status, MarketStatusType::Open);
                assert_eq!(timestamp, 5000000000);
                assert_eq!(message, Some("Market opened".to_string()));
            }
            _ => panic!("Expected MarketStatus event"),
        }

        // Test status without message
        let event2 = csv_source.next_event().unwrap().unwrap();
        match event2 {
            MarketEvent::MarketStatus { status, timestamp, message } => {
                assert_eq!(status, MarketStatusType::Halted);
                assert_eq!(timestamp, 5000000001);
                assert_eq!(message, None);
            }
            _ => panic!("Expected MarketStatus event"),
        }
    }

    #[test]
    fn test_csv_data_validation() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side").unwrap();
        writeln!(temp_file, "trade,1000000000,0,100,buy").unwrap(); // Invalid: zero price
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Should fail validation
        let result = csv_source.next_event();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DataError::ValidationError { .. }));
    }

    #[test]
    fn test_csv_malformed_records() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side").unwrap();
        writeln!(temp_file, "trade,invalid_timestamp,100.25,500,buy").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Should fail parsing
        let result = csv_source.next_event();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DataError::ParseError { .. }));
    }

    #[test]
    fn test_csv_playback_speed() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side").unwrap();
        writeln!(temp_file, "trade,1000000000,100.25,500,buy").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test setting playback speed
        assert!(csv_source.set_playback_speed(2.0).is_ok());
        assert!(csv_source.set_playback_speed(0.5).is_ok());
        
        // Test invalid playback speed
        assert!(csv_source.set_playback_speed(0.0).is_err());
        assert!(csv_source.set_playback_speed(-1.0).is_err());
    }

    #[test]
    fn test_csv_pause_resume() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side").unwrap();
        writeln!(temp_file, "trade,1000000000,100.25,500,buy").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Test pause/resume
        assert!(!csv_source.is_paused());
        assert!(csv_source.set_paused(true).is_ok());
        assert!(csv_source.is_paused());
        assert!(csv_source.set_paused(false).is_ok());
        assert!(!csv_source.is_paused());
    }

    #[test]
    fn test_csv_reset() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side").unwrap();
        writeln!(temp_file, "trade,1000000000,100.25,500,buy").unwrap();
        writeln!(temp_file, "trade,1000000001,100.30,200,sell").unwrap();
        temp_file.flush().unwrap();

        let mut csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        
        // Read first event
        let event1 = csv_source.next_event().unwrap().unwrap();
        assert!(matches!(event1, MarketEvent::Trade { .. }));
        
        // Reset and read first event again
        assert!(csv_source.reset().is_ok());
        assert!(!csv_source.is_finished());
        let event1_again = csv_source.next_event().unwrap().unwrap();
        assert_eq!(event1, event1_again);
    }

    #[test]
    fn test_csv_metadata() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "type,timestamp,price,qty,side").unwrap();
        writeln!(temp_file, "trade,1000000000,100.25,500,buy").unwrap();
        temp_file.flush().unwrap();

        let csv_source = CsvDataSource::new(temp_file.path()).unwrap();
        let metadata = csv_source.metadata();
        
        assert_eq!(metadata.source_type, "CSV");
        assert!(metadata.file_size.is_some());
        assert!(metadata.file_size.unwrap() > 0);
    }

    #[test]
    fn test_csv_integration_with_sample_file() {
        // Test with the sample CSV file if it exists
        if std::path::Path::new("sample_data.csv").exists() {
            let mut csv_source = CsvDataSource::new("sample_data.csv").unwrap();
            let mut event_count = 0;
            let mut trade_count = 0;
            let mut quote_count = 0;
            let mut order_count = 0;
            let mut cancel_count = 0;
            let mut status_count = 0;
            let mut bbo_count = 0;
            let mut error_count = 0;

            loop {
                match csv_source.next_event() {
                    Ok(Some(event)) => {
                        event_count += 1;
                        match event {
                            MarketEvent::Trade { .. } => trade_count += 1,
                            MarketEvent::Quote { .. } => quote_count += 1,
                            MarketEvent::OrderPlacement(_) => order_count += 1,
                            MarketEvent::OrderCancellation { .. } => cancel_count += 1,
                            MarketEvent::MarketStatus { .. } => status_count += 1,
                            MarketEvent::BestBidOffer { .. } => bbo_count += 1,
                            _ => {}
                        }
                        
                        // Verify all events have valid timestamps
                        assert!(event.timestamp() > 0);
                    }
                    Ok(None) => break, // End of file
                    Err(e) => {
                        error_count += 1;
                        println!("Error parsing record: {:?}", e);
                        break; // Stop on first error for debugging
                    }
                }
            }

            // Debug output
            println!("Events: {}, Trades: {}, Quotes: {}, Orders: {}, Cancels: {}, Status: {}, BBO: {}, Errors: {}", 
                     event_count, trade_count, quote_count, order_count, cancel_count, status_count, bbo_count, error_count);

            assert!(event_count > 0);
            assert!(trade_count > 0);
            assert!(quote_count > 0);
            assert!(order_count > 0);
            assert!(cancel_count > 0);
            assert!(status_count > 0);
            assert!(bbo_count > 0);
            assert_eq!(error_count, 0);
            assert!(csv_source.is_finished());
        }
    }
}