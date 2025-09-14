use orderbook::data::{CsvDataSource, DataSource, MarketEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a CSV data source from the sample file
    let mut csv_source = CsvDataSource::new("sample_data.csv")?;
    
    println!("CSV Data Source Metadata:");
    let metadata = csv_source.metadata();
    println!("  Name: {}", metadata.name);
    println!("  Type: {}", metadata.source_type);
    if let Some(size) = metadata.file_size {
        println!("  File Size: {} bytes", size);
    }
    
    // Set playback speed to 2x for faster replay
    csv_source.set_playback_speed(2.0)?;
    
    println!("\nReplaying market events:");
    let mut event_count = 0;
    
    while let Some(event) = csv_source.next_event()? {
        event_count += 1;
        
        match event {
            MarketEvent::Trade { price, qty, side, timestamp, trade_id } => {
                println!("  [{}] TRADE: {:?} {} @ {} (ID: {:?})", 
                         timestamp, side, qty, price as f64 / 10000.0, trade_id);
            }
            MarketEvent::Quote { bid, ask, bid_qty, ask_qty, timestamp } => {
                println!("  [{}] QUOTE: Bid: {:?} ({:?}) Ask: {:?} ({:?})", 
                         timestamp, 
                         bid.map(|p| p as f64 / 10000.0), bid_qty,
                         ask.map(|p| p as f64 / 10000.0), ask_qty);
            }
            MarketEvent::OrderPlacement(order) => {
                println!("  [{}] ORDER: ID {} {:?} {} @ {:?}", 
                         order.ts, order.id, order.side, order.qty,
                         order.price().map(|p| p as f64 / 10000.0));
            }
            MarketEvent::OrderCancellation { order_id, timestamp, reason } => {
                println!("  [{}] CANCEL: Order {} (Reason: {:?})", 
                         timestamp, order_id, reason);
            }
            MarketEvent::MarketStatus { status, timestamp, message } => {
                println!("  [{}] STATUS: {:?} (Message: {:?})", 
                         timestamp, status, message);
            }
            MarketEvent::BestBidOffer { best_bid, best_ask, bid_qty, ask_qty, timestamp } => {
                println!("  [{}] BBO: Bid: {:?} ({:?}) Ask: {:?} ({:?})", 
                         timestamp,
                         best_bid.map(|p| p as f64 / 10000.0), bid_qty,
                         best_ask.map(|p| p as f64 / 10000.0), ask_qty);
            }
            _ => {
                println!("  [{}] OTHER: {:?}", event.timestamp(), event);
            }
        }
    }
    
    println!("\nReplay completed. Processed {} events.", event_count);
    println!("Data source finished: {}", csv_source.is_finished());
    
    Ok(())
}