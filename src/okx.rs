use futures::stream::SplitSink;
use futures::stream::SplitStream;
use futures::stream::StreamExt;
use futures::SinkExt;
use reqwest::Client as cl;
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tungstenite::Error;
use tungstenite::Message;
use url::Url;

use crate::error::OrderBookError;
use crate::error::Result;
use crate::OrderBook;
use crate::Side;

// Currency is an enum that represents the currency in okx. todo: use just strings?
pub enum Currency {
    Btc,
    Usdt,
}

// Instrument is a pair of currencies
pub type Instrument = (Currency, Currency);

impl Currency {
    fn as_str(&self) -> &str {
        match self {
            Currency::Btc => "BTC",
            Currency::Usdt => "USDT",
        }
    }
}

// Channel is an enum that represents the channel in okx.
pub enum Channel {
    Books5,
}

impl Channel {
    fn as_str(&self) -> &str {
        match self {
            Channel::Books5 => "books5",
        }
    }
}

// Op is an enum that represents the websocket operation.
pub enum WsOp {
    Subscribe,
    Unsubscribe,
}

impl WsOp {
    fn as_str(&self) -> &str {
        match self {
            WsOp::Subscribe => "subscribe",
            WsOp::Unsubscribe => "unsubscribe",
        }
    }
}

// OrderBookAction is an enum that represents the action in the order book.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OrderBookAction {
    Snapshot,
    Update,
}

// OrderBookLevel is a struct that represents a level in the order book.
#[derive(Debug, Deserialize)]
struct OrderBookLevel(
    String, // price
    String, // size
    String,
    String,
);

// OrderBookData is a struct that represents the data in the order book.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderBookData {
    asks: Vec<OrderBookLevel>,
    bids: Vec<OrderBookLevel>,
    ts: String,
    seq_id: u64,
    inst_id: String,
}

// OrderBookWSResponse is a struct that represents the response from the order book websocket.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderBookWSResponse {
    action: Option<OrderBookAction>,
    data: Vec<OrderBookData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OrderBookHandshakeEvent {
    Subscribe,
    Error,
}

// OrderBookWSResponse is a struct that represents the response from the order book websocket.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderBookWSHandshakeResponse {
    event: OrderBookHandshakeEvent,
    code: Option<String>,
    msg: Option<String>,
    conn_id: String,
}

// OrderBookResponse is a struct that represents the response from the order book.
#[derive(Debug, Deserialize)]
struct OrderBookResponse {
    code: String,
    msg: String,
    data: Vec<OrderBookData>,
    ts: String,
}

// HTTPClient is a struct that represents the http client.
struct HTTPClient {
    base_url: Url,
}

impl HTTPClient {
    // new creates a new HTTPClient.
    pub fn new(base_url: &str) -> Result<Self> {
        let url = Url::parse(base_url)?;
        Ok(HTTPClient { base_url: url })
    }

    // get_order_book gets the order book from the http client.
    pub async fn get_order_book(&self, inst: Instrument) -> Result<OrderBook> {
        let mut url = self.base_url.join("market/books")?;
        url.set_query(Some(&format!(
            "instId={}-{}",
            inst.0.as_str(),
            inst.1.as_str()
        )));

        let cl = cl::new();
        let resp = cl.get(url.as_str()).send().await?;

        let response: OrderBookResponse = serde_json::from_str(&resp.text().await?)?;
        let mut ob = OrderBook::new();

        // fill asks
        for level in &response.data[0].asks {
            ob.insert_ask(level.0.parse()?, level.1.parse()?);
        }

        // fill bids
        for level in &response.data[0].bids {
            ob.insert_bid(level.0.parse()?, level.1.parse()?);
        }
        ob.ts = response.ts.parse()?;

        Ok(ob)
    }
}

// WSClient is a struct that represents the websocket client.
struct WSClient {
    url: Url,
}

