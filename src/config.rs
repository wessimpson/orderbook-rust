use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::env;
use crate::sim::{NetModel, MarketMakerConfig, OrderGenerationConfig};

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// Simulation configuration
    pub simulation: SimulationConfig,
    /// Network simulation parameters
    pub network: NetModel,
    /// Market maker configuration
    pub market_maker: MarketMakerConfig,
    /// Order generation configuration
    pub order_generation: OrderGenerationConfig,
    /// Data source configuration
    pub data_source: DataSourceConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Server configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Port to bind the WebSocket server
    pub port: u16,
    /// Host address to bind to
    pub host: String,
    /// Maximum number of concurrent WebSocket connections
    pub max_connections: usize,
    /// WebSocket message buffer size
    pub message_buffer_size: usize,
    /// Enable CORS for cross-origin requests
    pub enable_cors: bool,
    /// Health check endpoint path
    pub health_endpoint: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "0.0.0.0".to_string(),
            max_connections: 1000,
            message_buffer_size: 100,
            enable_cors: true,
            health_endpoint: "/health".to_string(),
        }
    }
}

/// Simulation configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Interval between simulation steps in milliseconds
    pub step_interval_ms: u64,
    /// Random seed for deterministic simulation
    pub random_seed: Option<u64>,
    /// Maximum number of price levels to maintain in snapshots
    pub max_depth_levels: usize,
    /// Maximum number of spread history points to keep
    pub max_spread_history: usize,
    /// Enable performance monitoring
    pub enable_monitoring: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            step_interval_ms: 100,
            random_seed: Some(42),
            max_depth_levels: 20,
            max_spread_history: 400,
            enable_monitoring: true,
        }
    }
}

/// Data source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceConfig {
    /// Default data directory for CSV/JSON files
    pub data_directory: PathBuf,
    /// Default CSV file for historical replay
    pub default_csv_file: Option<PathBuf>,
    /// Default JSON file for historical replay
    pub default_json_file: Option<PathBuf>,
    /// Maximum file size to process (in bytes)
    pub max_file_size: u64,
    /// Default playback speed multiplier
    pub default_playback_speed: f64,
    /// Enable data validation
    pub validate_data: bool,
}

impl Default for DataSourceConfig {
    fn default() -> Self {
        Self {
            data_directory: PathBuf::from("./data"),
            default_csv_file: None,
            default_json_file: None,
            max_file_size: 1024 * 1024 * 1024, // 1GB
            default_playback_speed: 1.0,
            validate_data: true,
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (error, warn, info, debug, trace)
    pub level: String,
    /// Enable structured JSON logging
    pub json_format: bool,
    /// Log file path (None for stdout only)
    pub log_file: Option<PathBuf>,
    /// Enable performance logging
    pub enable_performance_logs: bool,
    /// Enable WebSocket event logging
    pub enable_websocket_logs: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            json_format: false,
            log_file: None,
            enable_performance_logs: true,
            enable_websocket_logs: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            simulation: SimulationConfig::default(),
            network: NetModel::default(),
            market_maker: MarketMakerConfig::default(),
            order_generation: OrderGenerationConfig::default(),
            data_source: DataSourceConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file, falling back to defaults
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Ok(Self::default());
        }
        
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::IoError(format!("Failed to read config file: {}", e)))?;
        
        let config: Config = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse config file: {}", e)))?;
        
