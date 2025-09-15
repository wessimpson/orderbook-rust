#!/bin/bash

# Performance benchmarking script for the order book system
# This script runs comprehensive benchmarks and generates performance reports

set -e

echo "🚀 Starting Order Book Performance Benchmarks"
echo "=============================================="

# Check if criterion is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo not found. Please install Rust and Cargo."
    exit 1
fi

# Set up environment
export RUST_LOG=warn  # Reduce log noise during benchmarks
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-target}

# Create benchmark output directory
BENCHMARK_DIR="benchmark_results"
mkdir -p "$BENCHMARK_DIR"

echo "📊 Running Order Book Benchmarks..."
echo "-----------------------------------"

# Run order book benchmarks
echo "🔄 Testing order placement, matching, and cancellation..."
cargo bench --bench orderbook_benchmarks -- --output-format html

echo ""
echo "📈 Running Data Ingestion Benchmarks..."
echo "---------------------------------------"

# Run data ingestion benchmarks
echo "🔄 Testing CSV parsing, seeking, and large file processing..."
cargo bench --bench data_ingestion_benchmarks -- --output-format html

echo ""
echo "🧪 Running Unit Tests with Performance Validation..."
echo "---------------------------------------------------"

# Run tests in release mode for performance validation
cargo test --release --lib -- --nocapture

echo ""
echo "📋 Generating Performance Report..."
echo "----------------------------------"

# Generate a summary report
REPORT_FILE="$BENCHMARK_DIR/performance_summary.md"
cat > "$REPORT_FILE" << EOF
# Order Book Performance Report

Generated on: $(date)
Rust Version: $(rustc --version)
System: $(uname -a)

## Benchmark Results

### Order Book Operations
- **Order Placement**: See \`target/criterion/order_placement/report/index.html\`
- **Order Matching**: See \`target/criterion/order_matching/report/index.html\`
- **Order Cancellation**: See \`target/criterion/order_cancellation/report/index.html\`
- **Snapshot Generation**: See \`target/criterion/snapshot_generation/report/index.html\`

### Data Ingestion
- **CSV Parsing**: See \`target/criterion/csv_parsing/report/index.html\`
- **File Seeking**: See \`target/criterion/csv_seeking/report/index.html\`
- **Large File Processing**: See \`target/criterion/large_file_processing/report/index.html\`

### Memory Efficiency
- **Memory Usage**: See \`target/criterion/memory_efficiency/report/index.html\`
- **Circular Buffer**: Optimized for bounded memory usage
- **Order Pool**: Reduces allocation overhead

## Performance Targets

### Latency Targets (95th percentile)
- Order placement: < 10μs
- Order cancellation: < 5μs
- Snapshot generation: < 100μs
- CSV event parsing: < 1μs

### Throughput Targets
- Order processing: > 100,000 orders/second
- Data ingestion: > 1,000,000 events/second
- WebSocket updates: > 1,000 snapshots/second

### Memory Targets
- Steady-state memory: < 100MB for 100k orders
- Memory growth: < 1MB per 10k additional orders
- GC pressure: Minimal allocations in hot paths

## Optimization Notes

1. **Data Structures**: Using BTreeMap for price levels, VecDeque for FIFO queues
2. **Memory Management**: Circular buffers for time series, object pooling for orders
3. **Serialization**: Efficient JSON serialization for WebSocket updates
4. **Concurrency**: Lock-free where possible, minimal critical sections

## Monitoring

- Prometheus metrics available at http://localhost:3001/metrics
- Health check at http://localhost:3000/health
- Performance dashboard recommended for production monitoring

EOF

echo "✅ Performance benchmarks completed!"
echo ""
echo "📊 Results Summary:"
echo "  - HTML reports: target/criterion/*/report/index.html"
echo "  - Summary report: $REPORT_FILE"
echo "  - Raw data: target/criterion/"
echo ""
echo "🔍 To view detailed results:"
echo "  - Open target/criterion/report/index.html in your browser"
echo "  - Check $REPORT_FILE for summary"
echo ""
echo "💡 Performance Tips:"
echo "  - Run benchmarks on a dedicated machine for consistent results"
echo "  - Disable CPU frequency scaling for stable measurements"
echo "  - Close other applications to reduce system noise"
echo "  - Run multiple times and compare results for reliability"