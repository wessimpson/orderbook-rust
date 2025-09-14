use orderbook::{Order, Side, price_utils};

fn main() {
    println!("Order Book Server Starting...");
    
    // Example usage of the core types
    let order = Order::new_limit(1, Side::Buy, 100, price_utils::from_f64(50.25), 1000);
    println!("Created order: {:?}", order);
    println!("Order price: ${}", price_utils::format(order.price().unwrap()));
    
    println!("Server initialized with core types");
}