// WSClient is a struct that represents the websocket client.
impl WSClient {
    /// new creates a new WSClient.
    /// # Examples
    /// ```rust
    /// let mut cl =
    ///     WSClient::new("wss://ws.okx.com:8443/ws/v5/public").expect("failed to create okx client");
    /// let mut rx = cl
    ///     .subscribe((Currency::Btc, Currency::Usdt), Channel::Books5)
    ///     .await
    ///     .expect("failed to subscribe");
    /// for ob in rx.recv().await {
    ///     dbg!(&ob?);
    /// }
    /// ```
    pub fn new(url: &str) -> Result<Self> {
        let url = Url::parse(url)?;
        Ok(WSClient { url })
    }

    // subscribe listens to the websocket and return channel of snapshots.
    pub async fn subscribe(
        &mut self,
        inst: Instrument,
        chan: Channel,
    ) -> Result<Receiver<Result<OrderBook>>> {
        let (sender, receiver) = channel(1);
        let mut ob = OrderBook::new();
        let (ws_stream, _) = connect_async(self.url.as_str()).await?;
        let (mut write, mut read) = ws_stream.split();

        make_orderbook_request(WsOp::Subscribe, inst, chan, &mut read, &mut write).await?;

        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(msg) => {
                        let res = process_ws_message(&msg, &mut ob);
                        match res {
                            Ok(v) => match v {
                                Some(v) => {
                                    sender.send(Ok(v)).await.unwrap();
                                }
                                None => break,
                            },

                            Err(err) => {
                                sender.send(Err(err.into())).await.unwrap();
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        sender.send(Err(err.into())).await.unwrap();
                        break;
                    }
                }
            }
        });

        Ok(receiver)
    }

    pub async fn unsubstribe(&mut self, inst: Instrument, chan: Channel) -> Result<()> {
        let (ws_stream, _) = connect_async(self.url.as_str()).await?;
        let (mut write, mut read) = ws_stream.split();

        make_orderbook_request(WsOp::Unsubscribe, inst, chan, &mut read, &mut write).await?;

        Ok(())
    }
}

async fn make_orderbook_request(
    op: WsOp,
    inst: Instrument,
    chan: Channel,
    read: &mut SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
) -> Result<()> {
    let req = r#"
    {
      "op": "{{op}}",
      "args": [
        {
          "channel": "{{channel}}",
          "instId": "{{left_cur}}-{{right_cur}}"
        }
      ]
    }"#;

    let req = req.replace("{{op}}", op.as_str());
    let req = req.replace("{{channel}}", chan.as_str());
    let req = req.replace("{{left_cur}}", inst.0.as_str());
    let req = req.replace("{{right_cur}}", inst.1.as_str());

    write.send(Message::Text(req.to_string())).await?;

    match read.next().await {
        None => return Err(Error::ConnectionClosed.into()),
        Some(msg) => {
            let resp: OrderBookWSHandshakeResponse =
                serde_json::from_str(&msg?.to_string()).unwrap();
            if let OrderBookHandshakeEvent::Error = resp.event {
                return Err(OrderBookError::Connection(resp.msg));
            }
        }
    }

    Ok(())
}

fn process_ws_message(msg: &Message, ob: &mut OrderBook) -> Result<Option<OrderBook>> {
    match msg {
        Message::Text(txt) => {
            print!("{txt}");
            let response: OrderBookWSResponse = serde_json::from_str(&txt)?;
            match response.action {
                Some(OrderBookAction::Snapshot) | None => {
                    ob.cleanup();

                    for level in &response.data[0].asks {
                        ob.insert_ask(level.0.parse()?, level.1.parse()?);
                    }
                    for level in &response.data[0].bids {
                        ob.insert_bid(level.0.parse()?, level.1.parse()?);
                    }
                }
                Some(OrderBookAction::Update) => {
                    for level in &response.data[0].asks {
                        ob.update_side(Side::Asks, level.0.parse()?, level.1.parse()?);
                    }

                    for level in &response.data[0].bids {
                        ob.update_side(Side::Bids, level.0.parse()?, level.1.parse()?);
                    }
                }
            }

            return Ok(Some(ob.clone()));
        }
        _ => {}
    }

    Ok(None)
}
