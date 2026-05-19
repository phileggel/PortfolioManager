use super::category::AssetCategory;
use super::error::AssetDomainError;
use super::exchange::{self, Exchange};
use anyhow::Result;
use async_trait::async_trait;
use iso_currency::Currency;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::result::Result as StdResult;
use std::str::FromStr;
use uuid::Uuid;

/// Represents the classification of an asset.

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Default,
    Clone,
    Type,
    PartialEq,
    Eq,
    strum_macros::Display,
    strum_macros::EnumString,
)]
pub enum AssetClass {
    /// Real estate properties or REITs.
    RealEstate,
    /// Fiat currency or highly liquid equivalents.
    #[default]
    Cash,
    /// Individual company equities.
    Stocks,
    /// Fixed income securities.
    Bonds,
    /// Exchange Traded Funds.
    ETF,
    /// Managed investment funds.
    MutualFunds,
    /// Cryptocurrencies or other blockchain-based assets.
    DigitalAsset,
    /// Leveraged or contingent instruments derived from an underlying asset (warrants, options, futures, rights).
    Derivatives,
}

impl AssetClass {
    /// Returns the default risk level for this asset class (R3).
    pub fn default_risk(&self) -> u8 {
        match self {
            AssetClass::Cash => 1,
            AssetClass::Bonds => 2,
            AssetClass::RealEstate => 2,
            AssetClass::MutualFunds => 3,
            AssetClass::ETF => 3,
            AssetClass::Stocks => 4,
            AssetClass::DigitalAsset => 5,
            AssetClass::Derivatives => 5,
        }
    }
}

/// A financial instrument or resource held by a user.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Asset {
    /// Unique identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Asset classification.
    pub class: AssetClass,
    /// Category link.
    pub category: AssetCategory,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Risk score from 1 to 5.
    pub risk_level: u8,
    /// Identifier like ticker or ISIN.
    pub reference: String,
    /// Whether the asset is archived (soft-archived, reversible).
    pub is_archived: bool,
    /// Optional canonical trading venue (AST-021).
    pub exchange: Option<Exchange>,
}

