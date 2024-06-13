use std::{
    cmp::min,
    collections::{BTreeMap, HashMap, VecDeque},
    error::Error,
    fmt::Display,
};

#[derive(Debug, Copy, Clone)]
pub enum LivreError {
    UnfillableOrder,
    OrderNotFound,
    DuplicateOrderId,
    QuantityTooBig,
}

impl Display for LivreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            LivreError::UnfillableOrder => write!(f, "Could not fill order"),
            LivreError::DuplicateOrderId => write!(f, "Order id already in use"),
            LivreError::QuantityTooBig => write!(f, "Fill quantity exceeds order lot size"),
            LivreError::OrderNotFound => write!(f, "Could not find order matching id"),
        }
    }
}

impl Error for LivreError {}

#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    FillAndKill,
    GoodTillCancel,
    FillOrKill,
    GoodForDay,
    Market,
}

#[derive(Debug, Clone, Copy)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(Debug, Clone, Copy)]
pub enum OrderState {
    Filled,
    PartialFill(u64),
    Unfilled,
}

#[derive(Debug)]
pub struct Order {
    pub order_id: u64,
    pub order_type: OrderType,
    pub side: Side,
    pub price: u64,
    pub initial_quantity: u64,
    pub remaining_quantity: u64,
}

impl Order {
    pub fn new(
        order_type: OrderType,
        order_id: u64,
        side: Side,
        price: u64,
        quantity: u64,
    ) -> Self {
        Self {
            order_type,
            order_id,
            side,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
        }
    }

    pub fn is_filled(&self) -> bool {
        self.remaining_quantity == 0
    }

    pub fn order_state(&self) -> OrderState {
        if self.initial_quantity == self.remaining_quantity {
            OrderState::Unfilled
        } else if self.is_filled() {
            OrderState::Filled
        } else {
            OrderState::PartialFill(self.initial_quantity - self.remaining_quantity)
        }
    }

