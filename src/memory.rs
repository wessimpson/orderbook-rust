use std::collections::VecDeque;

use std::sync::atomic::{AtomicUsize, Ordering};

/// Memory-efficient circular buffer for time series data
#[derive(Debug, Clone)]
pub struct CircularBuffer<T> {
    data: VecDeque<T>,
    max_size: usize,
    total_added: usize,
}

impl<T> CircularBuffer<T> {
    /// Create a new circular buffer with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_size),
            max_size,
            total_added: 0,
        }
    }

    /// Add an item to the buffer, removing oldest if at capacity
    pub fn push(&mut self, item: T) {
        if self.data.len() >= self.max_size {
            self.data.pop_front();
        }
        self.data.push_back(item);
        self.total_added += 1;
    }

    /// Get the current size of the buffer
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the maximum capacity
    pub fn capacity(&self) -> usize {
        self.max_size
    }

    /// Get total number of items ever added
    pub fn total_added(&self) -> usize {
        self.total_added
    }

    /// Get an iterator over the current items
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.data.iter()
    }

    /// Get the most recent item
    pub fn back(&self) -> Option<&T> {
        self.data.back()
    }

    /// Get the oldest item
    pub fn front(&self) -> Option<&T> {
        self.data.front()
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Convert to a vector (for serialization)
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.data.iter().cloned().collect()
    }

    /// Shrink the buffer to a new smaller size
    pub fn shrink_to(&mut self, new_size: usize) {
        if new_size < self.max_size {
            self.max_size = new_size;
            while self.data.len() > new_size {
                self.data.pop_front();
            }
            self.data.shrink_to_fit();
        }
    }
}

/// Memory pool for reusing order objects to reduce allocations
pub struct OrderPool {
    available: VecDeque<crate::types::Order>,
    total_created: AtomicUsize,
    total_reused: AtomicUsize,
}

impl OrderPool {
    /// Create a new order pool
    pub fn new() -> Self {
        Self {
            available: VecDeque::new(),
            total_created: AtomicUsize::new(0),
            total_reused: AtomicUsize::new(0),
        }
    }

    /// Get an order from the pool or create a new one
    pub fn get_order(
        &mut self,
        id: crate::types::OrderId,
        side: crate::types::Side,
        qty: crate::types::Qty,
        order_type: crate::types::OrderType,
        ts: u128,
    ) -> crate::types::Order {
        if let Some(mut order) = self.available.pop_front() {
            // Reuse existing order
            order.id = id;
            order.side = side;
            order.qty = qty;
            order.order_type = order_type;
            order.ts = ts;
            
            self.total_reused.fetch_add(1, Ordering::Relaxed);
            order
        } else {
            // Create new order
            self.total_created.fetch_add(1, Ordering::Relaxed);
            crate::types::Order {
                id,
                side,
                qty,
                order_type,
                ts,
            }
        }
    }

    /// Return an order to the pool for reuse
    pub fn return_order(&mut self, order: crate::types::Order) {
        // Only keep a reasonable number of orders in the pool
        if self.available.len() < 1000 {
            self.available.push_back(order);
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            available: self.available.len(),
            total_created: self.total_created.load(Ordering::Relaxed),
            total_reused: self.total_reused.load(Ordering::Relaxed),
        }
    }

    /// Clear the pool
    pub fn clear(&mut self) {
        self.available.clear();
    }
}

impl Default for OrderPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for memory pools
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub available: usize,
    pub total_created: usize,
    pub total_reused: usize,
}

impl PoolStats {
    /// Calculate reuse rate as a percentage
    pub fn reuse_rate(&self) -> f64 {
        let total_requests = self.total_created + self.total_reused;
        if total_requests == 0 {
            0.0
        } else {
            (self.total_reused as f64 / total_requests as f64) * 100.0
        }
    }
}