impl Asset {
    /// Creates a new Asset.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        class: AssetClass,
        category: AssetCategory,
        currency: String,
        risk_level: u8,
        reference: String,
        exchange: Option<Exchange>,
    ) -> StdResult<Self, AssetDomainError> {
        Self::validate(&name, risk_level, &currency, &reference, exchange.as_ref())?;

        let reference = reference.trim().to_uppercase();

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name,
            class,
            category,
            currency,
            risk_level,
            reference,
            is_archived: false,
            exchange,
        })
    }

    /// Reconstructs an Asset with a known ID (used for updates).
    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        asset_id: String,
        name: String,
        class: AssetClass,
        category: AssetCategory,
        currency: String,
        risk_level: u8,
        reference: String,
        is_archived: bool,
        exchange: Option<Exchange>,
    ) -> StdResult<Self, AssetDomainError> {
        Self::validate(&name, risk_level, &currency, &reference, exchange.as_ref())?;

        let reference = reference.trim().to_uppercase();

        Ok(Self {
            id: asset_id,
            name,
            class,
            category,
            currency,
            risk_level,
            reference,
            is_archived,
            exchange,
        })
    }

    fn validate(
        name: &str,
        risk_level: u8,
        currency: &str,
        reference: &str,
        exchange: Option<&Exchange>,
    ) -> StdResult<(), AssetDomainError> {
        if name.trim().is_empty() {
            return Err(AssetDomainError::NameEmpty);
        }
        if reference.trim().is_empty() {
            return Err(AssetDomainError::ReferenceEmpty);
        }
        if !(1..=5).contains(&risk_level) {
            return Err(AssetDomainError::InvalidRiskLevel {
                received: risk_level,
            });
        }
        if Currency::from_str(currency).is_err() {
            return Err(AssetDomainError::InvalidCurrency {
                currency: currency.to_string(),
            });
        }
        if let Some(exchange) = exchange {
            if exchange::lookup(&exchange.code).is_none() {
                return Err(AssetDomainError::InvalidExchange {
                    exchange_code: exchange.code.clone(),
                });
            }
        }
        Ok(())
    }

    /// Restores an Asset from storage (no validation — already validated at write time).
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        asset_id: String,
        name: String,
        class: AssetClass,
        category: AssetCategory,
        currency: String,
        risk_level: u8,
        reference: String,
        is_archived: bool,
        exchange: Option<Exchange>,
    ) -> Self {
        Self {
            id: asset_id,
            name,
            class,
            category,
            currency,
            risk_level,
            reference,
            is_archived,
            exchange,
        }
    }

    /// Returns true if this is the system Cash Asset (CSH-016 / CSH-017).
    fn is_cash(&self) -> bool {
        self.class == AssetClass::Cash
    }

    /// Aggregate-level invariant: the system Cash Asset cannot be edited, archived,
    /// unarchived, or deleted by the user (CSH-016).
    pub fn ensure_user_managed(&self) -> Result<(), AssetDomainError> {
        if self.is_cash() {
            return Err(AssetDomainError::CashAssetNotEditable);
        }
        Ok(())
    }

    /// Aggregate-level invariant: an archived asset cannot be edited (R6). Archive
    /// must be reverted (`unarchive`) first.
    pub fn ensure_not_archived(&self) -> Result<(), AssetDomainError> {
        if self.is_archived {
            return Err(AssetDomainError::Archived);
        }
        Ok(())
    }

    /// Aggregate root method: applies an edit to this asset. Enforces the
    /// system-asset (CSH-016) and not-archived (R6) invariants on the loaded
    /// state, then validates the proposed input. Returns the updated `Asset`
    /// for the caller to persist.
    // The argument count mirrors the field set of Asset and matches the
    // existing `with_id` factory (B32 precedent above). It cannot be split
    // without introducing an intermediate value object.
    #[allow(clippy::too_many_arguments)]
    pub fn update_from(
        self,
        name: String,
        class: AssetClass,
        category: AssetCategory,
        currency: String,
        risk_level: u8,
        reference: String,
        exchange: Option<Exchange>,
    ) -> Result<Self, AssetDomainError> {
        self.ensure_user_managed()?;
        self.ensure_not_archived()?;
        Self::validate(&name, risk_level, &currency, &reference, exchange.as_ref())?;
        let reference = reference.trim().to_uppercase();
        Ok(Self {
            id: self.id,
            name,
            class,
            category,
            currency,
            risk_level,
            reference,
            is_archived: self.is_archived,
            exchange,
        })
    }

    /// Aggregate root method: archives this asset (R6 — reversible).
    /// Enforces the system-asset invariant (CSH-016).
    pub fn archive(self) -> Result<Self, AssetDomainError> {
        self.ensure_user_managed()?;
        Ok(Self {
            is_archived: true,
            ..self
        })
    }

    /// Aggregate root method: unarchives this asset (R18). Enforces the
    /// system-asset invariant (CSH-016).
    pub fn unarchive(self) -> Result<Self, AssetDomainError> {
        self.ensure_user_managed()?;
        Ok(Self {
            is_archived: false,
            ..self
        })
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;

    fn equity(id: &str, archived: bool) -> Asset {
        Asset::restore(
            id.to_string(),
            "Apple".to_string(),
            AssetClass::Stocks,
            AssetCategory::default(),
            "USD".to_string(),
            3,
            "AAPL".to_string(),
            archived,
            None,
        )
    }

    fn cash() -> Asset {
        Asset::restore(
            "cash-usd".to_string(),
            "USD Cash".to_string(),
            AssetClass::Cash,
            AssetCategory::default(),
            "USD".to_string(),
            1,
            "USD".to_string(),
            false,
            None,
        )
    }

    // CSH-016 — system Cash Asset cannot be edited via update_from.
    #[test]
    fn update_from_rejects_system_cash_asset() {
        let err = cash()
            .update_from(
                "Renamed".into(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".into(),
                3,
                "AAPL".into(),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, AssetDomainError::CashAssetNotEditable));
    }

    // R6 — archived asset cannot be edited via update_from.
    #[test]
    fn update_from_rejects_archived_asset() {
        let err = equity("a1", true)
            .update_from(
                "Renamed".into(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".into(),
                3,
                "AAPL".into(),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, AssetDomainError::Archived));
    }

    // update_from validates input after the state checks pass.
    #[test]
    fn update_from_rejects_empty_name() {
        let err = equity("a1", false)
            .update_from(
                "".into(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".into(),
                3,
                "AAPL".into(),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, AssetDomainError::NameEmpty));
    }

    // CSH-016 — system Cash Asset cannot be archived.
    #[test]
    fn archive_rejects_system_cash_asset() {
        assert!(matches!(
            cash().archive().unwrap_err(),
            AssetDomainError::CashAssetNotEditable
        ));
    }

    // CSH-016 — system Cash Asset cannot be unarchived.
    #[test]
    fn unarchive_rejects_system_cash_asset() {
        assert!(matches!(
            cash().unarchive().unwrap_err(),
            AssetDomainError::CashAssetNotEditable
        ));
    }

    // archive sets is_archived = true on a regular asset.
    #[test]
    fn archive_sets_archived_flag_on_user_asset() {
        let archived = equity("a1", false).archive().unwrap();
        assert!(archived.is_archived);
    }

    // unarchive clears is_archived on a regular asset.
    #[test]
    fn unarchive_clears_archived_flag_on_user_asset() {
        let unarchived = equity("a1", true).unarchive().unwrap();
        assert!(!unarchived.is_archived);
    }

    // ensure_user_managed rejects the system Cash Asset (used by delete service path).
    #[test]
    fn ensure_user_managed_rejects_system_cash_asset() {
        assert!(matches!(
            cash().ensure_user_managed().unwrap_err(),
            AssetDomainError::CashAssetNotEditable
        ));
    }

    // update_from rewrites every mutable field while preserving id and is_archived.
    #[test]
    fn update_from_applies_all_field_changes() {
        let updated = equity("a1", false)
            .update_from(
                "Microsoft".into(),
                AssetClass::ETF,
                AssetCategory::from_storage("cat-tech".into(), "Tech".into()),
                "EUR".into(),
                2,
                "MSFT".into(),
                None,
            )
            .unwrap();
        assert_eq!(updated.id, "a1");
        assert!(!updated.is_archived);
        assert_eq!(updated.name, "Microsoft");
        assert_eq!(updated.class, AssetClass::ETF);
        assert_eq!(updated.category.id, "cat-tech");
        assert_eq!(updated.currency, "EUR");
        assert_eq!(updated.risk_level, 2);
        assert_eq!(updated.reference, "MSFT");
    }

    // update_from normalizes reference: trims whitespace and uppercases.
    #[test]
    fn update_from_normalizes_reference() {
        let updated = equity("a1", false)
            .update_from(
                "Apple".into(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".into(),
                3,
                "  msft  ".into(),
                None,
            )
            .unwrap();
        assert_eq!(updated.reference, "MSFT");
    }

    // CSH-016 takes precedence over R6: a system Cash Asset that is also archived
    // must surface CashAssetNotEditable, not Archived (cash check runs first).
    #[test]
    fn update_from_check_order_cash_before_archived() {
        let archived_cash = Asset::restore(
            "cash-usd".into(),
            "USD Cash".into(),
            AssetClass::Cash,
            AssetCategory::default(),
            "USD".into(),
            1,
            "USD".into(),
            true,
            None,
        );
        let err = archived_cash
            .update_from(
                "x".into(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".into(),
                3,
                "AAPL".into(),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, AssetDomainError::CashAssetNotEditable));
    }

    // archive preserves all other fields (only is_archived flips).
    #[test]
    fn archive_preserves_other_fields() {
        let before = equity("a1", false);
        let after = before.clone().archive().unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.name, before.name);
        assert_eq!(after.class, before.class);
        assert_eq!(after.category.id, before.category.id);
        assert_eq!(after.currency, before.currency);
        assert_eq!(after.risk_level, before.risk_level);
        assert_eq!(after.reference, before.reference);
    }

    // unarchive preserves all other fields (only is_archived flips).
    #[test]
    fn unarchive_preserves_other_fields() {
        let before = equity("a1", true);
        let after = before.clone().unarchive().unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.name, before.name);
        assert_eq!(after.class, before.class);
        assert_eq!(after.category.id, before.category.id);
        assert_eq!(after.currency, before.currency);
        assert_eq!(after.risk_level, before.risk_level);
        assert_eq!(after.reference, before.reference);
    }
}

