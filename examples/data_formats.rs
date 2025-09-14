use orderbook::data::{
    BinaryDataSource, CsvDataSource, DataFormatDetector, DataSource, JsonDataSource, MarketEvent,
};
use orderbook::types::{Order, Side};
use std::io::Write;
use tempfile::NamedTempFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Data Source Format Examples");
    println!("===========================");

    // Create sample events
    let events = vec![
        MarketEvent::Trade {
            price: 10025,
            qty: 500,
            side: Side::Buy,
            timestamp: 1000000000,
            trade_id: Some("T1".to_string()),
        },
        MarketEvent::Quote {
            bid: Some(10020),
            ask: Some(10030),
            bid_qty: Some(100),
            ask_qty: Some(200),
            timestamp: 1000000001,
        },
        MarketEvent::OrderPlacement(Order::new_limit(1, Side::Buy, 100, 10020, 1000000002)),
        MarketEvent::OrderCancellation {
            order_id: 1,
            timestamp: 1000000003,
            reason: Some("User cancelled".to_string()),
        },
    ];

    // Test CSV format
    println!("\n1. CSV Data Source");
    println!("------------------");
    test_csv_format(&events)?;

    // Test JSON format
    println!("\n2. JSON Data Source");
    println!("-------------------");
    test_json_format(&events)?;

    // Test Binary format
    println!("\n3. Binary Data Source");
    println!("---------------------");
    test_binary_format(&events)?;

    // Test format detection
    println!("\n4. Format Detection");
    println!("-------------------");
    test_format_detection()?;

    println!("\nAll data source formats working correctly!");
    Ok(())
}

fn test_csv_format(_events: &[MarketEvent]) -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::with_suffix(".csv")?;
    
    // Write CSV header and data
    writeln!(temp_file, "type,timestamp,price,qty,side,trade_id")?;
    writeln!(temp_file, "trade,1000000000,100.25,500,buy,T1")?;
    writeln!(temp_file, "quote,1000000001,100.20,100.30,100,200")?;
    temp_file.flush()?;

    let mut csv_source = CsvDataSource::new(temp_file.path())?;
    let metadata = csv_source.metadata();
    
    println!("  Created CSV source: {}", metadata.name);
    println!("  File size: {} bytes", metadata.file_size.unwrap_or(0));
    
    let mut event_count = 0;
    while let Some(event) = csv_source.next_event()? {
        event_count += 1;
        println!("  Event {}: {:?}", event_count, event);
    }
    
    println!("  Total events read: {}", event_count);
    Ok(())
}

fn test_json_format(events: &[MarketEvent]) -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::with_suffix(".json")?;
    
    // Write JSON lines
    for event in events {
        writeln!(temp_file, "{}", serde_json::to_string(event)?)?;
    }
    temp_file.flush()?;

    let mut json_source = JsonDataSource::new(temp_file.path())?;
    let metadata = json_source.metadata();
    
    println!("  Created JSON source: {}", metadata.name);
    println!("  File size: {} bytes", metadata.file_size.unwrap_or(0));
    
    let mut event_count = 0;
    while let Some(event) = json_source.next_event()? {
        event_count += 1;
        println!("  Event {}: timestamp={}", event_count, event.timestamp());
    }
    
    println!("  Total events read: {}", event_count);
    Ok(())
}

fn test_binary_format(events: &[MarketEvent]) -> Result<(), Box<dyn std::error::Error>> {
    let temp_file = NamedTempFile::with_suffix(".bin")?;
    
    // Write binary data
    BinaryDataSource::write_binary_file(temp_file.path(), events)?;

    let mut binary_source = BinaryDataSource::new(temp_file.path())?;
    let metadata = binary_source.metadata();
    
    println!("  Created binary source: {}", metadata.name);
    println!("  File size: {} bytes", metadata.file_size.unwrap_or(0));
    println!("  Event count: {}", metadata.event_count.unwrap_or(0));
    
    if let Some((start, end)) = metadata.time_range {
        println!("  Time range: {} - {}", start, end);
    }
    
    let mut event_count = 0;
    while let Some(event) = binary_source.next_event()? {
        event_count += 1;
        println!("  Event {}: timestamp={}", event_count, event.timestamp());
    }
    
    println!("  Total events read: {}", event_count);

    // Test seeking
    binary_source.seek_to_time(1000000001)?;
    if let Some(event) = binary_source.next_event()? {
        println!("  After seek to 1000000001: timestamp={}", event.timestamp());
    }
    
    Ok(())
}

fn test_format_detection() -> Result<(), Box<dyn std::error::Error>> {
    // Test CSV detection
    let mut csv_file = NamedTempFile::with_suffix(".csv")?;
    writeln!(csv_file, "timestamp,price,qty,side")?;
    writeln!(csv_file, "1000000000,100.25,500,buy")?;
    csv_file.flush()?;
    
    let format = DataFormatDetector::detect_format(csv_file.path())?;
    println!("  CSV file detected as: {}", format);
    
    // Test JSON detection
    let mut json_file = NamedTempFile::with_suffix(".json")?;
    writeln!(json_file, r#"{{"Trade": {{"price": 10025}}}}"#)?;
    json_file.flush()?;
    
    let format = DataFormatDetector::detect_format(json_file.path())?;
    println!("  JSON file detected as: {}", format);
    
    // Test auto data source creation
    let data_source = DataFormatDetector::create_data_source(csv_file.path())?;
    println!("  Auto-created data source type: {}", data_source.metadata().source_type);
    
    Ok(())
}