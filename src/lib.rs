pub mod error;
pub mod okx;

use std::cmp::Reverse;
use std::collections::BTreeMap;

use okx::Channel;
use okx::Currency;
use okx::WebsocketClient;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;

// OrderBook is a struct that represents the order book of a trading pair.
#[derive(Debug, Clone)]
pub struct OrderBook {
    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Reverse<Decimal>, Decimal>,
    ts: i64,
}

// Side is an enum that represents the side of the order book.
pub enum Side {
    Bids,
    Asks,
}

impl OrderBook {
    // new creates a new OrderBook.
    pub fn new() -> Self {
        OrderBook {
            bids: Default::default(),
            asks: Default::default(),
            ts: 0,
        }
    }
}

impl OrderBook {
    // cleanup clears the bids and asks from the order book.
    pub fn cleanup(&mut self) {
        self.bids.clear();
        self.asks.clear();
    }

    // insert_ask inserts an ask into the order book.
    pub fn insert_ask(&mut self, price: f64, amount: f64) {
        let price = Decimal::from_f64(price).unwrap();
        let amount = Decimal::from_f64(amount).unwrap();
        let size = price * amount;

        self.asks.insert(Reverse(price), size);
    }

    // insert_bid inserts a bid into the order book.
    pub fn insert_bid(&mut self, price: f64, amount: f64) {
        let price = Decimal::from_f64(price).unwrap();
        let amount = Decimal::from_f64(amount).unwrap();
        let size = price * amount;
        self.bids.insert(price, size);
    }

    // update_side updates the bids or asks side of the order book.
    pub fn update_side(&mut self, side: Side, price: f64, amount: f64) {
        let price = Decimal::from_f64(price).unwrap();
        let amount = Decimal::from_f64(amount).unwrap();
        let size = price * amount;
        match side {
            Side::Bids => {
                if size.is_zero() {
                    self.bids.remove(&price);
                } else {
                    self.bids.insert(price, size);
                }
            }
            Side::Asks => {
                if size.is_zero() {
                    self.asks.remove(&Reverse(price));
                } else {
                    self.asks.insert(Reverse(price), size);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Reverse;
    use std::collections::BTreeMap;

    use rust_decimal::Decimal;

    use crate::OrderBook;
    use crate::Side;

    #[test]
    fn test_orderbook() {
        // Create a new OrderBook.
        let mut ob = OrderBook::new();

        // Insert first bid into the order book.
        ob.update_side(Side::Bids, 2., 100.);

        // Insert second bid into the order book.
        ob.update_side(Side::Bids, 1., 100.);

        assert_eq!(
            ob.bids,
            BTreeMap::from([
                (Decimal::from(1), Decimal::from(100)),
                (Decimal::from(2), Decimal::from(200))
            ])
        );

        // Insert first ask into the order book.
        ob.update_side(Side::Asks, 1., 100.);

        // Insert second ask into the order book.
        ob.update_side(Side::Asks, 2., 100.);
        assert_eq!(
            ob.asks,
            BTreeMap::from([
                (Reverse(Decimal::from(2)), Decimal::from(200)),
                (Reverse(Decimal::from(1)), Decimal::from(100))
            ])
        );
    }
}
