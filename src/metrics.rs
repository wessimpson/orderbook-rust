use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use metrics::{counter, gauge, histogram};
use sysinfo::{System, SystemExt, CpuExt, ProcessExt};

/// Performance metrics collector for the order book system
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Order processing metrics
    orders_processed: Arc<AtomicU64>,
    orders_failed: Arc<AtomicU64>,
    
    /// Latency tracking
    order_placement_latency: Arc<AtomicU64>,
    order_cancellation_latency: Arc<AtomicU64>,
    snapshot_generation_latency: Arc<AtomicU64>,
    
    /// Throughput metrics
    orders_per_second: Arc<AtomicU64>,
    trades_per_second: Arc<AtomicU64>,
    
    /// Memory usage
    memory_usage_bytes: Arc<AtomicU64>,
    
    /// Data ingestion metrics
    events_ingested: Arc<AtomicU64>,
    ingestion_errors: Arc<AtomicU64>,
    ingestion_rate: Arc<AtomicU64>,
    
    /// System metrics
    cpu_usage_percent: Arc<AtomicU64>,
    
    /// Start time for rate calculations
    start_time: Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMetrics {
    /// Create a new performance metrics collector
    pub fn new() -> Self {
        // Note: Metrics are registered automatically when first used
        
        Self {
            orders_processed: Arc::new(AtomicU64::new(0)),
            orders_failed: Arc::new(AtomicU64::new(0)),
            order_placement_latency: Arc::new(AtomicU64::new(0)),
            order_cancellation_latency: Arc::new(AtomicU64::new(0)),
            snapshot_generation_latency: Arc::new(AtomicU64::new(0)),
            orders_per_second: Arc::new(AtomicU64::new(0)),
            trades_per_second: Arc::new(AtomicU64::new(0)),
            memory_usage_bytes: Arc::new(AtomicU64::new(0)),
            events_ingested: Arc::new(AtomicU64::new(0)),
            ingestion_errors: Arc::new(AtomicU64::new(0)),
            ingestion_rate: Arc::new(AtomicU64::new(0)),
            cpu_usage_percent: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
        }
    }

    /// Record order placement metrics
    pub fn record_order_placement(&self, duration: Duration, success: bool) {
        let duration_ns = duration.as_nanos() as u64;
        
        if success {
            self.orders_processed.fetch_add(1, Ordering::Relaxed);
            counter!("orders_processed_total", 1);
        } else {
            self.orders_failed.fetch_add(1, Ordering::Relaxed);
            counter!("orders_failed_total", 1);
        }
        
        self.order_placement_latency.store(duration_ns, Ordering::Relaxed);
        histogram!("order_placement_duration_ns", duration_ns as f64);
    }

    /// Record order cancellation metrics
    pub fn record_order_cancellation(&self, duration: Duration, success: bool) {
        let duration_ns = duration.as_nanos() as u64;
        
        if success {
            self.orders_processed.fetch_add(1, Ordering::Relaxed);
            counter!("orders_processed_total", 1);
        } else {
            self.orders_failed.fetch_add(1, Ordering::Relaxed);
            counter!("orders_failed_total", 1);
        }
        
        self.order_cancellation_latency.store(duration_ns, Ordering::Relaxed);
        histogram!("order_cancellation_duration_ns", duration_ns as f64);
    }

    /// Record snapshot generation metrics
    pub fn record_snapshot_generation(&self, duration: Duration) {
        let duration_ns = duration.as_nanos() as u64;
        self.snapshot_generation_latency.store(duration_ns, Ordering::Relaxed);
        histogram!("snapshot_generation_duration_ns", duration_ns as f64);
    }

    /// Record trade generation
    pub fn record_trade(&self, count: usize) {
        counter!("trades_generated_total", count as u64);
    }

    /// Record data ingestion metrics
    pub fn record_data_ingestion(&self, duration: Duration, events_count: usize, errors_count: usize) {
        let duration_ns = duration.as_nanos() as u64;
        
        self.events_ingested.fetch_add(events_count as u64, Ordering::Relaxed);
        self.ingestion_errors.fetch_add(errors_count as u64, Ordering::Relaxed);
        
        counter!("events_ingested_total", events_count as u64);
        counter!("ingestion_errors_total", errors_count as u64);
        histogram!("data_ingestion_duration_ns", duration_ns as f64);
    }

    /// Update system metrics (CPU, memory)
    pub fn update_system_metrics(&self, system: &System) {
        // Update CPU usage
        let cpu_usage = system.global_cpu_info().cpu_usage() as u64;
        self.cpu_usage_percent.store(cpu_usage, Ordering::Relaxed);
        gauge!("cpu_usage_percent", cpu_usage as f64);

        // Update memory usage for current process
        if let Some(process) = system.process(sysinfo::get_current_pid().unwrap()) {
            let memory_bytes = process.memory() * 1024; // Convert KB to bytes
            self.memory_usage_bytes.store(memory_bytes, Ordering::Relaxed);
            gauge!("memory_usage_bytes", memory_bytes as f64);
        }
    }

    /// Calculate and update throughput metrics
    pub fn update_throughput_metrics(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            let orders_processed = self.orders_processed.load(Ordering::Relaxed) as f64;
            let orders_per_sec = orders_processed / elapsed;
            self.orders_per_second.store(orders_per_sec as u64, Ordering::Relaxed);
            gauge!("orders_per_second", orders_per_sec);

            let events_ingested = self.events_ingested.load(Ordering::Relaxed) as f64;
            let events_per_sec = events_ingested / elapsed;
            self.ingestion_rate.store(events_per_sec as u64, Ordering::Relaxed);
            gauge!("events_per_second", events_per_sec);
        }
    }

    /// Get current performance snapshot
    pub fn get_snapshot(&self) -> PerformanceSnapshot {
        PerformanceSnapshot {
            orders_processed: self.orders_processed.load(Ordering::Relaxed),
            orders_failed: self.orders_failed.load(Ordering::Relaxed),
            order_placement_latency_ns: self.order_placement_latency.load(Ordering::Relaxed),
            order_cancellation_latency_ns: self.order_cancellation_latency.load(Ordering::Relaxed),
            snapshot_generation_latency_ns: self.snapshot_generation_latency.load(Ordering::Relaxed),
            orders_per_second: self.orders_per_second.load(Ordering::Relaxed),
            trades_per_second: self.trades_per_second.load(Ordering::Relaxed),
            memory_usage_bytes: self.memory_usage_bytes.load(Ordering::Relaxed),
            events_ingested: self.events_ingested.load(Ordering::Relaxed),
            ingestion_errors: self.ingestion_errors.load(Ordering::Relaxed),
            ingestion_rate: self.ingestion_rate.load(Ordering::Relaxed),
            cpu_usage_percent: self.cpu_usage_percent.load(Ordering::Relaxed),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.orders_processed.store(0, Ordering::Relaxed);
        self.orders_failed.store(0, Ordering::Relaxed);
        self.order_placement_latency.store(0, Ordering::Relaxed);
        self.order_cancellation_latency.store(0, Ordering::Relaxed);
        self.snapshot_generation_latency.store(0, Ordering::Relaxed);
        self.orders_per_second.store(0, Ordering::Relaxed);
        self.trades_per_second.store(0, Ordering::Relaxed);
        self.memory_usage_bytes.store(0, Ordering::Relaxed);
        self.events_ingested.store(0, Ordering::Relaxed);
        self.ingestion_errors.store(0, Ordering::Relaxed);
        self.ingestion_rate.store(0, Ordering::Relaxed);
        self.cpu_usage_percent.store(0, Ordering::Relaxed);
    }
}

