use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use orderbook::data::{CsvDataSource, DataSource, MarketEvent};
use orderbook::types::{Side, Price, price_utils};
use std::io::Write;
use tempfile::NamedTempFile;

fn create_sample_csv_data(num_events: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    
    // Write CSV header
    writeln!(file, "event_type,timestamp,price,qty,side,trade_id").unwrap();
    
    let base_timestamp = 1640995200000000000u128; // 2022-01-01 00:00:00 UTC in nanoseconds
    let base_price = price_utils::from_f64(100.0);
    
    for i in 0..num_events {
        let timestamp = base_timestamp + (i as u128 * 1000000); // 1ms intervals
        let event_type = match i % 4 {
            0 => "trade",
            1 => "quote", 
            2 => "order",
            _ => "cancel",
        };
        
        match event_type {
            "trade" => {
                let price = base_price + ((i % 100) as Price * 10);
                let qty = 100 + (i % 500) as u64;
                let side = if i % 2 == 0 { "buy" } else { "sell" };
                writeln!(file, "trade,{},{},{},{},trade_{}", timestamp, price, qty, side, i).unwrap();
            }
            "quote" => {
                let bid = base_price - 50 + ((i % 50) as Price * 5);
                let ask = base_price + 50 + ((i % 50) as Price * 5);
                let bid_qty = 100 + (i % 200) as u64;
                let ask_qty = 100 + ((i + 1) % 200) as u64;
                writeln!(file, "quote,{},{},{},{},{}", timestamp, bid, ask, bid_qty, ask_qty).unwrap();
            }
            "order" => {
                let price = base_price + ((i % 200) as Price * 5);
                let qty = 50 + (i % 300) as u64;
                let side = if i % 3 == 0 { "buy" } else { "sell" };
                writeln!(file, "order,{},{},{},{},{},limit", timestamp, i, side, qty, price).unwrap();
            }
            "cancel" => {
                let order_id = if i > 10 { i - 10 } else { i };
                writeln!(file, "cancel,{},{},user_cancel", timestamp, order_id).unwrap();
            }
            _ => unreachable!(),
        }
    }
    
    file.flush().unwrap();
    file
}

