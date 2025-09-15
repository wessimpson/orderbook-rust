use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use orderbook::*;
use orderbook::engine::{OrderBook, OrderBookEngine};
use orderbook::queue_fifo::FifoLevel;
use orderbook::types::{Order, OrderType, Side, price_utils};
use orderbook::time::now_ns;

type TestOrderBook = OrderBook<FifoLevel>;

fn create_test_order(id: OrderId, side: Side, qty: Qty, price: Price) -> Order {
    Order {
        id,
        side,
        qty,
        order_type: OrderType::Limit { price },
        ts: now_ns(),
    }
}

fn create_market_order(id: OrderId, side: Side, qty: Qty) -> Order {
    Order {
        id,
        side,
        qty,
        order_type: OrderType::Market,
        ts: now_ns(),
    }
}

fn bench_order_placement(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_placement");
    
    for order_count in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*order_count as u64));
        
        group.bench_with_input(
            BenchmarkId::new("limit_orders", order_count),
            order_count,
            |b, &order_count| {
                b.iter_batched(
                    || {
                        let book = TestOrderBook::new();
                        let mut orders = Vec::new();
                        
                        // Pre-generate orders
                        for i in 0..order_count {
                            let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                            let base_price = price_utils::from_f64(100.0);
                            let price_offset = (i % 100) as Price * 100; // Spread orders across price levels
                            let price = if side == Side::Buy {
                                base_price - price_offset
                            } else {
                                base_price + price_offset
                            };
                            
                            orders.push(create_test_order(i as OrderId, side, 100, price));
                        }
                        
                        (book, orders)
                    },
                    |(mut book, orders)| {
                        for order in orders {
                            black_box(book.place(order).unwrap());
                        }
                        black_box(book)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_order_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_matching");
    
    for depth in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("market_order_sweep", depth),
            depth,
            |b, &depth| {
                b.iter_batched(
                    || {
                        let mut book = TestOrderBook::new();
                        
                        // Create a deep order book
                        for i in 0..depth {
                            let price = price_utils::from_f64(100.0) + i as Price * 10;
                            let buy_order = create_test_order(i as OrderId * 2, Side::Buy, 100, price - 500);
                            let sell_order = create_test_order(i as OrderId * 2 + 1, Side::Sell, 100, price + 500);
                            
                            book.place(buy_order).unwrap();
                            book.place(sell_order).unwrap();
                        }
                        
                        book
                    },
                    |mut book| {
                        // Place a large market order that will sweep multiple levels
                        let market_order = create_market_order(999999, Side::Buy, (depth * 50) as Qty);
                        black_box(book.place(market_order).unwrap())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_order_cancellation(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_cancellation");
    
    for order_count in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*order_count as u64));
        
        group.bench_with_input(
            BenchmarkId::new("random_cancellations", order_count),
            order_count,
            |b, &order_count| {
                b.iter_batched(
                    || {
                        let mut book = TestOrderBook::new();
                        let mut order_ids = Vec::new();
                        
                        // Place orders and collect IDs
                        for i in 0..order_count {
                            let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                            let base_price = price_utils::from_f64(100.0);
                            let price = if side == Side::Buy {
                                base_price - 1000
                            } else {
                                base_price + 1000
                            };
                            
                            let order = create_test_order(i as OrderId, side, 100, price);
                            order_ids.push(order.id);
                            book.place(order).unwrap();
                        }
                        
                        (book, order_ids)
                    },
                    |(mut book, order_ids)| {
                        // Cancel all orders
                        for order_id in order_ids {
                            black_box(book.cancel(order_id).unwrap());
                        }
                        black_box(book)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_snapshot_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_generation");
    
    for depth in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("full_snapshot", depth),
            depth,
            |b, &depth| {
                b.iter_batched(
                    || {
                        let mut book = TestOrderBook::new();
                        
                        // Create a deep order book with multiple price levels
                        for i in 0..depth {
                            for j in 0..10 { // 10 orders per level
                                let price = price_utils::from_f64(100.0) + i as Price * 10;
                                let buy_order = create_test_order(
                                    (i * 20 + j * 2) as OrderId, 
                                    Side::Buy, 
                                    100, 
                                    price - 500
                                );
                                let sell_order = create_test_order(
                                    (i * 20 + j * 2 + 1) as OrderId, 
                                    Side::Sell, 
                                    100, 
                                    price + 500
                                );
                                
                                book.place(buy_order).unwrap();
                                book.place(sell_order).unwrap();
                            }
                        }
                        
                        book
                    },
                    |book| {
                        black_box(book.snapshot())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");
    
    group.bench_function("realistic_trading", |b| {
        b.iter_batched(
            || TestOrderBook::new(),
            |mut book| {
                let mut order_id = 1;
                
                // Simulate realistic trading pattern
                for round in 0..100 {
                    let base_price = price_utils::from_f64(100.0);
                    
                    // Market making orders (70% of volume)
                    for i in 0..7 {
                        let spread = (i + 1) as Price * 10;
                        let buy_order = create_test_order(
                            order_id, 
                            Side::Buy, 
                            100, 
                            base_price - spread
                        );
                        let sell_order = create_test_order(
                            order_id + 1, 
                            Side::Sell, 
                            100, 
                            base_price + spread
                        );
                        
                        book.place(buy_order).unwrap();
                        book.place(sell_order).unwrap();
                        order_id += 2;
                    }
                    
                    // Market orders (20% of volume)
                    for _ in 0..2 {
                        let side = if round % 2 == 0 { Side::Buy } else { Side::Sell };
                        let market_order = create_market_order(order_id, side, 150);
                        let _ = book.place(market_order); // May fail if no liquidity
                        order_id += 1;
                    }
                    
                    // Cancellations (10% of volume)
                    if order_id > 20 {
                        let cancel_id = order_id - 20;
                        let _ = book.cancel(cancel_id); // May fail if already filled
                    }
                    
                    // Generate snapshot every 10 rounds
                    if round % 10 == 0 {
                        black_box(book.snapshot());
                    }
                }
                
                black_box(book)
            },
            criterion::BatchSize::SmallInput,
        );
    });
    
    group.finish();
}

fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");
    
    group.bench_function("large_order_book", |b| {
        b.iter_batched(
            || TestOrderBook::new(),
            |mut book| {
                // Create a very large order book to test memory efficiency
                for i in 0..50000 {
                    let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                    let base_price = price_utils::from_f64(100.0);
                    let price_levels = 1000;
                    let price_offset = (i % price_levels) as Price * 10;
                    let price = if side == Side::Buy {
                        base_price - price_offset
                    } else {
                        base_price + price_offset
                    };
                    
                    let order = create_test_order(i as OrderId, side, 100, price);
                    book.place(order).unwrap();
                    
                    // Periodically cancel some orders to test cleanup
                    if i > 1000 && i % 100 == 0 {
                        let cancel_id = i - 1000;
                        let _ = book.cancel(cancel_id);
                    }
                }
                
                black_box(book)
            },
            criterion::BatchSize::LargeInput,
        );
    });
    
    group.finish();
}

fn bench_price_level_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("price_level_operations");
    
    for orders_per_level in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("fifo_matching", orders_per_level),
            orders_per_level,
            |b, &orders_per_level| {
                b.iter_batched(
                    || {
                        let mut level = FifoLevel::new();
                        
                        // Fill level with orders
                        for i in 0..orders_per_level {
                            let order = create_test_order(i as OrderId, Side::Buy, 100, 50000);
                            level.enqueue(order);
                        }
                        
                        level
                    },
                    |mut level| {
                        // Match against the entire level
                        let (remaining, trades) = level.match_against(
                            999999, 
                            Side::Sell, 
                            (orders_per_level * 100) as Qty, 
                            50000
                        );
                        black_box((remaining, trades))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_order_placement,
    bench_order_matching,
    bench_order_cancellation,
    bench_snapshot_generation,
    bench_mixed_workload,
    bench_memory_efficiency,
    bench_price_level_operations
);

criterion_main!(benches);