/// Snapshot of performance metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    pub orders_processed: u64,
    pub orders_failed: u64,
    pub order_placement_latency_ns: u64,
    pub order_cancellation_latency_ns: u64,
    pub snapshot_generation_latency_ns: u64,
    pub orders_per_second: u64,
    pub trades_per_second: u64,
    pub memory_usage_bytes: u64,
    pub events_ingested: u64,
    pub ingestion_errors: u64,
    pub ingestion_rate: u64,
    pub cpu_usage_percent: u64,
    pub uptime_seconds: u64,
}

impl PerformanceSnapshot {
    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.orders_processed + self.orders_failed;
        if total == 0 {
            100.0
        } else {
            (self.orders_processed as f64 / total as f64) * 100.0
        }
    }

    /// Get error rate as a percentage
    pub fn error_rate(&self) -> f64 {
        let total = self.events_ingested + self.ingestion_errors;
        if total == 0 {
            0.0
        } else {
            (self.ingestion_errors as f64 / total as f64) * 100.0
        }
    }

    /// Get memory usage in MB
    pub fn memory_usage_mb(&self) -> f64 {
        self.memory_usage_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Get average order latency in microseconds
    pub fn avg_order_latency_us(&self) -> f64 {
        self.order_placement_latency_ns as f64 / 1000.0
    }

    /// Get average cancellation latency in microseconds
    pub fn avg_cancellation_latency_us(&self) -> f64 {
        self.order_cancellation_latency_ns as f64 / 1000.0
    }

    /// Get average snapshot latency in microseconds
    pub fn avg_snapshot_latency_us(&self) -> f64 {
        self.snapshot_generation_latency_ns as f64 / 1000.0
    }
}

