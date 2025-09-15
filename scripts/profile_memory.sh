#!/bin/bash

# Memory profiling script for the order book system
# This script helps identify memory usage patterns and potential leaks

set -e

echo "🧠 Memory Profiling for Order Book System"
echo "========================================="

# Check dependencies
if ! command -v valgrind &> /dev/null && ! command -v heaptrack &> /dev/null; then
    echo "⚠️  No memory profiling tools found."
    echo "   Install valgrind or heaptrack for detailed memory analysis."
    echo "   Falling back to basic memory monitoring..."
fi

# Build in release mode with debug symbols
echo "🔨 Building with debug symbols..."
RUSTFLAGS="-g" cargo build --release --bin serve

echo ""
echo "📊 Running Memory Usage Tests..."
echo "-------------------------------"

# Create test data
echo "📝 Creating test CSV data..."
cat > /tmp/test_market_data.csv << EOF
event_type,timestamp,price,qty,side,trade_id
trade,1640995200000000000,10000,100,buy,1
trade,1640995200001000000,10005,200,sell,2
quote,1640995200002000000,9995,10005,500,300
order,1640995200003000000,1,buy,150,9990,limit
order,1640995200004000000,2,sell,100,10010,limit
cancel,1640995200005000000,1,,user_cancel
EOF

# Function to monitor memory usage
monitor_memory() {
    local pid=$1
    local output_file=$2
    
    echo "timestamp,rss_kb,vsz_kb,cpu_percent" > "$output_file"
    
    while kill -0 "$pid" 2>/dev/null; do
        local stats=$(ps -o pid,rss,vsz,pcpu -p "$pid" --no-headers 2>/dev/null || echo "")
        if [ -n "$stats" ]; then
            local timestamp=$(date +%s)
            local rss=$(echo "$stats" | awk '{print $2}')
            local vsz=$(echo "$stats" | awk '{print $3}')
            local cpu=$(echo "$stats" | awk '{print $4}')
            echo "$timestamp,$rss,$vsz,$cpu" >> "$output_file"
        fi
        sleep 0.1
    done
}

# Test 1: Basic server startup memory usage
echo "🧪 Test 1: Server startup memory usage"
MEMORY_LOG="memory_startup.csv"

# Start server in background
RUST_LOG=warn timeout 10s ./target/release/serve --port 3000 --csv-file /tmp/test_market_data.csv &
SERVER_PID=$!

# Monitor memory usage
monitor_memory $SERVER_PID "$MEMORY_LOG" &
MONITOR_PID=$!

# Wait for server to finish or timeout
wait $SERVER_PID 2>/dev/null || true
kill $MONITOR_PID 2>/dev/null || true

if [ -f "$MEMORY_LOG" ]; then
    echo "📈 Startup memory usage:"
    echo "  Initial RSS: $(head -2 "$MEMORY_LOG" | tail -1 | cut -d',' -f2) KB"
    echo "  Peak RSS: $(tail -n +2 "$MEMORY_LOG" | cut -d',' -f2 | sort -n | tail -1) KB"
    echo "  Final RSS: $(tail -1 "$MEMORY_LOG" | cut -d',' -f2) KB"
fi

# Test 2: Memory usage under load
echo ""
echo "🧪 Test 2: Memory usage under simulated load"

# Create larger test data
echo "📝 Creating larger test dataset..."
cat > /tmp/large_test_data.csv << 'EOF'
event_type,timestamp,price,qty,side,trade_id
EOF

# Generate 10000 events
for i in $(seq 1 10000); do
    timestamp=$((1640995200000000000 + i * 1000000))
    price=$((10000 + (i % 100) * 10))
    qty=$((100 + (i % 500)))
    side=$([ $((i % 2)) -eq 0 ] && echo "buy" || echo "sell")
    echo "trade,$timestamp,$price,$qty,$side,$i" >> /tmp/large_test_data.csv
done

MEMORY_LOG_LOAD="memory_load.csv"

# Start server with larger dataset
RUST_LOG=warn timeout 30s ./target/release/serve --port 3000 --csv-file /tmp/large_test_data.csv &
SERVER_PID=$!

# Monitor memory usage
monitor_memory $SERVER_PID "$MEMORY_LOG_LOAD" &
MONITOR_PID=$!

# Wait for server to finish
wait $SERVER_PID 2>/dev/null || true
kill $MONITOR_PID 2>/dev/null || true

