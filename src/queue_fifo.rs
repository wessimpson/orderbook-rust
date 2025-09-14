use std::collections::VecDeque;
use crate::queue::QueueDiscipline;
use crate::types::{Order, OrderId, Price, Qty, Side, Trade};
use crate::time::now_ns;

/// FIFO (First-In-First-Out) queue discipline implementation
/// 
/// Orders are matched in the order they were received (time priority).
/// Uses VecDeque for efficient insertion at back and removal from front.
#[derive(Debug, Clone)]
pub struct FifoLevel {
    /// Queue of orders at this price level
    orders: VecDeque<Order>,
    /// Total quantity available at this level
    total_qty: Qty,
    /// Timestamp of last activity (for latency heatmap)
    last_activity_ts: u128,
}

impl FifoLevel {
    /// Create a new empty FIFO level
    pub fn new() -> Self {
        Self {
            orders: VecDeque::new(),
            total_qty: 0,
            last_activity_ts: now_ns(),
        }
    }

    /// Create a new FIFO level with an initial order
    pub fn with_order(order: Order) -> Self {
        let mut level = Self::new();
        level.enqueue(order);
        level
    }
}

impl Default for FifoLevel {
    fn default() -> Self {
        Self::new()
    }
}

impl QueueDiscipline for FifoLevel {
    fn enqueue(&mut self, order: Order) {
        self.total_qty += order.qty;
        self.orders.push_back(order);
        self.touch();
    }

    fn match_against(
        &mut self,
        taker_id: OrderId,
        _taker_side: Side,
        mut taker_qty: Qty,
        price: Price,
    ) -> (Qty, Vec<Trade>) {
        let mut trades = Vec::new();
        let trade_ts = now_ns();

        // Process orders in FIFO order (front to back)
        while taker_qty > 0 && !self.orders.is_empty() {
            let maker_order = self.orders.front_mut().unwrap();
            
            // Calculate trade quantity (minimum of taker and maker quantities)
            let trade_qty = std::cmp::min(taker_qty, maker_order.qty);
            
            // Create trade
            let trade = Trade {
                maker_id: maker_order.id,
                taker_id,
                price,
                qty: trade_qty,
                ts: trade_ts,
            };
            trades.push(trade);

            // Update quantities
            taker_qty -= trade_qty;
            maker_order.qty -= trade_qty;
            self.total_qty -= trade_qty;

            // Remove maker order if fully filled
            if maker_order.qty == 0 {
                self.orders.pop_front();
            }
        }

        self.touch();
        (taker_qty, trades)
    }

    fn cancel(&mut self, order_id: OrderId) -> Qty {
        // Find and remove the order with matching ID
        for i in 0..self.orders.len() {
            if self.orders[i].id == order_id {
                let cancelled_order = self.orders.remove(i).unwrap();
                self.total_qty -= cancelled_order.qty;
                self.touch();
                return cancelled_order.qty;
            }
        }
        0 // Order not found
    }

    fn total_qty(&self) -> Qty {
        self.total_qty
    }

    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    fn touch(&mut self) {
        self.last_activity_ts = now_ns();
    }

    fn last_ts(&self) -> u128 {
        self.last_activity_ts
    }

    fn order_count(&self) -> usize {
        self.orders.len()
    }

    fn oldest_order_ts(&self) -> Option<u128> {
        self.orders.front().map(|order| order.ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OrderType, Side};
    use crate::time::now_ns;

    fn create_test_order(id: OrderId, side: Side, qty: Qty, price: Price) -> Order {
        Order {
            id,
            side,
            qty,
            order_type: OrderType::Limit { price },
            ts: now_ns(),
        }
    }

    #[test]
    fn test_fifo_level_creation() {
        let level = FifoLevel::new();
        assert!(level.is_empty());
        assert_eq!(level.total_qty(), 0);
        assert_eq!(level.order_count(), 0);
        assert!(level.oldest_order_ts().is_none());
    }

    #[test]
    fn test_fifo_level_with_order() {
        let order = create_test_order(1, Side::Buy, 100, 5000);
        let level = FifoLevel::with_order(order.clone());
        
        assert!(!level.is_empty());
        assert_eq!(level.total_qty(), 100);
        assert_eq!(level.order_count(), 1);
        assert_eq!(level.oldest_order_ts(), Some(order.ts));
    }

    #[test]
    fn test_enqueue_orders() {
        let mut level = FifoLevel::new();
        
        let order1 = create_test_order(1, Side::Buy, 100, 5000);
        let order2 = create_test_order(2, Side::Buy, 200, 5000);
        
        level.enqueue(order1.clone());
        assert_eq!(level.total_qty(), 100);
        assert_eq!(level.order_count(), 1);
        
        level.enqueue(order2);
        assert_eq!(level.total_qty(), 300);
        assert_eq!(level.order_count(), 2);
        assert_eq!(level.oldest_order_ts(), Some(order1.ts));
    }

    #[test]
    fn test_fifo_matching_partial_fill() {
        let mut level = FifoLevel::new();
        
        // Add two buy orders at the same price
        let order1 = create_test_order(1, Side::Buy, 100, 5000);
        let order2 = create_test_order(2, Side::Buy, 200, 5000);
        
        level.enqueue(order1);
        level.enqueue(order2);
        
        // Match against a sell order for 150 shares
        let (remaining_qty, trades) = level.match_against(3, Side::Sell, 150, 5000);
        
        // Should have 0 remaining (fully matched)
        assert_eq!(remaining_qty, 0);
        
        // Should generate 2 trades
        assert_eq!(trades.len(), 2);
        
        // First trade: 100 shares with order 1 (FIFO)
        assert_eq!(trades[0].maker_id, 1);
        assert_eq!(trades[0].taker_id, 3);
        assert_eq!(trades[0].qty, 100);
        assert_eq!(trades[0].price, 5000);
        
        // Second trade: 50 shares with order 2
        assert_eq!(trades[1].maker_id, 2);
        assert_eq!(trades[1].taker_id, 3);
        assert_eq!(trades[1].qty, 50);
        assert_eq!(trades[1].price, 5000);
        
        // Level should have 150 shares remaining (200 - 50 from order 2)
        assert_eq!(level.total_qty(), 150);
        assert_eq!(level.order_count(), 1);
    }

    #[test]
    fn test_fifo_matching_complete_fill() {
        let mut level = FifoLevel::new();
        
        let order = create_test_order(1, Side::Buy, 100, 5000);
        level.enqueue(order);
        
        // Match exactly the available quantity
        let (remaining_qty, trades) = level.match_against(2, Side::Sell, 100, 5000);
        
        assert_eq!(remaining_qty, 0);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, 100);
        
        // Level should be empty
        assert!(level.is_empty());
        assert_eq!(level.total_qty(), 0);
        assert_eq!(level.order_count(), 0);
    }

