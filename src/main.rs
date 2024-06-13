use livre::{Order, Orderbook};

fn main() {
    let mut order_book = Orderbook::new();
    let bid1 = Order::new(livre::OrderType::Market, 1, livre::Side::Bid, 150, 10);
    if let Ok(inf) = order_book.add_order(bid1) {
        println!("{:?}", inf.order_state);
    }
    let ask1 = Order::new(livre::OrderType::Market, 2, livre::Side::Ask, 150, 5);
    match order_book.add_order(ask1) {
        Ok(inf) => println!("{:?}", inf.order_state),
        Err(err) => println!("{}", err),
    }
}
