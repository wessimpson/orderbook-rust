use serde::{Deserialize, Serialize};

/// Unique identifier for orders
pub type OrderId = u64;

/// Price represented as integer ticks for precision
pub type Price = u64;

/// Quantity of shares/contracts
pub type Qty = u64;

/// Order side (Buy or Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// Order type with associated data
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Limit { price: Price },
    Market,
}

/// Core order structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub qty: Qty,
    pub order_type: OrderType,
    pub ts: u128, // Nanosecond timestamp
}

/// Trade execution result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub maker_id: OrderId,
    pub taker_id: OrderId,
    pub price: Price,
    pub qty: Qty,
    pub ts: u128,
}

impl Order {
    /// Create a new limit order
    pub fn new_limit(id: OrderId, side: Side, qty: Qty, price: Price, ts: u128) -> Self {
        Self {
            id,
            side,
            qty,
            order_type: OrderType::Limit { price },
            ts,
        }
    }

    /// Create a new market order
    pub fn new_market(id: OrderId, side: Side, qty: Qty, ts: u128) -> Self {
        Self {
            id,
            side,
            qty,
            order_type: OrderType::Market,
            ts,
        }
    }

    /// Get the price for limit orders, None for market orders
    pub fn price(&self) -> Option<Price> {
        match self.order_type {
            OrderType::Limit { price } => Some(price),
            OrderType::Market => None,
        }
    }

    /// Check if this is a limit order
    pub fn is_limit(&self) -> bool {
        matches!(self.order_type, OrderType::Limit { .. })
    }

    /// Check if this is a market order
    pub fn is_market(&self) -> bool {
        matches!(self.order_type, OrderType::Market)
    }
}

impl Side {
    /// Get the opposite side
    pub fn opposite(&self) -> Side {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

/// Price utility functions
pub mod price_utils {
    use super::Price;

    /// Convert price from floating point to integer ticks
    /// Assumes 4 decimal places (e.g., $100.25 -> 1002500)
    pub fn from_f64(price: f64) -> Price {
        (price * 10000.0).round() as Price
    }

    /// Convert price from integer ticks to floating point
    /// Assumes 4 decimal places (e.g., 1002500 -> $100.25)
    pub fn to_f64(price: Price) -> f64 {
        price as f64 / 10000.0
    }

    /// Format price as string with proper decimal places
    pub fn format(price: Price) -> String {
        format!("{:.4}", to_f64(price))
    }

    /// Calculate spread between bid and ask prices
    pub fn spread(bid: Price, ask: Price) -> i64 {
        ask as i64 - bid as i64
    }

    /// Calculate mid-price between bid and ask
    pub fn mid_price(bid: Price, ask: Price) -> f64 {
        (bid as f64 + ask as f64) / 2.0 / 10000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::price_utils::*;

    #[test]
    fn test_order_creation() {
        let limit_order = Order::new_limit(1, Side::Buy, 100, from_f64(50.25), 1000);
        assert_eq!(limit_order.id, 1);
        assert_eq!(limit_order.side, Side::Buy);
        assert_eq!(limit_order.qty, 100);
        assert_eq!(limit_order.price(), Some(from_f64(50.25)));
        assert!(limit_order.is_limit());
        assert!(!limit_order.is_market());

        let market_order = Order::new_market(2, Side::Sell, 50, 2000);
        assert_eq!(market_order.id, 2);
        assert_eq!(market_order.side, Side::Sell);
        assert_eq!(market_order.qty, 50);
        assert_eq!(market_order.price(), None);
        assert!(!market_order.is_limit());
        assert!(market_order.is_market());
    }

    #[test]
    fn test_side_opposite() {
        assert_eq!(Side::Buy.opposite(), Side::Sell);
        assert_eq!(Side::Sell.opposite(), Side::Buy);
    }

    #[test]
    fn test_price_utils() {
        let price = from_f64(100.25);
        assert_eq!(price, 1002500);
        assert_eq!(to_f64(price), 100.25);
        assert_eq!(format(price), "100.2500");

        let bid = from_f64(100.00);
        let ask = from_f64(100.05);
        assert_eq!(spread(bid, ask), 500); // 5 cents in ticks
        assert_eq!(mid_price(bid, ask), 100.025);
    }

    #[test]
    fn test_serde_serialization() {
        let order = Order::new_limit(1, Side::Buy, 100, from_f64(50.25), 1000);
        let json = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&json).unwrap();
        assert_eq!(order, deserialized);

        let trade = Trade {
            maker_id: 1,
            taker_id: 2,
            price: from_f64(50.25),
            qty: 50,
            ts: 1000,
        };
        let json = serde_json::to_string(&trade).unwrap();
        let deserialized: Trade = serde_json::from_str(&json).unwrap();
        assert_eq!(trade, deserialized);
    }
}