if [ -f "$MEMORY_LOG_LOAD" ]; then
    echo "📈 Load test memory usage:"
    echo "  Initial RSS: $(head -2 "$MEMORY_LOG_LOAD" | tail -1 | cut -d',' -f2) KB"
    echo "  Peak RSS: $(tail -n +2 "$MEMORY_LOG_LOAD" | cut -d',' -f2 | sort -n | tail -1) KB"
    echo "  Final RSS: $(tail -1 "$MEMORY_LOG_LOAD" | cut -d',' -f2) KB"
fi

# Test 3: Memory leak detection (if valgrind is available)
if command -v valgrind &> /dev/null; then
    echo ""
    echo "🧪 Test 3: Memory leak detection with Valgrind"
    echo "⚠️  This may take several minutes..."
    
    timeout 60s valgrind \
        --tool=memcheck \
        --leak-check=full \
        --show-leak-kinds=all \
        --track-origins=yes \
        --log-file=valgrind_report.txt \
        ./target/release/serve --port 3000 --csv-file /tmp/test_market_data.csv \
        2>/dev/null || true
    
    if [ -f valgrind_report.txt ]; then
        echo "📋 Valgrind report summary:"
        grep -E "(definitely lost|indirectly lost|possibly lost)" valgrind_report.txt || echo "  No memory leaks detected!"
        echo "  Full report: valgrind_report.txt"
    fi
fi

# Generate memory usage report
echo ""
echo "📋 Generating Memory Usage Report..."
echo "-----------------------------------"

REPORT_FILE="memory_profile_report.md"
cat > "$REPORT_FILE" << EOF
# Memory Profile Report

Generated on: $(date)
System: $(uname -a)
Rust Version: $(rustc --version)

## Memory Usage Summary

### Startup Test
$(if [ -f "$MEMORY_LOG" ]; then
    echo "- Initial RSS: $(head -2 "$MEMORY_LOG" | tail -1 | cut -d',' -f2) KB"
    echo "- Peak RSS: $(tail -n +2 "$MEMORY_LOG" | cut -d',' -f2 | sort -n | tail -1) KB"
    echo "- Final RSS: $(tail -1 "$MEMORY_LOG" | cut -d',' -f2) KB"
else
    echo "- Data not available"
fi)

### Load Test (10k events)
$(if [ -f "$MEMORY_LOG_LOAD" ]; then
    echo "- Initial RSS: $(head -2 "$MEMORY_LOG_LOAD" | tail -1 | cut -d',' -f2) KB"
    echo "- Peak RSS: $(tail -n +2 "$MEMORY_LOG_LOAD" | cut -d',' -f2 | sort -n | tail -1) KB"
    echo "- Final RSS: $(tail -1 "$MEMORY_LOG_LOAD" | cut -d',' -f2) KB"
else
    echo "- Data not available"
fi)

### Memory Leak Analysis
$(if [ -f valgrind_report.txt ]; then
    echo "- Valgrind analysis completed"
    echo "- Leaks: $(grep -c "definitely lost" valgrind_report.txt || echo "0") definite, $(grep -c "possibly lost" valgrind_report.txt || echo "0") possible"
else
    echo "- Valgrind analysis not performed"
fi)

## Optimization Recommendations

1. **Circular Buffers**: Implemented for bounded memory usage in time series data
2. **Object Pooling**: Consider implementing for high-frequency order objects
3. **Memory Mapping**: For large CSV files, consider memory-mapped I/O
4. **Garbage Collection**: Monitor for excessive allocations in hot paths

## Memory Targets

- **Startup**: < 50MB RSS
- **Per 10k orders**: < 100MB additional RSS
- **Memory leaks**: Zero definite leaks
- **Growth rate**: Linear with data size, not time

## Files Generated

- \`$MEMORY_LOG\`: Startup memory usage data
- \`$MEMORY_LOG_LOAD\`: Load test memory usage data
- \`valgrind_report.txt\`: Detailed leak analysis (if available)

EOF

echo "✅ Memory profiling completed!"
echo ""
echo "📊 Results Summary:"
echo "  - Memory report: $REPORT_FILE"
echo "  - Raw data: $MEMORY_LOG, $MEMORY_LOG_LOAD"
if [ -f valgrind_report.txt ]; then
    echo "  - Leak analysis: valgrind_report.txt"
fi
echo ""
echo "💡 Memory Optimization Tips:"
echo "  - Use circular buffers for bounded growth"
echo "  - Implement object pooling for frequent allocations"
echo "  - Monitor RSS growth over time in production"
echo "  - Profile with heaptrack for detailed allocation analysis"

# Cleanup
rm -f /tmp/test_market_data.csv /tmp/large_test_data.csv