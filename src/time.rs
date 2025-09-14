use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp in nanoseconds since Unix epoch
pub fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos()
}

/// Convert milliseconds to nanoseconds
pub fn ms_to_ns(ms: u64) -> u128 {
    ms as u128 * 1_000_000
}

/// Convert nanoseconds to milliseconds
pub fn ns_to_ms(ns: u128) -> u64 {
    (ns / 1_000_000) as u64
}

/// Convert nanoseconds to seconds as f64
pub fn ns_to_secs(ns: u128) -> f64 {
    ns as f64 / 1_000_000_000.0
}

/// Convert seconds to nanoseconds
pub fn secs_to_ns(secs: f64) -> u128 {
    (secs * 1_000_000_000.0) as u128
}

/// Calculate elapsed time in nanoseconds between two timestamps
pub fn elapsed_ns(start: u128, end: u128) -> u128 {
    end.saturating_sub(start)
}

/// Format nanosecond timestamp as human-readable string
pub fn format_ns(ns: u128) -> String {
    let secs = ns_to_secs(ns);
    format!("{:.9}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_conversions() {
        let ms = 1000u64;
        let ns = ms_to_ns(ms);
        assert_eq!(ns, 1_000_000_000);
        assert_eq!(ns_to_ms(ns), ms);

        let secs = 1.5f64;
        let ns = secs_to_ns(secs);
        assert_eq!(ns, 1_500_000_000);
        assert_eq!(ns_to_secs(ns), secs);
    }

    #[test]
    fn test_elapsed_calculation() {
        let start = 1_000_000_000u128;
        let end = 1_500_000_000u128;
        assert_eq!(elapsed_ns(start, end), 500_000_000);
        
        // Test saturation on underflow
        assert_eq!(elapsed_ns(end, start), 0);
    }

    #[test]
    fn test_now_ns() {
        let ts1 = now_ns();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ts2 = now_ns();
        assert!(ts2 > ts1);
        assert!(elapsed_ns(ts1, ts2) > 0);
    }

    #[test]
    fn test_format_ns() {
        let ns = 1_500_000_000u128; // 1.5 seconds
        let formatted = format_ns(ns);
        assert!(formatted.contains("1.500000000"));
    }
}