fn bench_csv_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("csv_parsing");
    
    for num_events in [1000, 10000, 100000].iter() {
        group.throughput(Throughput::Elements(*num_events as u64));
        
        group.bench_with_input(
            BenchmarkId::new("parse_events", num_events),
            num_events,
            |b, &num_events| {
                b.iter_batched(
                    || {
                        let csv_file = create_sample_csv_data(num_events);
                        CsvDataSource::new(csv_file.path()).unwrap()
                    },
                    |mut data_source| {
                        let mut events = Vec::new();
                        
                        while let Ok(Some(event)) = data_source.next_event() {
                            events.push(event);
                        }
                        
                        black_box(events)
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_csv_seeking(c: &mut Criterion) {
    let mut group = c.benchmark_group("csv_seeking");
    
    let num_events = 50000;
    let csv_file = create_sample_csv_data(num_events);
    let base_timestamp = 1640995200000000000u128;
    
    group.bench_function("seek_to_middle", |b| {
        b.iter_batched(
            || CsvDataSource::new(csv_file.path()).unwrap(),
            |mut data_source| {
                let target_timestamp = base_timestamp + (num_events as u128 / 2 * 1000000);
                black_box(data_source.seek_to_time(target_timestamp).unwrap())
            },
            criterion::BatchSize::SmallInput,
        );
    });
    
    group.bench_function("seek_to_end", |b| {
        b.iter_batched(
            || CsvDataSource::new(csv_file.path()).unwrap(),
            |mut data_source| {
                let target_timestamp = base_timestamp + (num_events as u128 * 1000000);
                let _ = data_source.seek_to_time(target_timestamp); // May fail if timestamp not found
                black_box(())
            },
            criterion::BatchSize::SmallInput,
        );
    });
    
    group.finish();
}

fn bench_playback_speed_control(c: &mut Criterion) {
    let mut group = c.benchmark_group("playback_speed");
    
    let num_events = 1000;
    
    for speed in [0.1, 1.0, 2.0, 10.0].iter() {
        group.bench_with_input(
            BenchmarkId::new("speed_control", format!("{}x", speed)),
            speed,
            |b, &speed| {
                b.iter_batched(
                    || {
                        let csv_file = create_sample_csv_data(num_events);
                        let mut data_source = CsvDataSource::new(csv_file.path()).unwrap();
                        data_source.set_playback_speed(speed).unwrap();
                        data_source
                    },
                    |mut data_source| {
                        let mut events = Vec::new();
                        let start = std::time::Instant::now();
                        
                        // Read first 100 events to test timing
                        for _ in 0..100 {
                            if let Ok(Some(event)) = data_source.next_event() {
                                events.push(event);
                            } else {
                                break;
                            }
                        }
                        
                        let elapsed = start.elapsed();
                        black_box((events, elapsed))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_event_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_validation");
    
    // Create different types of events for validation testing
    let events = vec![
        MarketEvent::Trade {
            price: price_utils::from_f64(100.0),
            qty: 100,
            side: Side::Buy,
            timestamp: 1640995200000000000,
            trade_id: Some("trade_1".to_string()),
        },
        MarketEvent::Quote {
            bid: Some(price_utils::from_f64(99.95)),
            ask: Some(price_utils::from_f64(100.05)),
            bid_qty: Some(500),
            ask_qty: Some(300),
            timestamp: 1640995200000000000,
        },
        MarketEvent::OrderPlacement(orderbook::types::Order::new_limit(
            1,
            Side::Buy,
            100,
            price_utils::from_f64(99.90),
            1640995200000000000,
        )),
        MarketEvent::OrderCancellation {
            order_id: 1,
            timestamp: 1640995200000000000,
            reason: Some("user_cancel".to_string()),
        },
    ];
    
    group.throughput(Throughput::Elements(events.len() as u64));
    
    group.bench_function("validate_events", |b| {
        b.iter(|| {
            for event in &events {
                black_box(event.validate().unwrap());
            }
        });
    });
    
    group.finish();
}

fn bench_large_file_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_file_processing");
    
    // Test with a very large CSV file
    let num_events = 1000000; // 1 million events
    
    group.sample_size(10); // Reduce sample size for large benchmarks
    group.measurement_time(std::time::Duration::from_secs(30));
    
    group.bench_function("process_million_events", |b| {
        b.iter_batched(
            || {
                let csv_file = create_sample_csv_data(num_events);
                CsvDataSource::new(csv_file.path()).unwrap()
            },
            |mut data_source| {
                let mut event_count = 0;
                let mut trade_count = 0;
                let mut quote_count = 0;
                let mut order_count = 0;
                let mut cancel_count = 0;
                
                while let Ok(Some(event)) = data_source.next_event() {
                    event_count += 1;
                    
                    match event {
                        MarketEvent::Trade { .. } => trade_count += 1,
                        MarketEvent::Quote { .. } => quote_count += 1,
                        MarketEvent::OrderPlacement(_) => order_count += 1,
                        MarketEvent::OrderCancellation { .. } => cancel_count += 1,
                        _ => {}
                    }
                }
                
                black_box((event_count, trade_count, quote_count, order_count, cancel_count))
            },
            criterion::BatchSize::LargeInput,
        );
    });
    
    group.finish();
}

fn bench_memory_usage_during_ingestion(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    group.bench_function("streaming_vs_batch", |b| {
        b.iter_batched(
            || {
                let num_events = 100000;
                let csv_file = create_sample_csv_data(num_events);
                CsvDataSource::new(csv_file.path()).unwrap()
            },
            |mut data_source| {
                // Test streaming processing (should use constant memory)
                let mut processed = 0;
                let mut last_timestamp = 0u128;
                
                while let Ok(Some(event)) = data_source.next_event() {
                    processed += 1;
                    last_timestamp = event.timestamp();
                    
                    // Process event immediately without storing
                    match event {
                        MarketEvent::Trade { price, qty, .. } => {
                            // Simulate processing
                            let _ = price * qty as u64;
                        }
                        MarketEvent::Quote { bid, ask, .. } => {
                            // Simulate spread calculation
                            if let (Some(b), Some(a)) = (bid, ask) {
                                let _ = a - b;
                            }
                        }
                        _ => {}
                    }
                }
                
                black_box((processed, last_timestamp))
            },
            criterion::BatchSize::LargeInput,
        );
    });
    
    group.finish();
}

fn bench_concurrent_data_sources(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_processing");
    
    group.bench_function("multiple_csv_sources", |b| {
        b.iter_batched(
            || {
                // Create multiple CSV files
                let files: Vec<_> = (0..4)
                    .map(|_i| {
                        let csv_file = create_sample_csv_data(10000);
                        CsvDataSource::new(csv_file.path()).unwrap()
                    })
                    .collect();
                files
            },
            |mut data_sources| {
                let mut total_events = 0;
                
                // Process all sources in round-robin fashion
                loop {
                    let mut any_active = false;
                    
                    for data_source in &mut data_sources {
                        if let Ok(Some(_event)) = data_source.next_event() {
                            total_events += 1;
                            any_active = true;
                        }
                    }
                    
                    if !any_active {
                        break;
                    }
                }
                
                black_box(total_events)
            },
            criterion::BatchSize::LargeInput,
        );
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_csv_parsing,
    bench_csv_seeking,
    bench_playback_speed_control,
    bench_event_validation,
    bench_large_file_processing,
    bench_memory_usage_during_ingestion,
    bench_concurrent_data_sources
);

criterion_main!(benches);