/// Memory-efficient string interning for repeated strings
pub struct StringInterner {
    strings: Vec<String>,
    indices: std::collections::HashMap<String, usize>,
}

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            indices: std::collections::HashMap::new(),
        }
    }

    /// Intern a string and return its index
    pub fn intern(&mut self, s: &str) -> usize {
        if let Some(&index) = self.indices.get(s) {
            index
        } else {
            let index = self.strings.len();
            self.strings.push(s.to_string());
            self.indices.insert(s.to_string(), index);
            index
        }
    }

    /// Get a string by its index
    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index).map(|s| s.as_str())
    }

    /// Get the number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the interner is empty
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Clear all interned strings
    pub fn clear(&mut self) {
        self.strings.clear();
        self.indices.clear();
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory usage tracker for monitoring system memory consumption
pub struct MemoryTracker {
    initial_memory: usize,
    peak_memory: AtomicUsize,
}

impl MemoryTracker {
    /// Create a new memory tracker
    pub fn new() -> Self {
        let initial = Self::get_current_memory_usage();
        Self {
            initial_memory: initial,
            peak_memory: AtomicUsize::new(initial),
        }
    }

    /// Update peak memory usage
    pub fn update_peak(&self) {
        let current = Self::get_current_memory_usage();
        let mut peak = self.peak_memory.load(Ordering::Relaxed);
        
        while current > peak {
            match self.peak_memory.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }

    /// Get current memory usage in bytes
    pub fn current_usage(&self) -> usize {
        Self::get_current_memory_usage()
    }

    /// Get peak memory usage in bytes
    pub fn peak_usage(&self) -> usize {
        self.peak_memory.load(Ordering::Relaxed)
    }

    /// Get memory usage since initialization
    pub fn usage_since_init(&self) -> isize {
        Self::get_current_memory_usage() as isize - self.initial_memory as isize
    }

    /// Get current memory usage from the system
    fn get_current_memory_usage() -> usize {
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            if let Ok(contents) = fs::read_to_string("/proc/self/status") {
                for line in contents.lines() {
                    if line.starts_with("VmRSS:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<usize>() {
                                return kb * 1024; // Convert KB to bytes
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("ps")
                .args(&["-o", "rss=", "-p"])
                .arg(std::process::id().to_string())
                .output()
            {
                if let Ok(rss_str) = String::from_utf8(output.stdout) {
                    if let Ok(kb) = rss_str.trim().parse::<usize>() {
                        return kb * 1024; // Convert KB to bytes
                    }
                }
            }
        }

        // Fallback: return 0 if we can't determine memory usage
        0
    }
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circular_buffer() {
        let mut buffer = CircularBuffer::new(3);
        
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.capacity(), 3);
        
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);
        
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.total_added(), 3);
        assert_eq!(buffer.front(), Some(&1));
        assert_eq!(buffer.back(), Some(&3));
        
        // Adding one more should remove the first
        buffer.push(4);
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.total_added(), 4);
        assert_eq!(buffer.front(), Some(&2));
        assert_eq!(buffer.back(), Some(&4));
        
        let vec = buffer.to_vec();
        assert_eq!(vec, vec![2, 3, 4]);
    }

    #[test]
    fn test_circular_buffer_shrink() {
        let mut buffer = CircularBuffer::new(5);
        
        for i in 1..=5 {
            buffer.push(i);
        }
        
        assert_eq!(buffer.len(), 5);
        
        buffer.shrink_to(3);
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.capacity(), 3);
        assert_eq!(buffer.to_vec(), vec![3, 4, 5]);
    }

    #[test]
    fn test_order_pool() {
        let mut pool = OrderPool::new();
        
        let order1 = pool.get_order(
            1,
            crate::types::Side::Buy,
            100,
            crate::types::OrderType::Market,
            12345,
        );
        
        let stats = pool.stats();
        assert_eq!(stats.total_created, 1);
        assert_eq!(stats.total_reused, 0);
        assert_eq!(stats.available, 0);
        
        pool.return_order(order1);
        let stats = pool.stats();
        assert_eq!(stats.available, 1);
        
        let order2 = pool.get_order(
            2,
            crate::types::Side::Sell,
            200,
            crate::types::OrderType::Market,
            12346,
        );
        
        let stats = pool.stats();
        assert_eq!(stats.total_created, 1);
        assert_eq!(stats.total_reused, 1);
        assert_eq!(stats.available, 0);
        assert_eq!(stats.reuse_rate(), 50.0);
        
        assert_eq!(order2.id, 2);
        assert_eq!(order2.qty, 200);
    }

    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();
        
        assert!(interner.is_empty());
        
        let index1 = interner.intern("hello");
        let index2 = interner.intern("world");
        let index3 = interner.intern("hello"); // Should reuse
        
        assert_eq!(index1, index3);
        assert_ne!(index1, index2);
        assert_eq!(interner.len(), 2);
        
        assert_eq!(interner.get(index1), Some("hello"));
        assert_eq!(interner.get(index2), Some("world"));
        assert_eq!(interner.get(999), None);
    }

    #[test]
    fn test_memory_tracker() {
        let tracker = MemoryTracker::new();
        
        // Test that we can get memory measurements (may be 0 on some systems)
        let initial = tracker.current_usage();
        let peak = tracker.peak_usage();
        
        // Peak should initially be the same as initial (or both 0 if tracking unavailable)
        if initial == 0 && peak == 0 {
            // Memory tracking not available on this system, skip detailed checks
            println!("Memory tracking not available on this system");
            return;
        }
        
        // Allocate some memory to potentially increase usage
        let _large_vec: Vec<u8> = vec![0; 1024 * 1024]; // 1MB allocation
        
        // Update peak after allocation
        tracker.update_peak();
        let new_peak = tracker.peak_usage();
        let current_after_alloc = tracker.current_usage();
        
        // New peak should be >= old peak (monotonic increase)
        assert!(new_peak >= peak, "New peak ({}) should be >= old peak ({})", new_peak, peak);
        
        // Test usage since init calculation
        let usage_diff = tracker.usage_since_init();
        // This can be positive, negative, or zero depending on memory allocation patterns
        // Just ensure it's a reasonable value (not extremely large)
        assert!(usage_diff.abs() < 1_000_000_000, "Usage difference should be reasonable: {}", usage_diff);
        
        // Test that current usage is reasonable
        assert!(current_after_alloc < 10_000_000_000, "Current usage should be reasonable: {}", current_after_alloc);
    }
}