/// Performance monitor that periodically collects system metrics
pub struct PerformanceMonitor {
    metrics: Arc<PerformanceMetrics>,
    system: System,
    update_interval: Duration,
}

impl PerformanceMonitor {
    /// Create a new performance monitor
    pub fn new(metrics: Arc<PerformanceMetrics>) -> Self {
        Self {
            metrics,
            system: System::new_all(),
            update_interval: Duration::from_secs(1),
        }
    }

    /// Set the update interval for system metrics
    pub fn with_update_interval(mut self, interval: Duration) -> Self {
        self.update_interval = interval;
        self
    }

    /// Start monitoring in a background thread
    pub fn start_monitoring(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.update_interval);
            
            loop {
                interval.tick().await;
                
                // Refresh system information
                self.system.refresh_all();
                
                // Update metrics
                self.metrics.update_system_metrics(&self.system);
                self.metrics.update_throughput_metrics();
            }
        })
    }
}

/// Initialize metrics exporter for Prometheus
pub fn init_metrics_exporter(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use metrics_exporter_prometheus::PrometheusBuilder;
    
    let builder = PrometheusBuilder::new();
    let handle = builder
        .with_http_listener(([0, 0, 0, 0], port))
        .install()?;
    
    tracing::info!("Metrics server started on port {}", port);
    
    // Keep the handle alive
    let _ = handle;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_performance_metrics_creation() {
        let metrics = PerformanceMetrics::new();
        let snapshot = metrics.get_snapshot();
        
        assert_eq!(snapshot.orders_processed, 0);
        assert_eq!(snapshot.orders_failed, 0);
        assert_eq!(snapshot.success_rate(), 100.0);
        assert_eq!(snapshot.error_rate(), 0.0);
    }

    #[test]
    fn test_order_placement_recording() {
        let metrics = PerformanceMetrics::new();
        
        // Record successful order
        metrics.record_order_placement(Duration::from_micros(100), true);
        let snapshot = metrics.get_snapshot();
        assert_eq!(snapshot.orders_processed, 1);
        assert_eq!(snapshot.orders_failed, 0);
        assert_eq!(snapshot.order_placement_latency_ns, 100_000);
        
        // Record failed order
        metrics.record_order_placement(Duration::from_micros(200), false);
        let snapshot = metrics.get_snapshot();
        assert_eq!(snapshot.orders_processed, 1);
        assert_eq!(snapshot.orders_failed, 1);
        assert_eq!(snapshot.success_rate(), 50.0);
    }

    #[test]
    fn test_data_ingestion_recording() {
        let metrics = PerformanceMetrics::new();
        
        metrics.record_data_ingestion(Duration::from_millis(10), 100, 5);
        let snapshot = metrics.get_snapshot();
        
        assert_eq!(snapshot.events_ingested, 100);
        assert_eq!(snapshot.ingestion_errors, 5);
        assert!((snapshot.error_rate() - 4.76).abs() < 0.1); // 5/(100+5) = 4.76%
    }

    #[test]
    fn test_performance_snapshot_calculations() {
        let snapshot = PerformanceSnapshot {
            orders_processed: 80,
            orders_failed: 20,
            order_placement_latency_ns: 50_000,
            order_cancellation_latency_ns: 30_000,
            snapshot_generation_latency_ns: 100_000,
            orders_per_second: 1000,
            trades_per_second: 500,
            memory_usage_bytes: 1024 * 1024 * 100, // 100 MB
            events_ingested: 950,
            ingestion_errors: 50,
            ingestion_rate: 2000,
            cpu_usage_percent: 75,
            uptime_seconds: 3600,
        };
        
        assert_eq!(snapshot.success_rate(), 80.0);
        assert_eq!(snapshot.error_rate(), 5.0);
        assert_eq!(snapshot.memory_usage_mb(), 100.0);
        assert_eq!(snapshot.avg_order_latency_us(), 50.0);
        assert_eq!(snapshot.avg_cancellation_latency_us(), 30.0);
        assert_eq!(snapshot.avg_snapshot_latency_us(), 100.0);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = PerformanceMetrics::new();
        
        // Record some data
        metrics.record_order_placement(Duration::from_micros(100), true);
        metrics.record_data_ingestion(Duration::from_millis(10), 100, 5);
        
        // Verify data is recorded
        let snapshot = metrics.get_snapshot();
        assert!(snapshot.orders_processed > 0);
        assert!(snapshot.events_ingested > 0);
        
        // Reset and verify
        metrics.reset();
        let snapshot = metrics.get_snapshot();
        assert_eq!(snapshot.orders_processed, 0);
        assert_eq!(snapshot.events_ingested, 0);
    }
}