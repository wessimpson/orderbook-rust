use crate::types::{Order, OrderId, Price, Qty, Side, Trade};

/// Trait defining the interface for order queue disciplines
/// 
/// This trait abstracts different matching algorithms (FIFO, Pro-Rata, etc.)
/// allowing the order book to be generic over the matching strategy.
pub trait QueueDiscipline {
    /// Add an order to the queue
    /// 
    /// # Arguments
    /// * `order` - The order to add to the queue
    fn enqueue(&mut self, order: Order);

    /// Match a taker order against orders in this queue
    /// 
    /// # Arguments
    /// * `taker_id` - ID of the taker order
    /// * `taker_side` - Side of the taker order (opposite to this queue's side)
    /// * `taker_qty` - Quantity of the taker order to match
    /// * `price` - Price level for matching
    /// 
    /// # Returns
    /// * Tuple of (remaining_taker_qty, trades_generated)
    fn match_against(
        &mut self,
        taker_id: OrderId,
        taker_side: Side,
        taker_qty: Qty,
        price: Price,
    ) -> (Qty, Vec<Trade>);

    /// Cancel an order from the queue
    /// 
    /// # Arguments
    /// * `order_id` - ID of the order to cancel
    /// 
    /// # Returns
    /// * Quantity that was cancelled (0 if order not found)
    fn cancel(&mut self, order_id: OrderId) -> Qty;

    /// Get the total quantity available at this price level
    fn total_qty(&self) -> Qty;

    /// Check if the queue is empty
    fn is_empty(&self) -> bool;

    /// Mark this price level as recently active (for latency tracking)
    fn touch(&mut self);

    /// Get the timestamp of the last activity on this price level
    fn last_ts(&self) -> u128;

    /// Get the number of orders in the queue
    fn order_count(&self) -> usize;

    /// Get the oldest order timestamp in the queue (for latency calculations)
    fn oldest_order_ts(&self) -> Option<u128>;
}