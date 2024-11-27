use std::result;

use thiserror::Error;

// Define a custom error type
pub type Result<T> = result::Result<T, OrderBookError>;

#[derive(Error, Debug)]
pub enum OrderBookError {
    #[error("connection error: {0:?}")]
    Connection(Option<String>),
    #[error("parse address: {0:?}")]
    ParseAddress(#[from] url::ParseError),
    #[error("tokio tunstenite: {0:?}")]
    TokioTungstenite(#[from] tungstenite::Error),
    #[error("serde: {0:?}")]
    Serde(#[from] serde_json::Error),
    #[error("parse float: {0:?}")]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[error("parse int: {0:?}")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("reqwest: {0:?}")]
    Reqwest(#[from] reqwest::Error),
}
