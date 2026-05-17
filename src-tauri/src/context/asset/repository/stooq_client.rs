use crate::context::asset::domain::PriceProvider;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::time::Duration;

const STOOQ_URL_TEMPLATE: &str = "https://stooq.com/q/l/?s={symbol}&f=sd2t2ohlcv&h&e=csv";
const REQUEST_TIMEOUT_SECS: u64 = 10;
const MICROS_PER_UNIT: f64 = 1_000_000.0;
const CSV_CLOSE_COLUMN_INDEX: usize = 6;

/// Production [`PriceProvider`] backed by Stooq's CSV endpoint (ADR-008).
pub struct ReqwestStooqClient {
    client: reqwest::Client,
}

impl Default for ReqwestStooqClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestStooqClient {
    /// Creates a new client with a 10-second per-request timeout.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .expect("reqwest client build");
        Self { client }
    }
}

#[async_trait]
impl PriceProvider for ReqwestStooqClient {
    async fn fetch_price(&self, symbol: &str) -> Result<i64> {
        let url = STOOQ_URL_TEMPLATE.replace("{symbol}", symbol);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Stooq fetch request failed for symbol: {symbol}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("Stooq returned {} for symbol {symbol}", resp.status());
        }

        let body = resp
            .text()
            .await
            .with_context(|| format!("Stooq response read failed for symbol: {symbol}"))?;

        parse_close_micros(&body)
            .with_context(|| format!("Stooq response parse failed for symbol: {symbol}"))
    }
}

fn parse_close_micros(csv: &str) -> Result<i64> {
    let data_row = csv
        .lines()
        .nth(1)
        .ok_or_else(|| anyhow!("missing data row"))?;
    let close = data_row
        .split(',')
        .nth(CSV_CLOSE_COLUMN_INDEX)
        .ok_or_else(|| anyhow!("missing close column"))?;
    let price: f64 = close
        .trim()
        .parse()
        .map_err(|e| anyhow!("close not numeric ({close:?}): {e}"))?;
    if !price.is_finite() || price <= 0.0 {
        return Err(anyhow!("close is non-finite or non-positive: {price}"));
    }
    Ok((price * MICROS_PER_UNIT).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_close_from_well_formed_csv() {
        let csv = "Symbol,Date,Time,Open,High,Low,Close,Volume\n\
                   AAPL.US,2026-05-16,21:55:00,189.50,190.20,188.75,189.95,12345678";
        let micros = parse_close_micros(csv).unwrap();
        assert_eq!(micros, 189_950_000);
    }

    #[test]
    fn rejects_missing_data_row() {
        let csv = "Symbol,Date,Time,Open,High,Low,Close,Volume\n";
        assert!(parse_close_micros(csv).is_err());
    }

    #[test]
    fn rejects_non_numeric_close() {
        let csv = "Symbol,Date,Time,Open,High,Low,Close,Volume\n\
                   AAPL.US,2026-05-16,21:55:00,189.50,190.20,188.75,N/D,0";
        assert!(parse_close_micros(csv).is_err());
    }

    #[test]
    fn rejects_non_positive_close() {
        let csv = "Symbol,Date,Time,Open,High,Low,Close,Volume\n\
                   AAPL.US,2026-05-16,21:55:00,0,0,0,0,0";
        assert!(parse_close_micros(csv).is_err());
    }
}
