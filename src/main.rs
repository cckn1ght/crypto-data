use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};

mod constants;
mod types;
mod utils;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    // #[arg(short, long, default_value = "spot")]
    // market: constants::MARKET,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all symbols of a market
    ListSymbols {
        #[arg(short, long, default_value = "spot")]
        market: constants::MARKET,
    },
    /// download klines of a market, either all symbols or specific symbols
    GetKlines {
        /// market to download klines from
        #[arg(short, long, default_value = "spot")]
        market: constants::MARKET,

        /// If true, download klines of all symbols, otherwise download klines of specific symbols
        #[arg(long, default_value = "false")]
        all: bool,

        /// Interval of the klines, supports "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w", "1M"
        #[arg(short, long, default_value = "1h")]
        interval: String,

        /// UTC Start Time, supports both unix timestamp in milliseconds or date string in this format: "2022-01-01 00:00:00" . Default is 30 days ago of the end time
        #[arg(long)]
        start_time: Option<String>,

        /// UTC End Time, supports both unix timestamp in milliseconds or date string in this format: "2022-01-01 00:00:00" . Default is now()
        #[arg(long)]
        end_time: Option<String>,

        /// Director for saving the downloaded klines, default is current directory
        #[arg(long, short)]
        director: Option<String>,

        symbols: Vec<String>,
    },
}

struct GetKlinesArgs<'a> {
    market: &'a constants::MARKET,
    all: bool,
    interval: &'a str,
    start_time: &'a Option<String>,
    end_time: &'a Option<String>,
    symbols: &'a [String],
    director: &'a Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = utils::build_http_client()?;

    match &cli.command {
        Commands::ListSymbols { market } => handle_list_symbols(&client, market)?,
        Commands::GetKlines {
            market,
            all,
            interval,
            start_time,
            end_time,
            symbols,
            director,
        } => {
            let args = GetKlinesArgs {
                market,
                all: *all,
                interval,
                start_time,
                end_time,
                symbols,
                director,
            };
            handle_get_klines(&client, args)?
        }
    }

    Ok(())
}

fn handle_list_symbols(
    client: &reqwest::blocking::Client,
    market: &constants::MARKET,
) -> Result<()> {
    let api_base_url = constants::MARKET_BASE_URL
        .get(market)
        .context("unsupported market")?;
    let symbols = utils::get_trading_symbols(client, api_base_url)?;
    println!("{:?}", symbols);
    Ok(())
}

fn handle_get_klines(client: &reqwest::blocking::Client, args: GetKlinesArgs) -> Result<()> {
    if !args.all && args.symbols.is_empty() {
        println!("Please provide symbols to download klines from or use --all to download klines of all symbols.");
        return Ok(());
    }

    let output_dir = prepare_output_directory(args.director)?;
    let (start_datetime, end_datetime) = resolve_time_range(args.start_time, args.end_time)?;
    let api_base_url = constants::MARKET_BASE_URL
        .get(args.market)
        .context("unsupported market")?
        .to_string();
    println!(
        "Start time: {}, End time: {}, saving files to: {}",
        start_datetime, end_datetime, output_dir
    );

    let fetch_props = types::FetchProps {
        api_base_url: api_base_url.clone(),
        market: *args.market,
        contract_type: "".to_string(),
        symbol: "".to_string(),
        interval: args.interval.to_string(),
        start_time: start_datetime.timestamp_millis(),
        end_time: end_datetime.timestamp_millis(),
        director: output_dir,
    };

    if args.all {
        let all_symbols = utils::get_trading_symbols(client, &api_base_url)?;
        return utils::get_historical_candlesticks_for_symbols(client, fetch_props, all_symbols);
    }

    let uppercased_symbols = args
        .symbols
        .iter()
        .map(|s| s.to_uppercase())
        .collect::<Vec<String>>();
    utils::get_historical_candlesticks_for_symbols(client, fetch_props, uppercased_symbols)
}

fn prepare_output_directory(director: &Option<String>) -> Result<String> {
    let output_dir = director.clone().unwrap_or_else(|| ".".to_string());
    let path = Path::new(&output_dir);
    if !path.exists() {
        fs::create_dir_all(path).context("failed to create output directory")?;
        println!(
            "Created directory: {}",
            fs::canonicalize(path)
                .context("failed to canonicalize output directory")?
                .to_str()
                .context("invalid output directory unicode")?
        );
    }

    let canonical = fs::canonicalize(path).context("failed to canonicalize output directory")?;
    let text = canonical
        .to_str()
        .context("invalid output directory unicode")?
        .to_string();
    Ok(text)
}

fn resolve_time_range(
    start_time: &Option<String>,
    end_time: &Option<String>,
) -> Result<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
    let end_datetime = match end_time {
        Some(value) => utils::parse_date_time(value)?,
        None => Utc::now(),
    };
    let start_datetime = match start_time {
        Some(value) => utils::parse_date_time(value)?,
        None => end_datetime - Duration::days(30),
    };

    Ok((start_datetime, end_datetime))
}
