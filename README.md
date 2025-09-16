# High-Performance Order Book Engine 🚀

A production-grade, high-performance order book implementation written in Rust, designed for financial trading systems and market microstructure research. This system demonstrates advanced systems programming concepts, real-time data processing, and financial market mechanics.

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Performance](https://img.shields.io/badge/Performance-100k%2B%20orders%2Fsec-green.svg)](#performance-metrics)

## 🎯 Overview

This order book engine is built from the ground up to handle high-frequency trading scenarios with microsecond-level latencies. It features a modular architecture with pluggable matching algorithms, comprehensive performance monitoring, and real-time market data streaming capabilities.

### Key Highlights

- **Ultra-Low Latency**: Order placement < 10μs (95th percentile)
- **High Throughput**: 100,000+ orders/second processing capability
- **Memory Efficient**: Bounded memory usage with circular buffers
- **Modular Design**: Pluggable queue disciplines and data sources
- **Production Ready**: Comprehensive monitoring, logging, and error handling
- **Educational**: Well-documented code demonstrating financial concepts

## 🏗️ Architecture

### Core Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Data Sources  │    │  Order Book     │    │   WebSocket     │
│                 │    │    Engine       │    │    Server       │
│ • CSV Files     │───▶│                 │───▶│                 │
│ • JSON Streams  │    │ • FIFO Matching │    │ • Real-time     │
│ • Binary Data   │    │ • Price Levels  │    │   Broadcasting  │
│ • Live Feeds    │    │ • Order Index   │    │ • Health Checks │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Simulation    │    │   Performance   │    │    Monitoring   │
│                 │    │    Metrics      │    │                 │
│ • Market Making │    │                 │    │ • Prometheus    │
│ • Order Flow    │    │ • Latency       │    │ • Grafana       │
│ • PnL Tracking  │    │ • Throughput    │    │ • Alerting      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Design Philosophy

The system is built around several key principles:

1. **Performance First**: Every component is optimized for low-latency, high-throughput operations
2. **Modularity**: Trait-based abstractions allow easy extension and testing
3. **Observability**: Comprehensive metrics and logging for production deployment
4. **Correctness**: Extensive validation and error handling ensure data integrity
5. **Educational Value**: Clear, well-documented code that demonstrates financial concepts

## 🔧 Core Features

### Order Book Engine

The heart of the system is a generic order book implementation that supports multiple matching algorithms:

```rust
pub trait OrderBookEngine {
    fn place(&mut self, order: Order) -> EngineResult<Vec<Trade>>;
    fn cancel(&mut self, order_id: OrderId) -> EngineResult<Qty>;
    fn best_bid(&self) -> Option<Price>;
    fn best_ask(&self) -> Option<Price>;
    fn depth_at(&self, side: Side, price: Price) -> Qty;
    fn snapshot(&self) -> DepthSnapshot;
}
```

**Key Features:**
- **Price-Time Priority**: Orders matched based on price improvement and time priority
- **Efficient Data Structures**: BTreeMap for price levels, HashMap for O(1) order lookup
- **Memory Management**: Circular buffers prevent unbounded memory growth
- **Validation**: Comprehensive order validation with configurable limits

### Queue Disciplines

Pluggable matching algorithms through the `QueueDiscipline` trait:

```rust
pub trait QueueDiscipline {
    fn enqueue(&mut self, order: Order);
    fn match_against(&mut self, taker_id: OrderId, taker_side: Side, 
                     taker_qty: Qty, price: Price) -> (Qty, Vec<Trade>);
    fn cancel(&mut self, order_id: OrderId) -> Qty;
    fn total_qty(&self) -> Qty;
    fn is_empty(&self) -> bool;
}
```

**Current Implementations:**
- **FIFO (First-In-First-Out)**: Standard time priority matching
- **Pro-Rata**: Proportional allocation (planned)
- **Size-Time Priority**: Hybrid approaches (planned)

### Data Ingestion System

Flexible data source abstraction supporting multiple formats:

```rust
pub trait DataSource {
    fn next_event(&mut self) -> DataResult<Option<MarketEvent>>;
    fn seek_to_time(&mut self, timestamp: u128) -> DataResult<()>;
    fn set_playback_speed(&mut self, multiplier: f64) -> DataResult<()>;
    fn is_finished(&self) -> bool;
}
```

**Supported Formats:**
- **CSV**: Standard market data files with configurable schemas
- **JSON**: Structured market events with schema validation
- **Binary**: High-performance custom format (planned)
- **Live Feeds**: Real-time market data integration (planned)

### Market Simulation

Realistic market simulation for testing and demonstration:

- **Market Making**: Automated liquidity provision with configurable spreads
- **Order Flow Generation**: Realistic order arrival patterns
- **Network Simulation**: Latency and packet loss modeling
- **PnL Tracking**: Real-time profit and loss calculation

### Real-Time Streaming

WebSocket server for real-time market data distribution:

- **Snapshot Broadcasting**: Real-time order book snapshots
- **Health Monitoring**: System health and performance metrics
- **Connection Management**: Robust connection handling with reconnection
- **Message Validation**: Input validation and rate limiting

## 🚀 Performance Metrics

### Benchmark Results

The system has been extensively benchmarked using Criterion.rs:

| Operation | Latency (95th percentile) | Throughput |
|-----------|---------------------------|------------|
| Order Placement | < 10μs | 100,000+ ops/sec |
| Order Cancellation | < 5μs | 200,000+ ops/sec |
| Snapshot Generation | < 100μs | 10,000+ snapshots/sec |
| CSV Event Parsing | < 1μs | 1,000,000+ events/sec |

### Memory Efficiency

- **Bounded Growth**: Circular buffers prevent memory leaks
- **Efficient Cleanup**: Automatic removal of empty price levels
- **Object Pooling**: Reuse of frequently allocated objects (planned)
- **Memory Tracking**: Real-time memory usage monitoring

### Optimization Techniques

1. **Data Structure Selection**:
   - `BTreeMap` for O(log n) price-ordered access
   - `VecDeque` for O(1) FIFO queue operations
   - `HashMap` for O(1) order lookup during cancellations

2. **Memory Management**:
   - Minimal allocations in hot paths
   - Circular buffers for time series data
   - Efficient serialization for network transmission

3. **Algorithmic Optimizations**:
   - Price level aggregation for market data
   - Batch processing for bulk operations
   - Lazy evaluation for expensive computations

## 📊 Monitoring and Observability

### Performance Metrics

Comprehensive metrics collection using the `metrics` crate:

```rust
pub struct PerformanceMetrics {
    pub order_placement_latency: Histogram,
    pub order_cancellation_latency: Histogram,
    pub snapshot_generation_latency: Histogram,
    pub throughput_counter: Counter,
    pub error_counter: Counter,
}
```

### Prometheus Integration

- **Metrics Export**: Prometheus-compatible metrics on configurable port
- **Custom Metrics**: Business-specific KPIs (spread, volume, PnL)
- **Alerting Rules**: Pre-configured alerts for performance degradation
- **Grafana Dashboard**: Ready-to-use visualization dashboard

### Logging

Structured logging using the `tracing` crate:

- **Contextual Logging**: Request IDs and correlation tracking
- **Performance Logging**: Detailed timing information
- **Error Tracking**: Comprehensive error reporting with stack traces
- **Audit Trail**: Complete order lifecycle logging

## 🛠️ Getting Started

### Prerequisites

- Rust 1.70+ (latest stable recommended)
- Cargo (comes with Rust)

### Quick Start

```bash
# Clone the repository
git clone https://github.com/yourusername/orderbook-engine.git
cd orderbook-engine

# Build the project
cargo build --release

# Run the server
cargo run --bin serve --release

# Run benchmarks
cargo bench

# Run tests
cargo test
```

### Configuration

The system supports configuration through environment variables and TOML files:

```toml
[server]
port = 3000
simulation_interval_ms = 100

[logging]
level = "info"
format = "json"

[metrics]
enabled = true
port = 3001
```

### Example Usage

```rust
use orderbook::*;

// Create an order book with FIFO matching
let mut book = OrderBook::<FifoLevel>::new();

// Place a limit order
let order = Order::new_limit(1, Side::Buy, 100, price_utils::from_f64(50.25), now_ns());
let trades = book.place(order)?;

// Generate market data snapshot
let snapshot = book.snapshot();
println!("Best bid: {:?}, Best ask: {:?}", snapshot.best_bid, snapshot.best_ask);
```

## 📈 Data Sources and Examples

### CSV Data Replay

```bash
# Run the CSV replay example
cargo run --example csv_replay -- sample_data.csv

# With custom playback speed
cargo run --example csv_replay -- --speed 2.0 sample_data.csv
```

### Market Simulation

```bash
# Run market simulation demo
cargo run --example simulation_demo

# With custom parameters
cargo run --example simulation_demo -- --market-makers 5 --order-rate 1000
```

## 🧪 Testing

### Unit Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test module
cargo test engine::tests
```

### Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench orderbook_benchmarks

# Generate HTML reports
cargo bench --bench orderbook_benchmarks -- --output-format html
```

### Memory Profiling

```bash
# Run memory profiling script
./scripts/profile_memory.sh

# View memory usage report
cat memory_profile_report.md
```

## 🔍 Code Structure

```
src/
├── lib.rs              # Public API and re-exports
├── types.rs            # Core type definitions
├── engine.rs           # Order book engine implementation
├── queue.rs            # Queue discipline trait
├── queue_fifo.rs       # FIFO queue implementation
├── data.rs             # Data ingestion system
├── sim.rs              # Market simulation
├── server.rs           # WebSocket server
├── metrics.rs          # Performance monitoring
├── memory.rs           # Memory management utilities
├── logging.rs          # Structured logging
├── time.rs             # Time utilities
├── error.rs            # Error types and handling
└── config.rs           # Configuration management

benches/
├── orderbook_benchmarks.rs     # Core engine benchmarks
└── data_ingestion_benchmarks.rs # Data processing benchmarks

examples/
├── csv_replay.rs       # CSV data replay example
├── simulation_demo.rs  # Market simulation example
└── data_formats.rs     # Data format examples

scripts/
├── run_benchmarks.sh   # Benchmark automation
└── profile_memory.sh   # Memory profiling
```

## 🎓 Educational Value

This project serves as an excellent learning resource for:

### Systems Programming Concepts

- **Memory Management**: Efficient allocation strategies and lifetime management
- **Concurrency**: Thread-safe data structures and async programming
- **Performance Optimization**: Profiling, benchmarking, and optimization techniques
- **Error Handling**: Robust error propagation and recovery strategies

### Financial Technology

- **Market Microstructure**: Order book mechanics and price discovery
- **Trading Systems**: Order lifecycle and matching algorithms
- **Risk Management**: Position tracking and PnL calculation
- **Market Data**: Real-time data processing and distribution

### Software Architecture

- **Trait-Based Design**: Flexible abstractions and polymorphism
- **Modular Architecture**: Separation of concerns and dependency injection
- **Testing Strategies**: Unit testing, integration testing, and benchmarking
- **Observability**: Metrics, logging, and monitoring best practices

## 🚀 Performance Tuning

### For High-Frequency Trading

1. **Compiler Optimizations**:
   ```bash
   RUSTFLAGS="-C target-cpu=native -C opt-level=3" cargo build --release
   ```

2. **System Tuning**:
   - Pin CPU cores to avoid context switching
   - Increase system limits (file descriptors, memory)
   - Use dedicated hardware with low-latency networking

3. **Memory Optimization**:
   - Increase circular buffer sizes for longer history
   - Use memory-mapped files for large datasets
   - Consider custom allocators for specific workloads

### For Large Datasets

1. **Data Processing**:
   - Implement data compression for storage
   - Use streaming processing for large files
   - Consider sharding across multiple instances

2. **Monitoring**:
   - Set up comprehensive alerting
   - Implement graceful degradation under load
   - Regular performance regression testing

## 🤝 Contributing

Contributions are welcome! Areas for improvement include:

- **Additional Queue Disciplines**: Pro-rata, size-time priority
- **Data Sources**: Binary formats, live feed integration
- **Performance**: SIMD optimizations, custom allocators
- **Features**: Order modification, iceberg orders, stop orders
- **Testing**: Property-based testing, fuzzing

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Rust community for excellent tooling and libraries
- Financial industry professionals for domain expertise
- Open source contributors for inspiration and best practices

---

**Built with ❤️ in Rust for the financial technology community**