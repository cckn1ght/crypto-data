use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::blocking::Client;

use crate::constants;
use crate::constants::{
    HEADERS, KLINE_LIMIT, MAX_RETRIES, RATE_LIMIT_BASE_BACKOFF_MS, RATE_LIMIT_MAX_BACKOFF_MS,
    REQUEST_ERROR_RETRY_DELAY_MS, REQUEST_THROTTLE_MS,
};
use crate::types::{Candle, ExchangeInfo, FetchProps, RawCandle};

pub fn build_http_client() -> Result<Client> {
    Client::builder()
        .no_proxy()
        .build()
        .context("failed to build HTTP client")
}

pub fn generate_query_string(fetch_props: &FetchProps) -> Result<String> {
    let mut query: HashMap<&str, String> = HashMap::from([
        ("symbol", fetch_props.symbol.clone()),
        ("interval", fetch_props.interval.clone()),
        ("startTime", fetch_props.start_time.to_string()),
        ("endTime", fetch_props.end_time.to_string()),
        ("limit", KLINE_LIMIT.to_string()),
    ]);

    if fetch_props.market != constants::MARKET::Spot {
        query.insert("contractType", fetch_props.contract_type.clone());
    }

    let encoded = serde_qs::to_string(&query).context("failed to encode query string")?;
    Ok(format!("klines?{}", encoded))
}

fn csv_file_path(fetch_props: &FetchProps, init_start_time: i64) -> PathBuf {
    let filename = format!(
        "{}_{}_{}_{}_{}.csv",
        fetch_props.symbol,
        fetch_props.market,
        fetch_props.interval,
        init_start_time,
        fetch_props.end_time
    );
    Path::join(Path::new(&fetch_props.director), filename.as_str())
}

fn create_csv_writer(
    fetch_props: &FetchProps,
    init_start_time: i64,
) -> Result<csv::Writer<std::fs::File>> {
    let path = csv_file_path(fetch_props, init_start_time);
    let mut writer = csv::Writer::from_path(path).context("failed to create csv file")?;
    writer
        .write_record(HEADERS)
        .context("failed to write csv header")?;
    Ok(writer)
}

fn write_candles(writer: &mut csv::Writer<std::fs::File>, candles: &[Candle]) -> Result<()> {
    for candle in candles {
        writer
            .write_record([
                candle.open_time.to_string(),
                candle.open.clone(),
                candle.high.clone(),
                candle.low.clone(),
                candle.close.clone(),
                candle.volume.clone(),
                candle.close_time.to_string(),
                candle.quote_asset_volume.clone(),
                candle.number_of_trades.to_string(),
                candle.taker_buy_base_asset_volume.clone(),
                candle.taker_buy_quote_asset_volume.clone(),
            ])
            .context("failed to write candle record")?;
    }
    Ok(())
}

pub fn get_historical_candlesticks_for_symbols(
    client: &Client,
    fetch_props: FetchProps,
    symbols: Vec<String>,
) -> Result<()> {
    let total = symbols.len();
    let pb = indicatif::ProgressBar::new(total as u64);
    let mut failed_symbols: Vec<String> = Vec::new();

    for symbol in symbols {
        let props = FetchProps {
            api_base_url: fetch_props.api_base_url.clone(),
            market: fetch_props.market,
            contract_type: fetch_props.contract_type.clone(),
            symbol: symbol.clone(),
            interval: fetch_props.interval.clone(),
            start_time: fetch_props.start_time,
            end_time: fetch_props.end_time,
            director: fetch_props.director.clone(),
        };

        let path = csv_file_path(&props, fetch_props.start_time);
        if path.exists() {
            pb.println(format!("[+] {} already exists, skipping...", symbol));
            pb.inc(1);
            continue;
        }

        if fetch_symbol_with_retries(client, &props).is_err() {
            failed_symbols.push(symbol.clone());
            pb.println(format!(
                "[-] failed {} after {} retries",
                symbol, MAX_RETRIES
            ));
        } else {
            pb.println(format!("[+] finished {}", symbol));
        }

        pb.inc(1);
    }

    pb.finish_with_message("done");

    if failed_symbols.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("failed symbols: {}", failed_symbols.join(", ")))
    }
}

fn fetch_symbol_with_retries(client: &Client, fetch_props: &FetchProps) -> Result<()> {
    let mut last_error = None;

    for attempt in 1..=MAX_RETRIES {
        match get_historical_candlesticks(client, fetch_props) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if attempt < MAX_RETRIES {
                    let wait_ms = retry_delay_ms(&err, attempt);
                    eprintln!(
                        "Error fetching historical candlesticks for {}: {}. Retrying in {} ms...",
                        fetch_props.symbol, err, wait_ms
                    );
                    sleep(Duration::from_millis(wait_ms));
                }
                last_error = Some(err);
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow!("unknown error while fetching {}", fetch_props.symbol)))
}

fn get_historical_candlesticks(client: &Client, fetch_props: &FetchProps) -> Result<()> {
    if fetch_props.start_time > fetch_props.end_time {
        return Err(anyhow!("start_time cannot be greater than end_time"));
    }

    let init_start_time = fetch_props.start_time;
    let mut writer = create_csv_writer(fetch_props, init_start_time)?;
    let mut next_start_time = init_start_time;

    loop {
        let mut current_fetch = fetch_props.clone();
        current_fetch.start_time = next_start_time;

        let page = fetch_candles_page(client, &current_fetch)?;
        if page.is_empty() {
            break;
        }

        let is_finished = page.len() < KLINE_LIMIT;
        let last_open_time = page
            .last()
            .map(|c| c.open_time)
            .context("missing last candle for pagination")?;
        let mut rows = page;

        if !is_finished {
            rows.pop();
        }

        write_candles(&mut writer, &rows)?;

        if is_finished {
            break;
        }

        next_start_time = last_open_time;
    }

    writer.flush().context("failed to flush csv writer")?;
    Ok(())
}