    #[test]
    fn test_fifo_matching_insufficient_liquidity() {
        let mut level = FifoLevel::new();
        
        let order = create_test_order(1, Side::Buy, 100, 5000);
        level.enqueue(order);
        
        // Try to match more than available
        let (remaining_qty, trades) = level.match_against(2, Side::Sell, 200, 5000);
        
        assert_eq!(remaining_qty, 100); // 100 shares couldn't be matched
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, 100);
        
        // Level should be empty after consuming all liquidity
        assert!(level.is_empty());
        assert_eq!(level.total_qty(), 0);
    }

    #[test]
    fn test_cancel_order() {
        let mut level = FifoLevel::new();
        
        let order1 = create_test_order(1, Side::Buy, 100, 5000);
        let order2 = create_test_order(2, Side::Buy, 200, 5000);
        let order3 = create_test_order(3, Side::Buy, 150, 5000);
        
        level.enqueue(order1);
        level.enqueue(order2);
        level.enqueue(order3);
        
        // Cancel middle order
        let cancelled_qty = level.cancel(2);
        assert_eq!(cancelled_qty, 200);
        assert_eq!(level.total_qty(), 250); // 100 + 150
        assert_eq!(level.order_count(), 2);
        
        // Try to cancel non-existent order
        let cancelled_qty = level.cancel(999);
        assert_eq!(cancelled_qty, 0);
        assert_eq!(level.total_qty(), 250); // Unchanged
        assert_eq!(level.order_count(), 2);
        
        // Verify FIFO order is maintained after cancellation
        let (remaining_qty, trades) = level.match_against(4, Side::Sell, 50, 5000);
        assert_eq!(remaining_qty, 0);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].maker_id, 1); // Should match order 1 first
        assert_eq!(trades[0].qty, 50);
    }

    #[test]
    fn test_cancel_all_orders() {
        let mut level = FifoLevel::new();
        
        let order1 = create_test_order(1, Side::Buy, 100, 5000);
        let order2 = create_test_order(2, Side::Buy, 200, 5000);
        
        level.enqueue(order1);
        level.enqueue(order2);
        
        // Cancel both orders
        level.cancel(1);
        level.cancel(2);
        
        assert!(level.is_empty());
        assert_eq!(level.total_qty(), 0);
        assert_eq!(level.order_count(), 0);
        assert!(level.oldest_order_ts().is_none());
    }

    #[test]
    fn test_timestamp_tracking() {
        let mut level = FifoLevel::new();
        let initial_ts = level.last_ts();
        
        // Sleep briefly to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(1));
        
        // Touch should update timestamp
        level.touch();
        assert!(level.last_ts() > initial_ts);
        
        let touch_ts = level.last_ts();
        
        // Enqueue should update timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let order = create_test_order(1, Side::Buy, 100, 5000);
        level.enqueue(order);
        assert!(level.last_ts() > touch_ts);
        
        let enqueue_ts = level.last_ts();
        
        // Match should update timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        level.match_against(2, Side::Sell, 50, 5000);
        assert!(level.last_ts() > enqueue_ts);
        
        let match_ts = level.last_ts();
        
        // Cancel should update timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        level.cancel(1);
        assert!(level.last_ts() > match_ts);
    }

    #[test]
    fn test_oldest_order_timestamp() {
        let mut level = FifoLevel::new();
        
        // Add orders with different timestamps
        let ts1 = now_ns();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ts2 = now_ns();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ts3 = now_ns();
        
        let order1 = Order {
            id: 1,
            side: Side::Buy,
            qty: 100,
            order_type: OrderType::Limit { price: 5000 },
            ts: ts1,
        };
        let order2 = Order {
            id: 2,
            side: Side::Buy,
            qty: 200,
            order_type: OrderType::Limit { price: 5000 },
            ts: ts2,
        };
        let order3 = Order {
            id: 3,
            side: Side::Buy,
            qty: 150,
            order_type: OrderType::Limit { price: 5000 },
            ts: ts3,
        };
        
        level.enqueue(order1);
        level.enqueue(order2);
        level.enqueue(order3);
        
        // Oldest should be the first order
        assert_eq!(level.oldest_order_ts(), Some(ts1));
        
        // After matching first order, oldest should be second order
        level.match_against(4, Side::Sell, 100, 5000);
        assert_eq!(level.oldest_order_ts(), Some(ts2));
        
        // After cancelling second order, oldest should be third order
        level.cancel(2);
        assert_eq!(level.oldest_order_ts(), Some(ts3));
        
        // After cancelling last order, should be None
        level.cancel(3);
        assert_eq!(level.oldest_order_ts(), None);
    }
}