#[cfg(test)]
mod exchange_tests {
    use super::super::exchange::Exchange;
    use super::*;

    /// Constructs a canonical exchange value for use in tests.
    fn xpar() -> Exchange {
        super::super::exchange::lookup("XPAR").expect("XPAR must be in the curated set")
    }

    /// Constructs a non-canonical exchange value for use in tests (AST-001 rejection path).
    fn bogus_exchange() -> Exchange {
        Exchange {
            code: "BOGUS".to_string(),
            label: "Bogus Exchange".to_string(),
        }
    }

    fn equity_with_exchange(id: &str, exchange: Option<Exchange>) -> Asset {
        Asset::restore(
            id.to_string(),
            "Apple".to_string(),
            AssetClass::Stocks,
            AssetCategory::default(),
            "USD".to_string(),
            3,
            "AAPL".to_string(),
            false,
            exchange,
        )
    }

    // Asset::restore round-trips exchange = None
    #[test]
    fn restore_accepts_exchange_none() {
        let asset = equity_with_exchange("a1", None);
        assert!(asset.exchange.is_none());
    }

    // Asset::restore round-trips exchange = Some(canonical)
    #[test]
    fn restore_accepts_canonical_exchange() {
        let asset = equity_with_exchange("a1", Some(xpar()));
        let exchange = asset.exchange.expect("exchange should be Some");
        assert_eq!(exchange.code, "XPAR");
    }