fn fetch_candles_page(client: &Client, fetch_props: &FetchProps) -> Result<Vec<Candle>> {
    let query = generate_query_string(fetch_props)?;
    let url = format!("{}{}", fetch_props.api_base_url, query);

    sleep(Duration::from_millis(REQUEST_THROTTLE_MS));

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("request failed for symbol {}", fetch_props.symbol))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "<failed to read response body>".to_string());
        return Err(anyhow!(
            "non-success status {} for symbol {}: {}",
            status,
            fetch_props.symbol,
            body
        ));
    }

    let raw_candles: Vec<RawCandle> = response
        .json()
        .with_context(|| format!("invalid kline response for symbol {}", fetch_props.symbol))?;

    Ok(raw_candles.into_iter().map(Candle::from_raw).collect())
}

fn retry_delay_ms(error: &anyhow::Error, attempt: usize) -> u64 {
    if is_rate_limit_error(error) {
        return rate_limit_backoff_ms(attempt);
    }
    REQUEST_ERROR_RETRY_DELAY_MS
}

fn is_rate_limit_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("status 429") || message.contains("status 418")
}

fn rate_limit_backoff_ms(attempt: usize) -> u64 {
    let shift = attempt.saturating_sub(1) as u32;
    let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
    let wait = RATE_LIMIT_BASE_BACKOFF_MS.saturating_mul(multiplier);
    wait.min(RATE_LIMIT_MAX_BACKOFF_MS)
}

pub fn get_trading_symbols(client: &Client, api_base_url: &str) -> Result<Vec<String>> {
    let url = format!("{}{}", api_base_url, "exchangeInfo");
    let response = client
        .get(&url)
        .send()
        .context("failed to request exchangeInfo")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "<failed to read response body>".to_string());
        return Err(anyhow!(
            "exchangeInfo request failed with {}: {}",
            status,
            body
        ));
    }

    let exchange_info: ExchangeInfo = response
        .json()
        .context("failed to decode exchangeInfo response")?;

    let symbols = exchange_info
        .symbols
        .into_iter()
        .filter(|symbol| match symbol.status.as_deref() {
            Some(status) => status == "TRADING",
            None => true,
        })
        .map(|symbol| symbol.symbol)
        .collect();

    Ok(symbols)
}

pub fn parse_date_time(date_string: &str) -> Result<DateTime<Utc>> {
    if let Ok(timestamp) = date_string.parse::<i64>() {
        if let Some(date) = DateTime::from_timestamp_millis(timestamp) {
            return Ok(date);
        }
    }

    if let Ok(naive_datetime) = NaiveDateTime::parse_from_str(date_string, "%Y-%m-%d %H:%M:%S") {
        return Ok(naive_datetime.and_utc());
    }

    Err(anyhow!(
        "failed to parse datetime: {} (expected unix ms or %Y-%m-%d %H:%M:%S)",
        date_string
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{KLINE_LIMIT, MARKET};
    use crate::types::FetchProps;

    fn base_props(market: MARKET) -> FetchProps {
        FetchProps {
            api_base_url: "http://127.0.0.1:9/".to_string(),
            market,
            contract_type: "PERPETUAL".to_string(),
            symbol: "BTCUSDT".to_string(),
            interval: "1h".to_string(),
            start_time: 1_700_000_000_123,
            end_time: 1_700_003_600_123,
            director: ".".to_string(),
        }
    }

    #[test]
    fn parse_date_time_preserves_unix_milliseconds() {
        let unix_ms = 1_700_000_000_123_i64;
        let parsed = parse_date_time(&unix_ms.to_string()).expect("should parse unix ms");
        assert_eq!(parsed.timestamp_millis(), unix_ms);
    }

    #[test]
    fn parse_date_time_accepts_datetime_string() {
        let parsed = parse_date_time("2022-01-01 00:00:00").expect("should parse datetime string");
        assert_eq!(parsed.timestamp(), 1_640_995_200);
    }

    #[test]
    fn generate_query_string_uses_limit_constant_for_spot() {
        let props = base_props(MARKET::Spot);
        let query = generate_query_string(&props).expect("should generate query");

        assert!(query.contains(&format!("limit={}", KLINE_LIMIT)));
        assert!(!query.contains("contractType"));
        assert!(query.contains("symbol=BTCUSDT"));
    }

    #[test]
    fn generate_query_string_includes_contract_type_for_futures() {
        let props = base_props(MARKET::UsdFutures);
        let query = generate_query_string(&props).expect("should generate query");
        assert!(query.contains("contractType=PERPETUAL"));
    }

    #[test]
    fn parse_date_time_returns_error_for_invalid_value() {
        let result = parse_date_time("not-a-date");
        assert!(result.is_err());
    }

    #[test]
    fn rate_limit_backoff_is_exponential_and_capped() {
        assert_eq!(rate_limit_backoff_ms(1), RATE_LIMIT_BASE_BACKOFF_MS);
        assert_eq!(rate_limit_backoff_ms(2), RATE_LIMIT_BASE_BACKOFF_MS * 2);
        assert_eq!(rate_limit_backoff_ms(3), RATE_LIMIT_BASE_BACKOFF_MS * 4);
        assert_eq!(rate_limit_backoff_ms(20), RATE_LIMIT_MAX_BACKOFF_MS);
    }
}
