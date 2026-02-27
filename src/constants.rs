use clap::ValueEnum;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

pub const SPOT_API_BASE_URL: &str = "https://api.binance.com/api/v3/";
pub const USD_FUTURES_API_BASE_URL: &str = "https://fapi.binance.com/fapi/v1/";
pub const COIN_FUTURES_API_BASE_URL: &str = "https://dapi.binance.com/dapi/v1/";
#[derive(
    Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Hash,
)]
pub enum MARKET {
    Spot,
    UsdFutures,
    CoinFutures,
}

impl fmt::Display for MARKET {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
pub static MARKET_BASE_URL: Lazy<HashMap<MARKET, &'static str>> = Lazy::new(|| {
    [
        (MARKET::Spot, SPOT_API_BASE_URL),
        (MARKET::UsdFutures, USD_FUTURES_API_BASE_URL),
        (MARKET::CoinFutures, COIN_FUTURES_API_BASE_URL),
    ]
    .iter()
    .cloned()
    .collect()
});

pub const HEADERS: [&str; 11] = [
    "Open_Time",
    "Open",
    "High",
    "Low",
    "Close",
    "Volume",
    "CloseTime",
    "Quote_Asset_Volume",
    "Number_Of_Trades",
    "Taker_Buy_Base_Asset_Volume",
    "Taker_Buy_Quote_Asset_Volume",
];
pub const KLINE_LIMIT: usize = 1000;
pub const MAX_RETRIES: usize = 6;
pub const REQUEST_THROTTLE_MS: u64 = 150;
pub const REQUEST_ERROR_RETRY_DELAY_MS: u64 = 1_500;
pub const RATE_LIMIT_BASE_BACKOFF_MS: u64 = 3_000;
pub const RATE_LIMIT_MAX_BACKOFF_MS: u64 = 30_000;