    // Asset::new accepts exchange = None (AST-001 — absent is always valid)
    #[test]
    fn new_accepts_no_exchange() {
        let result = Asset::new(
            "Apple".to_string(),
            AssetClass::Stocks,
            AssetCategory::default(),
            "USD".to_string(),
            3,
            "AAPL".to_string(),
            None,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().exchange.is_none());
    }

    // Asset::new accepts exchange = Some(canonical) (AST-001 — curated membership passes)
    #[test]
    fn new_accepts_canonical_exchange() {
        let result = Asset::new(
            "Air Liquide".to_string(),
            AssetClass::Stocks,
            AssetCategory::default(),
            "EUR".to_string(),
            4,
            "AI".to_string(),
            Some(xpar()),
        );
        assert!(result.is_ok());
        let asset = result.unwrap();
        let exchange = asset.exchange.expect("exchange should be Some");
        assert_eq!(exchange.code, "XPAR");
    }

    // Asset::new rejects exchange = Some(non-curated) with InvalidExchange (AST-001)
    #[test]
    fn new_rejects_non_curated_exchange() {
        let err = Asset::new(
            "Some Asset".to_string(),
            AssetClass::Stocks,
            AssetCategory::default(),
            "USD".to_string(),
            3,
            "REF".to_string(),
            Some(bogus_exchange()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, AssetDomainError::InvalidExchange { exchange_code } if exchange_code == "BOGUS"),
            "expected InvalidExchange {{ code: \"BOGUS\" }}, got: {err:?}"
        );
    }

    // update_from accepts exchange = None → no exchange after update (AST-022 clear)
    #[test]
    fn update_from_clears_exchange_when_none_passed() {
        let asset = equity_with_exchange("a1", Some(xpar()));
        let updated = asset
            .update_from(
                "Apple".to_string(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".to_string(),
                3,
                "AAPL".to_string(),
                None,
            )
            .unwrap();
        assert!(updated.exchange.is_none());
    }

    // update_from accepts exchange = Some(canonical) when currently None (AST-022 set)
    #[test]
    fn update_from_sets_exchange_when_previously_none() {
        let asset = equity_with_exchange("a1", None);
        let updated = asset
            .update_from(
                "Air Liquide".to_string(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "EUR".to_string(),
                4,
                "AI".to_string(),
                Some(xpar()),
            )
            .unwrap();
        let exchange = updated
            .exchange
            .expect("exchange should be Some after update");
        assert_eq!(exchange.code, "XPAR");
    }

    // update_from changes exchange from one canonical value to another (AST-022 change)
    #[test]
    fn update_from_changes_exchange_to_different_canonical_value() {
        let initial =
            super::super::exchange::lookup("XNAS").expect("XNAS must be in the curated set");
        let asset = equity_with_exchange("a1", Some(initial));
        let updated = asset
            .update_from(
                "Air Liquide".to_string(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "EUR".to_string(),
                4,
                "AI".to_string(),
                Some(xpar()),
            )
            .unwrap();
        let exchange = updated
            .exchange
            .expect("exchange should be Some after update");
        assert_eq!(exchange.code, "XPAR");
    }

    // update_from rejects non-curated exchange with InvalidExchange (AST-001)
    #[test]
    fn update_from_rejects_non_curated_exchange() {
        let asset = equity_with_exchange("a1", None);
        let err = asset
            .update_from(
                "Some Asset".to_string(),
                AssetClass::Stocks,
                AssetCategory::default(),
                "USD".to_string(),
                3,
                "REF".to_string(),
                Some(bogus_exchange()),
            )
            .unwrap_err();
        assert!(
            matches!(&err, AssetDomainError::InvalidExchange { exchange_code } if exchange_code == "BOGUS"),
            "expected InvalidExchange {{ code: \"BOGUS\" }}, got: {err:?}"
        );
    }
}

/// Interface for asset persistence.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AssetRepository: Send + Sync {
    /// Fetches all active (non-archived) assets.
    async fn get_all(&self) -> Result<Vec<Asset>>;
    /// Fetches all assets including archived ones.
    async fn get_all_including_archived(&self) -> Result<Vec<Asset>>;
    /// Fetches an asset by its ID.
    async fn get_by_id(&self, id: &str) -> Result<Option<Asset>>;
    /// Persists a new asset.
    async fn create(&self, asset: Asset) -> Result<Asset>;
    /// Updates an existing asset.
    async fn update(&self, asset: Asset) -> Result<Asset>;
    /// Soft-deletes an asset.
    async fn delete(&self, id: &str) -> Result<()>;
    /// Archives an asset (reversible).
    async fn archive(&self, id: &str) -> Result<()>;
    /// Unarchives an asset.
    async fn unarchive(&self, id: &str) -> Result<()>;
}
