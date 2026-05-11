use super::error::AssetPriceDomainError;
use anyhow::Result;
use async_trait::async_trait;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use specta::Type;

/// A manually recorded market price for a financial asset on a specific date.
/// Owned by the `asset` bounded context (MKT spec).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AssetPrice {
    /// ID of the asset whose market price this record describes.
    pub asset_id: String,
    /// ISO 8601 calendar date of the price observation (e.g. "2026-04-26").
    pub date: String,
    /// Market price per unit in the asset's native currency (i64 micro-units, ADR-001).
    pub price: i64,
}

impl AssetPrice {
    // `with_id()` is not applicable: AssetPrice has no surrogate ID.
    // Its identity is the composite natural key (asset_id, date).

    /// Creates a new AssetPrice after validating price > 0 (MKT-021) and
    /// date is well-formed ISO 8601 and not in the future (MKT-022).
    pub fn new(
        asset_id: String,
        date: String,
        price: i64,
    ) -> std::result::Result<Self, AssetPriceDomainError> {
        if price <= 0 {
            return Err(AssetPriceDomainError::NotPositive);
        }
        let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map_err(|_| AssetPriceDomainError::InvalidDateFormat { date: date.clone() })?;
        let today = chrono::Local::now().date_naive();
        if parsed > today {
            return Err(AssetPriceDomainError::DateInFuture);
        }
        Ok(Self {
            asset_id,
            date,
            price,
        })
    }

    /// Restores an AssetPrice from storage without validation (B1 — restore factory).
    pub fn restore(asset_id: String, date: String, price: i64) -> Self {
        Self {
            asset_id,
            date,
            price,
        }
    }
}

/// Interface for AssetPrice persistence (upsert by (asset_id, date), MKT-025).
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AssetPriceRepository: Send + Sync {
    /// Upserts a price record: inserts or overwrites by (asset_id, date) (MKT-025).
    async fn upsert(&self, price: AssetPrice) -> Result<()>;
    /// Returns the most recently dated price for the given asset, or None (MKT-031).
    async fn get_latest(&self, asset_id: &str) -> Result<Option<AssetPrice>>;
    /// Returns all recorded prices for the given asset, ordered by date descending (MKT-072).
    async fn get_all_for_asset(&self, asset_id: &str) -> Result<Vec<AssetPrice>>;
    /// Returns the price record for the given (asset_id, date) pair, or None (MKT-083).
    async fn get_by_asset_and_date(&self, asset_id: &str, date: &str)
        -> Result<Option<AssetPrice>>;
    /// Deletes the price record for the given (asset_id, date) pair; no-op if absent (MKT-090).
    async fn delete(&self, asset_id: &str, date: &str) -> Result<()>;
    /// Atomically deletes the record at `original_date` and upserts `new_price` (MKT-084).
    async fn replace_atomic(
        &self,
        asset_id: &str,
        original_date: &str,
        new_price: AssetPrice,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // MKT-021 — new() rejects price <= 0
    #[test]
    fn new_rejects_non_positive_price() {
        let err = AssetPrice::new("a".to_string(), "2026-01-01".to_string(), 0).unwrap_err();
        assert!(
            matches!(err, AssetPriceDomainError::NotPositive),
            "got: {err:?}"
        );
        let err = AssetPrice::new("a".to_string(), "2026-01-01".to_string(), -1).unwrap_err();
        assert!(
            matches!(err, AssetPriceDomainError::NotPositive),
            "got: {err:?}"
        );
    }

    // MKT-022 — new() rejects a malformed date string with the offending input echoed back
    #[test]
    fn new_rejects_malformed_date() {
        let err =
            AssetPrice::new("a".to_string(), "not-a-date".to_string(), 1_000_000).unwrap_err();
        assert!(
            matches!(&err, AssetPriceDomainError::InvalidDateFormat { date } if date == "not-a-date"),
            "got: {err:?}"
        );
    }

    // MKT-022 — new() rejects a date that is in the future
    #[test]
    fn new_rejects_future_date() {
        let err =
            AssetPrice::new("a".to_string(), "2099-12-31".to_string(), 1_000_000).unwrap_err();
        assert!(
            matches!(err, AssetPriceDomainError::DateInFuture),
            "got: {err:?}"
        );
    }

    // MKT-021/022 — new() accepts a valid past price and date
    #[test]
    fn new_accepts_valid_past_price_and_date() {
        let ap =
            AssetPrice::new("asset-1".to_string(), "2026-01-01".to_string(), 100_000_000).unwrap();
        assert_eq!(ap.asset_id, "asset-1");
        assert_eq!(ap.date, "2026-01-01");
        assert_eq!(ap.price, 100_000_000);
    }

    // B1 restore — restore() round-trips fields without validation (negative price + future date accepted)
    #[test]
    fn restore_roundtrips_without_validation() {
        let ap = AssetPrice::restore("x".to_string(), "2099-01-01".to_string(), -1);
        assert_eq!(ap.asset_id, "x");
        assert_eq!(ap.date, "2099-01-01");
        assert_eq!(ap.price, -1);
    }
}