        config.validate()?;
        Ok(config)
    }
    
    /// Load configuration from environment variables and file
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Self::load_from_file("config.toml")?;
        
        // Override with environment variables
        config.apply_env_overrides();
        config.validate()?;
        
        Ok(config)
    }
    
    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Server configuration
        if let Ok(port) = env::var("ORDERBOOK_PORT") {
            if let Ok(port) = port.parse() {
                self.server.port = port;
            }
        }
        
        if let Ok(host) = env::var("ORDERBOOK_HOST") {
            self.server.host = host;
        }
        
        if let Ok(max_conn) = env::var("ORDERBOOK_MAX_CONNECTIONS") {
            if let Ok(max_conn) = max_conn.parse() {
                self.server.max_connections = max_conn;
            }
        }
        
        // Simulation configuration
        if let Ok(interval) = env::var("ORDERBOOK_SIMULATION_INTERVAL") {
            if let Ok(interval) = interval.parse() {
                self.simulation.step_interval_ms = interval;
            }
        }
        
        if let Ok(seed) = env::var("ORDERBOOK_RANDOM_SEED") {
            if let Ok(seed) = seed.parse() {
                self.simulation.random_seed = Some(seed);
            }
        }
        
        // Data source configuration
        if let Ok(data_dir) = env::var("ORDERBOOK_DATA_DIR") {
            self.data_source.data_directory = PathBuf::from(data_dir);
        }
        
        if let Ok(csv_file) = env::var("ORDERBOOK_CSV_FILE") {
            self.data_source.default_csv_file = Some(PathBuf::from(csv_file));
        }
        
        if let Ok(json_file) = env::var("ORDERBOOK_JSON_FILE") {
            self.data_source.default_json_file = Some(PathBuf::from(json_file));
        }
        
        // Logging configuration
        if let Ok(log_level) = env::var("RUST_LOG") {
            self.logging.level = log_level;
        }
        
        if let Ok(log_file) = env::var("ORDERBOOK_LOG_FILE") {
            self.logging.log_file = Some(PathBuf::from(log_file));
        }
        
        // Network configuration
        if let Ok(latency) = env::var("ORDERBOOK_BASE_LATENCY_NS") {
            if let Ok(latency) = latency.parse() {
                self.network.base_latency_ns = latency;
            }
        }
        
        if let Ok(jitter) = env::var("ORDERBOOK_JITTER_NS") {
            if let Ok(jitter) = jitter.parse() {
                self.network.jitter_ns = jitter;
            }
        }
    }
    
    /// Validate configuration values
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate server configuration
        if self.server.port == 0 {
            return Err(ConfigError::ValidationError("Server port cannot be 0".to_string()));
        }
        
        if self.server.max_connections == 0 {
            return Err(ConfigError::ValidationError("Max connections cannot be 0".to_string()));
        }
        
        if self.server.message_buffer_size == 0 {
            return Err(ConfigError::ValidationError("Message buffer size cannot be 0".to_string()));
        }
        
        // Validate simulation configuration
        if self.simulation.step_interval_ms == 0 {
            return Err(ConfigError::ValidationError("Simulation step interval cannot be 0".to_string()));
        }
        
        if self.simulation.step_interval_ms > 60000 {
            return Err(ConfigError::ValidationError("Simulation step interval cannot exceed 60 seconds".to_string()));
        }
        
        if self.simulation.max_depth_levels == 0 {
            return Err(ConfigError::ValidationError("Max depth levels cannot be 0".to_string()));
        }
        
        // Validate network configuration
        if self.network.drop_prob < 0.0 || self.network.drop_prob > 1.0 {
            return Err(ConfigError::ValidationError("Drop probability must be between 0.0 and 1.0".to_string()));
        }
        
        if self.network.reorder_prob < 0.0 || self.network.reorder_prob > 1.0 {
            return Err(ConfigError::ValidationError("Reorder probability must be between 0.0 and 1.0".to_string()));
        }
        
        // Validate market maker configuration
        if self.market_maker.target_spread == 0 {
            return Err(ConfigError::ValidationError("Target spread cannot be 0".to_string()));
        }
        
        if self.market_maker.order_size == 0 {
            return Err(ConfigError::ValidationError("Market maker order size cannot be 0".to_string()));
        }
        
        if self.market_maker.mm_probability < 0.0 || self.market_maker.mm_probability > 1.0 {
            return Err(ConfigError::ValidationError("Market maker probability must be between 0.0 and 1.0".to_string()));
        }
        
        // Validate order generation configuration
        if self.order_generation.market_order_prob < 0.0 || self.order_generation.market_order_prob > 1.0 {
            return Err(ConfigError::ValidationError("Market order probability must be between 0.0 and 1.0".to_string()));
        }
        
        if self.order_generation.min_order_size == 0 {
            return Err(ConfigError::ValidationError("Minimum order size cannot be 0".to_string()));
        }
        
        if self.order_generation.max_order_size < self.order_generation.min_order_size {
            return Err(ConfigError::ValidationError("Maximum order size cannot be less than minimum order size".to_string()));
        }
        
        // Validate data source configuration
        if self.data_source.max_file_size == 0 {
            return Err(ConfigError::ValidationError("Max file size cannot be 0".to_string()));
        }
        
        if self.data_source.default_playback_speed <= 0.0 {
            return Err(ConfigError::ValidationError("Default playback speed must be positive".to_string()));
        }
        
        // Validate logging configuration
        let valid_levels = ["error", "warn", "info", "debug", "trace", "off"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(ConfigError::ValidationError(format!("Invalid log level: {}", self.logging.level)));
        }
        
        Ok(())
    }
    
    /// Save configuration to file
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::SerializeError(format!("Failed to serialize config: {}", e)))?;
        
        fs::write(path, content)
            .map_err(|e| ConfigError::IoError(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }
    
    /// Generate a default configuration file
    pub fn generate_default_config_file() -> Result<(), ConfigError> {
        let config = Self::default();
        config.save_to_file("config.toml")?;
        println!("Generated default configuration file: config.toml");
        Ok(())
    }
}

/// Configuration error types
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Serialization error: {0}")]
    SerializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.simulation.step_interval_ms, 100);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        
        // Test invalid port
        config.server.port = 0;
        assert!(config.validate().is_err());
        
        // Test invalid simulation interval
        config.server.port = 3000;
        config.simulation.step_interval_ms = 0;
        assert!(config.validate().is_err());
        
        // Test invalid probabilities
        config.simulation.step_interval_ms = 100;
        config.network.drop_prob = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_file_operations() {
        let config = Config::default();
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // Test saving
        config.save_to_file(temp_file.path()).unwrap();
        
        // Test loading
        let loaded_config = Config::load_from_file(temp_file.path()).unwrap();
        assert_eq!(config.server.port, loaded_config.server.port);
        assert_eq!(config.simulation.step_interval_ms, loaded_config.simulation.step_interval_ms);
    }

    #[test]
    fn test_env_overrides() {
        env::set_var("ORDERBOOK_PORT", "8080");
        env::set_var("ORDERBOOK_SIMULATION_INTERVAL", "200");
        
        let mut config = Config::default();
        config.apply_env_overrides();
        
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.simulation.step_interval_ms, 200);
        
        // Clean up
        env::remove_var("ORDERBOOK_PORT");
        env::remove_var("ORDERBOOK_SIMULATION_INTERVAL");
    }
}