    pub fn fill(&mut self, quantity: u64) -> Result<(), LivreError> {
        if quantity > self.remaining_quantity {
            Err(LivreError::QuantityTooBig)
        } else {
            self.remaining_quantity -= quantity;
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct ModifyOrder {
    pub order_id: u64,
    pub side: Side,
    pub price: u64,
    pub quantity: u64,
}

impl ModifyOrder {
    fn to_order(self, order_type: OrderType) -> Order {
        Order::new(
            order_type,
            self.order_id,
            self.side,
            self.price,
            self.quantity,
        )
    }
}
pub struct Trade {
    pub taker_order_id: u64,
    pub maker_order_id: u64,
    pub price: u64,
    pub quantity: u64,
}

impl Trade {
    fn new(taker_order_id: u64, maker_order_id: u64, price: u64, quantity: u64) -> Self {
        Self {
            taker_order_id,
            maker_order_id,
            price,
            quantity,
        }
    }
}

type TradeLog = Vec<Trade>;
pub struct LevelIdentifier {
    price: u64,
    side: Side,
}

impl LevelIdentifier {
    fn new(price: u64, side: Side) -> Self {
        Self { price, side }
    }
}

pub struct MatchInfo {
    pub trade_log: TradeLog,
    pub order_state: OrderState,
}

impl MatchInfo {
    fn new(trade_log: TradeLog, order_state: OrderState) -> Self {
        Self {
            trade_log,
            order_state,
        }
    }
}
pub struct Orderbook {
    pub bids: BTreeMap<u64, VecDeque<Order>>,
    pub asks: BTreeMap<u64, VecDeque<Order>>,
    pub orders: HashMap<u64, LevelIdentifier>,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
        }
    }

    pub fn add_order(&mut self, mut order: Order) -> Result<MatchInfo, LivreError> {
        match order {
            Order {
                side,
                price,
                order_type: OrderType::FillAndKill,
                ..
            } => {
                if !self.can_match(side, price) {
                    return Err(LivreError::UnfillableOrder);
                }
            }

            Order {
                side,
                price,
                initial_quantity,
                order_type: OrderType::FillOrKill,
                ..
            } => {
                if !self.can_fully_fill(price, initial_quantity, side) {
                    return Err(LivreError::UnfillableOrder);
                }
            }

            Order { order_id, .. } if self.orders.contains_key(&order_id) => {
                return Err(LivreError::DuplicateOrderId)
            }
            _ => {}
        };

        let trade_log = self.match_order(&mut order);
        let order_state = order.order_state();
        if !order.is_filled()
            && matches!(
                order.order_type,
                OrderType::GoodForDay | OrderType::Market | OrderType::GoodTillCancel
            )
        {
            self.orders.insert(
                order.order_id,
                LevelIdentifier::new(order.price, order.side),
            );
            let book_side = match order.side {
                Side::Ask => &mut self.asks,
                Side::Bid => &mut self.bids,
            };
            book_side
                .entry(order.price)
                .or_insert_with(VecDeque::new)
                .push_back(order);
        }

        Ok(MatchInfo::new(trade_log, order_state))
    }

    pub fn cancel_order(&mut self, order_id: u64) -> Result<Order, LivreError> {
        if let Some(level) = self.orders.get(&order_id) {
            let book_side = match level.side {
                Side::Ask => &mut self.asks,
                Side::Bid => &mut self.bids,
            };

            let level = book_side.get_mut(&level.price).unwrap();
            // can can unwrap as we know it exists in our orders map
            let idx = level
                .iter()
                .position(|order| order.order_id == order_id)
                .unwrap();
            self.orders
                .remove(&order_id)
                .ok_or(LivreError::OrderNotFound)?;
            level.remove(idx).ok_or(LivreError::OrderNotFound)
        } else {
            Err(LivreError::OrderNotFound)
        }
    }

    pub fn modify_order(&mut self, order: ModifyOrder) -> Result<MatchInfo, LivreError> {
        let old_order = self.cancel_order(order.order_id)?;
        let order = order.to_order(old_order.order_type);
        self.add_order(order)
    }

    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    fn match_order(&mut self, order: &mut Order) -> TradeLog {
        let mut trade_log = Vec::new();

        match order.side {
            Side::Bid => {
                while let Some((best_price, queue)) = self.asks.pop_first() {
                    if best_price > order.price {
                        self.asks.insert(best_price, queue);
                        break;
                    }
                    self.match_level(best_price, queue, order, &mut trade_log);
                }
            }
            Side::Ask => {
                while let Some((best_price, queue)) = self.bids.pop_last() {
                    if best_price < order.price {
                        self.bids.insert(best_price, queue);
                        break;
                    }
                    self.match_level(best_price, queue, order, &mut trade_log);
                }
            }
        };
        trade_log
    }

    fn match_level(
        &mut self,
        best_price: u64,
        mut queue: VecDeque<Order>,
        order: &mut Order,
        trade_log: &mut TradeLog,
    ) {
        while !order.is_filled() && !queue.is_empty() {
            // already checked if queue is empty, so there will always be a front element
            let maker_order = queue.front_mut().unwrap();
            let trade_quantity = min(maker_order.remaining_quantity, order.remaining_quantity);
            // can unwrap as quantity will necessarily be leq than both order's quantity
            maker_order.fill(trade_quantity).unwrap();
            order.fill(trade_quantity).unwrap();
            trade_log.push(Trade::new(
                order.order_id,
                maker_order.order_id,
                best_price,
                trade_quantity,
            ));
            if maker_order.is_filled() {
                self.orders.remove(&maker_order.order_id);
                queue.pop_front();
            }
        }

        if !queue.is_empty() {
            self.asks.insert(best_price, queue);
        }
    }

    pub fn can_match(&self, side: Side, price: u64) -> bool {
        match side {
            Side::Ask => {
                if let Some((best_price, _)) = self.bids.last_key_value() {
                    *best_price >= price
                } else {
                    false
                }
            }
            Side::Bid => {
                if let Some((best_price, _)) = self.asks.first_key_value() {
                    *best_price <= price
                } else {
                    false
                }
            }
        }
    }

    pub fn can_fully_fill(&self, price: u64, mut quantity: u64, side: Side) -> bool {
        if !self.can_match(side, price) {
            return false;
        }

        let mut level_iter: Box<dyn Iterator<Item = _>> = match side {
            Side::Ask => Box::new(self.bids.iter()),
            Side::Bid => Box::new(self.asks.iter().rev()),
        };
        while let Some((level_price, queue)) = level_iter.next() {
            if (matches!(side, Side::Ask) && *level_price < price)
                || (matches!(side, Side::Bid) && *level_price > price)
            {
                return false;
            }
            let level_quantity = queue.iter().map(|order| order.remaining_quantity).sum();
            if quantity <= level_quantity {
                return true;
            }
            quantity -= level_quantity;
        }
        false
    }
}
