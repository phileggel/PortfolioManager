use crate::context::account::{AccountApplicationError, AccountService};
use crate::context::asset::AssetService;
use crate::core::logger::BACKEND;
use serde::Serialize;
use specta::Type;
use std::result::Result as StdResult;
use std::sync::Arc;

/// Enriched view of a single active holding (quantity > 0) with asset metadata (ACD-020).
#[derive(Debug, Serialize, Clone, Type)]
pub struct HoldingDetail {
    /// ID of the held asset.
    pub asset_id: String,
    /// Display name of the asset.
    pub asset_name: String,
    /// Ticker or user-defined reference.
    pub asset_reference: String,
    /// Current units held (i64 micro-units, ADR-001).
    pub quantity: i64,
    /// VWAP purchase price in account currency (i64 micro-units, ADR-001).
    pub average_price: i64,
    /// Total cost of position: quantity × average_price / MICRO (i64 micro-units, ACD-023).
    pub cost_basis: i64,
    /// Sum of realized P&L from all Sell transactions for this asset (i64 micro-units, SEL-042).
    pub realized_pnl: i64,
    /// ISO 4217 currency code of the asset's native currency (MKT-023).
    pub asset_currency: String,
    /// Most recently dated price for this asset in asset currency (i64 micros). None if no price recorded (MKT-031).
    pub current_price: Option<i64>,
    /// ISO date string of the price observation. None when current_price is None (MKT-031).
    pub current_price_date: Option<String>,
    /// Unrealized gain/loss in account currency (i64 micros). None on currency mismatch or no price (MKT-033/034).
    /// 0 (not None) when current price equals average price (MKT-033).
    pub unrealized_pnl: Option<i64>,
    /// Performance percentage as i64 micros (5.25% = 5_250_000). None when unrealized_pnl is None or cost_basis = 0 (MKT-035).
    /// 0 (not None) when unrealized_pnl is 0 (MKT-035).
    pub performance_pct: Option<i64>,
}

/// Enriched view of a fully-closed position (quantity == 0, ACD-044).
#[derive(Debug, Serialize, Clone, Type)]
pub struct ClosedHoldingDetail {
    /// ID of the previously held asset.
    pub asset_id: String,
    /// Display name of the asset.
    pub asset_name: String,
    /// Ticker or user-defined reference.
    pub asset_reference: String,
    /// Total realized P&L for this position (micro-units, ACD-045).
    pub realized_pnl: i64,
    /// ISO date of the most recent sell for this position ("YYYY-MM-DD", ACD-043).
    pub last_sold_date: String,
}

/// Top-level response for the get_account_details command (ACD spec).
#[derive(Debug, Serialize, Clone, Type)]
pub struct AccountDetailsResponse {
    /// Display name of the account (ACD-032).
    pub account_name: String,
    /// Active holdings (quantity > 0), sorted by asset_name asc (ACD-020, ACD-033).
    pub holdings: Vec<HoldingDetail>,
    /// Closed positions (quantity == 0), sorted by asset_name asc (ACD-044, ACD-046).
    pub closed_holdings: Vec<ClosedHoldingDetail>,
    /// Total holding count regardless of quantity (ACD-034).
    pub total_holding_count: i64,
    /// Sum of cost_basis across all active holdings, 0 if none (ACD-031).
    pub total_cost_basis: i64,
    /// Sum of total_realized_pnl across ALL holdings (active + closed), 0 if none (ACD-047).
    pub total_realized_pnl: i64,
    /// Sum of unrealized_pnl across same-currency priced active holdings. None when none qualify (MKT-040).
    pub total_unrealized_pnl: Option<i64>,
    /// Total economic value of the account in account-currency micros (CSH-094):
    /// `cash_holding.quantity + Σ_h (h.quantity × latest_price(h))` over non-cash active holdings.
    /// Unpriced non-cash holdings contribute 0 (no fallback to `average_price`).
    /// Returns 0 when no Cash Holding and no priced non-cash holdings.
    pub total_global_value: i64,
}

/// Orchestrates a cross-context read of account + asset data (ADR-003, ADR-004).
pub struct AccountDetailsUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
}

impl AccountDetailsUseCase {
    /// Creates a new use case instance.
    pub fn new(account_service: Arc<AccountService>, asset_service: Arc<AssetService>) -> Self {
        Self {
            account_service,
            asset_service,
        }
    }

    /// Builds an AccountDetailsResponse for the given account (ACD-012 to ACD-050).
    pub async fn get_account_details(
        &self,
        account_id: &str,
    ) -> StdResult<AccountDetailsResponse, AccountApplicationError> {
        // ACD-032 — fetch account; bail with not-found if missing (ACD-012)
        let account = self
            .account_service
            .get_by_id(account_id)
            .await?
            .ok_or_else(|| AccountApplicationError::AccountNotFound {
                account_id: account_id.to_string(),
            })?;

        // ACD-034 — total count before quantity filter
        let all_holdings = self
            .account_service
            .get_holdings_for_account(account_id)
            .await?;
        let total_holding_count = all_holdings.len() as i64;

        // ACD-047 — total realized pnl from ALL holdings (active + closed)
        let total_realized_pnl: i64 = all_holdings.iter().map(|h| h.total_realized_pnl).sum();

        // ACD-020 — active holdings (quantity > 0); ACD-044 — closed (quantity == 0, last_sold_date set)
        let (active_holdings, closed_holdings_raw): (Vec<_>, Vec<_>) =
            all_holdings.into_iter().partition(|h| h.quantity > 0);

        // ACD-022 — enrich each active holding with asset metadata; ACD-021 — archived assets included
        // CSH-094 — accumulate the Global Value as we go: cash quantity + Σ priced non-cash holdings.
        // CSH-093 — accumulate the total cost basis from non-cash holdings only (cash has no
        // cost basis by spec; its `cost_basis` field is set to 0 below so the row stays blank).
        let mut details: Vec<HoldingDetail> = Vec::with_capacity(active_holdings.len());
        let mut total_global_value: i64 = 0;
        let mut total_cost_basis: i64 = 0;
        for holding in active_holdings {
            let asset = self
                .asset_service
                .get_asset_by_id(&holding.asset_id)
                .await
                .map_err(|e| {
                    tracing::error!(target: BACKEND, asset_id = %holding.asset_id, err = ?e, "get_account_details: get_asset_by_id failed");
                    AccountApplicationError::DatabaseError
                })?
                .ok_or_else(|| {
                    tracing::error!(target: BACKEND, asset_id = %holding.asset_id, "get_account_details: holding references missing asset");
                    AccountApplicationError::DatabaseError
                })?;

            let is_cash = asset.class == crate::context::asset::AssetClass::Cash;

            // CSH-094 — Cash Holding contributes its raw quantity (already in account currency).
            // Other holdings contribute `quantity × latest_price` only when priced and currencies
            // match (no FX conversion in v1 — mirrors the MKT-033 same-currency guard for
            // unrealized_pnl). Unpriced or foreign-currency non-cash holdings contribute 0.
            if is_cash {
                total_global_value = total_global_value.saturating_add(holding.quantity);
            }

            // CSH-093 — cash holdings carry no cost basis. Non-cash uses the standard
            // (quantity × average_price) formula with i128 intermediates (ACD-023/024).
            let cost_basis = if is_cash {
                0
            } else {
                let cb =
                    (holding.quantity as i128 * holding.average_price as i128 / 1_000_000) as i64;
                total_cost_basis = total_cost_basis.saturating_add(cb);
                cb
            };

            // MKT-031 — fetch latest price, degrade gracefully on failure
            let latest_price = self
                .asset_service
                .get_latest_price(&holding.asset_id)
                .await
                .ok()
                .flatten();

            let (current_price, current_price_date, unrealized_pnl, performance_pct) =
                if let Some(ref lp) = latest_price {
                    let cp = lp.price;
                    let cp_date = lp.date.clone();
                    // MKT-033/034 — only compute P&L when currencies match
                    let (upnl, perf) = if asset.currency == account.currency {
                        // MKT-033 — widen to i128 before subtraction to prevent i64 overflow
                        let upnl = ((cp as i128 - holding.average_price as i128)
                            * holding.quantity as i128
                            / 1_000_000) as i64;
                        let perf = if cost_basis != 0 {
                            Some((upnl as i128 * 100_000_000 / cost_basis as i128) as i64)
                        } else {
                            None
                        };
                        (Some(upnl), perf)
                    } else {
                        (None, None)
                    };
                    (Some(cp), Some(cp_date), upnl, perf)
                } else {
                    (None, None, None, None)
                };

            // CSH-094 — same-currency priced non-cash holding contributes its market value.
            if asset.class != crate::context::asset::AssetClass::Cash
                && asset.currency == account.currency
            {
                if let Some(cp) = current_price {
                    let market_value = (holding.quantity as i128 * cp as i128 / 1_000_000) as i64;
                    total_global_value = total_global_value.saturating_add(market_value);
                }
            }

            details.push(HoldingDetail {
                asset_id: holding.asset_id,
                asset_name: asset.name,
                asset_reference: asset.reference,
                quantity: holding.quantity,
                average_price: holding.average_price,
                cost_basis,
                realized_pnl: holding.total_realized_pnl,
                asset_currency: asset.currency,
                current_price,
                current_price_date,
                unrealized_pnl,
                performance_pct,
            });
        }

        // ACD-033 — sort alphabetically by asset_name ascending
        details.sort_by(|a, b| a.asset_name.cmp(&b.asset_name));

        // ACD-031 / CSH-093 — total_cost_basis already accumulated above (non-cash only).

        // MKT-040 — sum unrealized_pnl across qualifying holdings; None when none qualify
        let qualifying_pnls: Vec<i64> = details.iter().filter_map(|d| d.unrealized_pnl).collect();
        let total_unrealized_pnl = if qualifying_pnls.is_empty() {
            None
        } else {
            Some(qualifying_pnls.iter().sum())
        };

        // ACD-044/ACD-045 — enrich closed positions with asset metadata
        // Only holdings with last_sold_date set are shown (they're genuinely closed)
        let mut closed_details: Vec<ClosedHoldingDetail> =
            Vec::with_capacity(closed_holdings_raw.len());
        for holding in closed_holdings_raw {
            let Some(last_sold_date) = holding.last_sold_date else {
                continue; // ACD-045: skip qty=0 holdings without a sell date
            };
            let asset = self
                .asset_service
                .get_asset_by_id(&holding.asset_id)
                .await
                .map_err(|e| {
                    tracing::error!(target: BACKEND, asset_id = %holding.asset_id, err = ?e, "get_account_details: get_asset_by_id failed (closed)");
                    AccountApplicationError::DatabaseError
                })?
                .ok_or_else(|| {
                    tracing::error!(target: BACKEND, asset_id = %holding.asset_id, "get_account_details: closed holding references missing asset");
                    AccountApplicationError::DatabaseError
                })?;
            closed_details.push(ClosedHoldingDetail {
                asset_id: holding.asset_id,
                asset_name: asset.name,
                asset_reference: asset.reference,
                realized_pnl: holding.total_realized_pnl,
                last_sold_date,
            });
        }

        // ACD-046 — sort closed holdings by asset_name ascending
        closed_details.sort_by(|a, b| a.asset_name.cmp(&b.asset_name));

        Ok(AccountDetailsResponse {
            account_name: account.name,
            holdings: details,
            closed_holdings: closed_details,
            total_holding_count,
            total_cost_basis,
            total_realized_pnl,
            total_unrealized_pnl,
            total_global_value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::account::{
        AccountService, Holding, HoldingRepository, SqliteAccountRepository,
        SqliteHoldingRepository, SqliteTransactionRepository, UpdateFrequency,
    };
    use crate::context::asset::AssetService;
    use crate::context::asset::{
        AssetClass, CreateAssetDTO, SqliteAssetCategoryRepository, SqliteAssetRepository,
        SYSTEM_CATEGORY_ID,
    };
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup(pool: &sqlx::Pool<sqlx::Sqlite>) -> (Arc<AccountService>, Arc<AssetService>) {
        let account_svc = Arc::new(AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        ));
        let asset_svc = Arc::new(AssetService::new(
            Box::new(SqliteAssetRepository::new(pool.clone())),
            Box::new(SqliteAssetCategoryRepository::new(pool.clone())),
            Box::new(crate::context::asset::SqliteAssetPriceRepository::new(
                pool.clone(),
            )),
        ));
        (account_svc, asset_svc)
    }

    async fn make_pool() -> sqlx::Pool<sqlx::Sqlite> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("test pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    // ACD-012 — unknown account returns AccountApplicationError::AccountNotFound with id payload
    #[tokio::test]
    async fn unknown_account_returns_error() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let err = uc.get_account_details("nonexistent-id").await.unwrap_err();
        assert!(
            matches!(
                &err,
                AccountApplicationError::AccountNotFound { account_id }
                    if account_id == "nonexistent-id"
            ),
            "got: {err:?}"
        );
    }

    // ACD-020 — holdings with quantity == 0 are excluded; ACD-034 — total_holding_count counts all
    #[tokio::test]
    async fn zero_quantity_holdings_excluded_from_active() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;

        let account = account_svc
            .create(
                "Test".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "AAPL".to_string(),
                reference: "AAPL".to_string(),
                class: AssetClass::Stocks,
                currency: "USD".to_string(),
                risk_level: 3,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();

        // Insert a zero-quantity holding directly via repo
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(Holding::new(account.id.clone(), asset.id.clone(), 0, 0, 0, None).unwrap())
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();

        assert_eq!(resp.holdings.len(), 0, "active holdings should be empty");
        assert_eq!(
            resp.total_holding_count, 1,
            "total count should include zero-qty holding"
        );
        assert_eq!(resp.total_cost_basis, 0);
    }

    // ACD-023/024 — cost basis uses i128 intermediates; ACD-031 — total_cost_basis is sum
    #[tokio::test]
    async fn cost_basis_and_total_computed_correctly() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;

        let account = account_svc
            .create(
                "Portfolio".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Bond".to_string(),
                reference: "BOND".to_string(),
                class: AssetClass::Bonds,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();

        // 2.0 units at 100.00 → cost_basis = 200_000_000 micros = 200.00
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    2_000_000,   // 2.0 units
                    100_000_000, // 100.00
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();

        assert_eq!(resp.holdings.len(), 1);
        assert_eq!(resp.holdings[0].cost_basis, 200_000_000);
        assert_eq!(resp.total_cost_basis, 200_000_000);
    }

    // ACD-021 — holdings for archived assets are included (quantity > 0)
    #[tokio::test]
    async fn archived_asset_holding_included() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;

        let account = account_svc
            .create(
                "Archived Test".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Archived Stock".to_string(),
                reference: "ARCH".to_string(),
                class: AssetClass::Stocks,
                currency: "USD".to_string(),
                risk_level: 2,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();

        // Archive the asset
        asset_svc.archive_asset(&asset.id).await.unwrap();

        // Insert a positive-quantity holding for the archived asset
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    1_000_000,
                    50_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();

        assert_eq!(
            resp.holdings.len(),
            1,
            "archived asset holding should be included"
        );
        assert_eq!(resp.holdings[0].asset_reference, "ARCH");
    }

    // ACD-032 — account_name is present in the response
    #[tokio::test]
    async fn account_name_present_in_response() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "My Account".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.account_name, "My Account");
    }

    // ACD-043 — Holding entity exposes last_sold_date: Option<String> and total_realized_pnl: i64
    #[tokio::test]
    async fn holding_entity_carries_last_sold_date_and_total_realized_pnl() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "X".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "A".to_string(),
                reference: "A".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    0,
                    50_000_000,
                    15_000_000, // total_realized_pnl
                    Some("2026-01-15".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let holdings = holding_repo.get_by_account(&account.id).await.unwrap();
        assert_eq!(holdings.len(), 1);
        assert_eq!(holdings[0].total_realized_pnl, 15_000_000);
        assert_eq!(holdings[0].last_sold_date.as_deref(), Some("2026-01-15"));
    }

    // ACD-044 — closed_holdings contains holdings with quantity == 0; active holdings do not
    #[tokio::test]
    async fn closed_holdings_contains_zero_qty_holdings() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "Acct".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Closed Co".to_string(),
                reference: "CC".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    0,
                    50_000_000,
                    5_000_000,
                    Some("2025-12-01".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.holdings.len(), 0);
        assert_eq!(resp.closed_holdings.len(), 1);
        assert_eq!(resp.closed_holdings[0].asset_reference, "CC");
    }

    // ACD-044 — closed holdings are enriched with asset_name and asset_reference
    #[tokio::test]
    async fn closed_holdings_enriched_with_asset_metadata() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "Acct".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Meta Inc".to_string(),
                reference: "META".to_string(),
                class: AssetClass::Stocks,
                currency: "USD".to_string(),
                risk_level: 3,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id,
                    0,
                    0,
                    1_000_000,
                    Some("2026-03-10".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.closed_holdings[0].asset_name, "Meta Inc");
        assert_eq!(resp.closed_holdings[0].asset_reference, "META");
    }

    // ACD-045 — ClosedHoldingDetail.realized_pnl equals Holding.total_realized_pnl
    #[tokio::test]
    async fn closed_holding_detail_realized_pnl_matches_holding() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "P".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Q".to_string(),
                reference: "Q".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id,
                    0,
                    0,
                    42_000_000,
                    Some("2026-02-01".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.closed_holdings[0].realized_pnl, 42_000_000);
    }

    // ACD-045 — last_sold_date on ClosedHoldingDetail is non-optional String from Holding
    #[tokio::test]
    async fn closed_holding_detail_last_sold_date_is_non_optional() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "D".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "E".to_string(),
                reference: "E".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id,
                    0,
                    0,
                    0,
                    Some("2025-11-30".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.closed_holdings[0].last_sold_date, "2025-11-30");
    }

    // ACD-045 — holdings with last_sold_date == None are excluded from closed_holdings
    #[tokio::test]
    async fn holding_without_last_sold_date_excluded_from_closed_holdings() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "F".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "G".to_string(),
                reference: "G".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        // qty=0 but no last_sold_date — should not appear in closed_holdings
        holding_repo
            .upsert(Holding::new(account.id.clone(), asset.id, 0, 0, 0, None).unwrap())
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.closed_holdings.len(), 0);
    }

    // ACD-046 — closed_holdings sorted by asset_name ascending
    #[tokio::test]
    async fn closed_holdings_sorted_by_asset_name_ascending() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "H".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        for (name, reference) in [("Zebra", "ZBR"), ("Alpha", "ALP"), ("Mango", "MNG")] {
            let asset = asset_svc
                .create_asset(CreateAssetDTO {
                    name: name.to_string(),
                    reference: reference.to_string(),
                    class: AssetClass::Stocks,
                    currency: "EUR".to_string(),
                    risk_level: 1,
                    category_id: SYSTEM_CATEGORY_ID.to_string(),
                })
                .await
                .unwrap();
            holding_repo
                .upsert(
                    Holding::new(
                        account.id.clone(),
                        asset.id,
                        0,
                        0,
                        0,
                        Some("2026-01-01".to_string()),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
        }

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        let names: Vec<&str> = resp
            .closed_holdings
            .iter()
            .map(|h| h.asset_name.as_str())
            .collect();
        assert_eq!(names, vec!["Alpha", "Mango", "Zebra"]);
    }

    // ACD-047 — total_realized_pnl is sum across ALL holdings (active + closed)
    #[tokio::test]
    async fn total_realized_pnl_sums_active_and_closed_holdings() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "I".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());

        // Active holding with partial sells (pnl = 10)
        let asset1 = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Active".to_string(),
                reference: "ACT".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset1.id,
                    1_000_000,
                    50_000_000,
                    10_000_000,
                    Some("2025-06-01".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        // Closed holding (pnl = 25)
        let asset2 = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Closed".to_string(),
                reference: "CLO".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset2.id,
                    0,
                    0,
                    25_000_000,
                    Some("2026-01-10".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.total_realized_pnl, 35_000_000);
    }

    // ACD-047 — total_realized_pnl is 0 when no holdings have realized P&L
    #[tokio::test]
    async fn total_realized_pnl_is_zero_when_no_sells() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "J".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.total_realized_pnl, 0);
    }

    // ACD-050 — closed_holdings is empty list when no closed positions exist
    #[tokio::test]
    async fn closed_holdings_empty_when_no_closed_positions() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "K".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert!(resp.closed_holdings.is_empty());
    }

    // MKT-031 — unrealized_pnl is None on a HoldingDetail when no price has been recorded
    #[tokio::test]
    async fn holding_detail_unrealized_pnl_is_none_when_no_price() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id,
                    1_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert!(resp.holdings[0].unrealized_pnl.is_none());
        assert!(resp.holdings[0].current_price.is_none());
    }

    // MKT-033 — unrealized_pnl is computed when asset currency matches account currency
    #[tokio::test]
    async fn holding_detail_unrealized_pnl_computed_same_currency() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(), // same as account
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        // 2 units at avg_price 100.00 → cost_basis = 200.00
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    2_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        // current_price = 110.00 → unrealized_pnl = (110 - 100) * 2 = 20.00
        asset_svc
            .record_asset_price(&asset.id, "2026-01-01", 110.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.holdings[0].unrealized_pnl, Some(20_000_000));
    }

    // MKT-034 — unrealized_pnl is None when asset currency differs from account currency
    #[tokio::test]
    async fn holding_detail_unrealized_pnl_is_none_on_currency_mismatch() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "USD".to_string(), // differs from account EUR
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    1_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        asset_svc
            .record_asset_price(&asset.id, "2026-01-01", 110.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        // current_price present, but P&L is None due to mismatch
        assert!(resp.holdings[0].current_price.is_some());
        assert!(resp.holdings[0].unrealized_pnl.is_none());
        assert!(resp.holdings[0].performance_pct.is_none());
    }

    // MKT-035 — performance_pct is None when cost_basis is zero
    #[tokio::test]
    async fn holding_detail_performance_pct_is_none_when_cost_basis_zero() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        // average_price = 0 → cost_basis = 0
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(account.id.clone(), asset.id.clone(), 1_000_000, 0, 0, None).unwrap(),
            )
            .await
            .unwrap();
        asset_svc
            .record_asset_price(&asset.id, "2026-01-01", 50.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert!(resp.holdings[0].performance_pct.is_none());
    }

    // MKT-035 — performance_pct is computed correctly (5.25% = 5_250_000 micros)
    #[tokio::test]
    async fn holding_detail_performance_pct_computed_correctly() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        // 2 units at avg 100.00 → cost_basis = 200_000_000 micros
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    2_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        // current_price = 110.00 → unrealized = 20.00 → perf = 20/200 = 10% = 10_000_000 micros
        asset_svc
            .record_asset_price(&asset.id, "2026-01-01", 110.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.holdings[0].performance_pct, Some(10_000_000));
    }

    // MKT-033 — unrealized_pnl is Some(0) (not None) when current_price equals average_price
    #[tokio::test]
    async fn holding_detail_unrealized_pnl_is_zero_when_price_equals_average() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        // avg_price = 100.00, qty = 2
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    2_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        // current_price == average_price → unrealized_pnl = 0
        asset_svc
            .record_asset_price(&asset.id, "2026-01-01", 100.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.holdings[0].unrealized_pnl, Some(0));
    }

    // MKT-035 — performance_pct is Some(0) (not None) when unrealized_pnl is 0 and cost_basis nonzero
    #[tokio::test]
    async fn holding_detail_performance_pct_is_zero_when_unrealized_pnl_is_zero() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        // avg_price = 100.00, cost_basis = 100_000_000 micros
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    1_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        // current_price == average_price → unrealized_pnl = 0 → perf = 0%
        asset_svc
            .record_asset_price(&asset.id, "2026-01-01", 100.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert_eq!(resp.holdings[0].unrealized_pnl, Some(0));
        assert_eq!(resp.holdings[0].performance_pct, Some(0));
    }

    // MKT-040 — total_unrealized_pnl is None when no holding has a computable value
    #[tokio::test]
    async fn total_unrealized_pnl_is_none_when_no_qualifying_holdings() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        // No holdings at all → None
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        assert!(resp.total_unrealized_pnl.is_none());
    }

    // MKT-040 — total_unrealized_pnl sums unrealized_pnl across same-currency priced active holdings
    #[tokio::test]
    async fn total_unrealized_pnl_sums_qualifying_holdings() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        // Holding 1: EUR asset, gain 20.00
        let asset1 = asset_svc
            .create_asset(CreateAssetDTO {
                name: "A1".to_string(),
                reference: "A1".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset1.id.clone(),
                    2_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        asset_svc
            .record_asset_price(&asset1.id, "2026-01-01", 110.0)
            .await
            .unwrap();

        // Holding 2: USD asset (mismatch) — should NOT contribute
        let asset2 = asset_svc
            .create_asset(CreateAssetDTO {
                name: "A2".to_string(),
                reference: "A2".to_string(),
                class: AssetClass::Stocks,
                currency: "USD".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset2.id.clone(),
                    1_000_000,
                    50_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        asset_svc
            .record_asset_price(&asset2.id, "2026-01-01", 100.0)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();
        // Only holding 1 qualifies: unrealized_pnl = 20_000_000
        assert_eq!(resp.total_unrealized_pnl, Some(20_000_000));
    }

    // ACD-033 — holdings sorted by asset_name ascending
    #[tokio::test]
    async fn holdings_sorted_by_asset_name_ascending() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;

        let account = account_svc
            .create(
                "Alpha".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        for (name, reference) in [
            ("Zebra Fund", "ZBR"),
            ("Apple Inc", "AAPL"),
            ("Microsoft", "MSFT"),
        ] {
            let asset = asset_svc
                .create_asset(CreateAssetDTO {
                    name: name.to_string(),
                    reference: reference.to_string(),
                    class: AssetClass::Stocks,
                    currency: "USD".to_string(),
                    risk_level: 2,
                    category_id: SYSTEM_CATEGORY_ID.to_string(),
                })
                .await
                .unwrap();
            let holding_repo = SqliteHoldingRepository::new(pool.clone());
            holding_repo
                .upsert(
                    Holding::new(account.id.clone(), asset.id, 1_000_000, 50_000_000, 0, None)
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();

        let names: Vec<&str> = resp
            .holdings
            .iter()
            .map(|h| h.asset_name.as_str())
            .collect();
        assert_eq!(names, vec!["Apple Inc", "Microsoft", "Zebra Fund"]);
    }

    // CSH-093 — total_cost_basis sums non-cash holdings only.
    // CSH-094 — total_global_value = cash quantity + Σ priced same-currency non-cash holdings.
    // CSH-091 — cash row's cost_basis is 0 in the response (rendered blank in UI).
    #[tokio::test]
    async fn cash_holding_excluded_from_cost_basis_and_added_to_global_value() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;

        let account = account_svc
            .create(
                "Cash + Stocks".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        // Seed the cash asset for the account currency, then a deposit so the cash
        // holding exists with a known balance (250.00 EUR in micros).
        asset_svc.seed_cash_asset("EUR").await.unwrap();
        account_svc
            .record_deposit(&account.id, "2020-01-01".to_string(), 250_000_000, None)
            .await
            .unwrap();

        // Add a non-cash holding worth 200 EUR cost basis. No price recorded yet,
        // so it contributes 0 to the global value (CSH-094 no-fallback semantic).
        let asset = asset_svc
            .create_asset(CreateAssetDTO {
                name: "Bond".to_string(),
                reference: "BOND".to_string(),
                class: AssetClass::Bonds,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        let holding_repo = SqliteHoldingRepository::new(pool.clone());
        holding_repo
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    2_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();

        // Two active holdings: cash + bond.
        assert_eq!(resp.holdings.len(), 2);

        // CSH-091: cash row carries cost_basis = 0; bond row keeps its 200 EUR cost basis.
        let cash_row = resp
            .holdings
            .iter()
            .find(|h| h.asset_id.starts_with("system-cash-"))
            .expect("cash holding present");
        let bond_row = resp
            .holdings
            .iter()
            .find(|h| h.asset_id == asset.id)
            .expect("bond holding present");
        assert_eq!(cash_row.cost_basis, 0, "cash holding has no cost basis");
        assert_eq!(bond_row.cost_basis, 200_000_000, "bond cost basis = 200");

        // CSH-093: total_cost_basis sums non-cash only.
        assert_eq!(
            resp.total_cost_basis, 200_000_000,
            "total_cost_basis must exclude the cash holding (CSH-093)"
        );

        // CSH-094: with no recorded price for the bond, global value is cash only.
        assert_eq!(
            resp.total_global_value, 250_000_000,
            "global value = cash (250) + bond (0, unpriced) = 250"
        );
    }

    // CSH-097 — Cash row hidden at quantity = 0 (the cash holding follows
    // ACD-020's quantity > 0 filter without override). Setup: deposit then
    // withdraw the full balance so the cash holding exists at quantity 0
    // (Deposit + Withdrawal pair remains, so CSH-013 cleanup does not fire).
    #[tokio::test]
    async fn cash_holding_hidden_when_quantity_is_zero() {
        let pool = make_pool().await;
        let (account_svc, asset_svc) = setup(&pool).await;

        let account = account_svc
            .create(
                "Cash drained".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        asset_svc.seed_cash_asset("EUR").await.unwrap();
        account_svc
            .record_deposit(&account.id, "2020-01-01".to_string(), 100_000_000, None)
            .await
            .unwrap();
        account_svc
            .record_withdrawal(&account.id, "2020-02-01".to_string(), 100_000_000, None)
            .await
            .unwrap();

        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);
        let resp = uc.get_account_details(&account.id).await.unwrap();

        assert!(
            !resp
                .holdings
                .iter()
                .any(|h| h.asset_id.starts_with("system-cash-")),
            "cash row must be hidden when its quantity is 0 (CSH-097)"
        );
    }

    // -------------------------------------------------------------------------
    // Asset-side failure translation (gold rule coverage)
    // -------------------------------------------------------------------------
    //
    // Each test seeds a real holding via the Sqlite-backed account_svc, then
    // injects a failing asset_svc (mocked asset_repo) so the orchestrator hits
    // the corresponding asset-lookup branch. Active-loop and closed-loop sites
    // mirror each other but have separate coverage so both branches are exercised.

    use crate::context::asset::{
        MockAssetCategoryRepository, MockAssetPriceRepository, MockAssetRepository,
    };

    fn failing_asset_svc(ar_setup: impl FnOnce(&mut MockAssetRepository)) -> Arc<AssetService> {
        let mut ar = MockAssetRepository::new();
        ar_setup(&mut ar);
        Arc::new(AssetService::new(
            Box::new(ar),
            Box::new(MockAssetCategoryRepository::new()),
            Box::new(MockAssetPriceRepository::new()),
        ))
    }

    async fn seed_account_with_active_holding(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        account_svc: &Arc<AccountService>,
        real_asset_svc: &Arc<AssetService>,
    ) -> (String, String) {
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = real_asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    1_000_000,
                    100_000_000,
                    0,
                    None,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        (account.id, asset.id)
    }

    async fn seed_account_with_closed_holding(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        account_svc: &Arc<AccountService>,
        real_asset_svc: &Arc<AssetService>,
    ) -> (String, String) {
        let account = account_svc
            .create(
                "A".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let asset = real_asset_svc
            .create_asset(CreateAssetDTO {
                name: "X".to_string(),
                reference: "X".to_string(),
                class: AssetClass::Stocks,
                currency: "EUR".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap();
        SqliteHoldingRepository::new(pool.clone())
            .upsert(
                Holding::new(
                    account.id.clone(),
                    asset.id.clone(),
                    0,
                    50_000_000,
                    0,
                    Some("2026-01-01".to_string()),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        (account.id, asset.id)
    }

    // Active loop: asset-repo Err → DatabaseError
    #[tokio::test]
    async fn get_account_details_translates_active_loop_asset_repo_failure() {
        let pool = make_pool().await;
        let (account_svc, real_asset_svc) = setup(&pool).await;
        let (account_id, _) =
            seed_account_with_active_holding(&pool, &account_svc, &real_asset_svc).await;

        let asset_svc = failing_asset_svc(|ar| {
            ar.expect_get_by_id()
                .returning(|_| Err(anyhow::anyhow!("simulated asset repo failure")));
        });
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);

        let err = uc.get_account_details(&account_id).await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }

    // Active loop: asset-repo Ok(None) (FK integrity violation) → DatabaseError
    #[tokio::test]
    async fn get_account_details_translates_active_loop_missing_asset() {
        let pool = make_pool().await;
        let (account_svc, real_asset_svc) = setup(&pool).await;
        let (account_id, _) =
            seed_account_with_active_holding(&pool, &account_svc, &real_asset_svc).await;

        let asset_svc = failing_asset_svc(|ar| {
            ar.expect_get_by_id().returning(|_| Ok(None));
        });
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);

        let err = uc.get_account_details(&account_id).await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }

    // Closed loop: asset-repo Err → DatabaseError
    #[tokio::test]
    async fn get_account_details_translates_closed_loop_asset_repo_failure() {
        let pool = make_pool().await;
        let (account_svc, real_asset_svc) = setup(&pool).await;
        let (account_id, _) =
            seed_account_with_closed_holding(&pool, &account_svc, &real_asset_svc).await;

        let asset_svc = failing_asset_svc(|ar| {
            ar.expect_get_by_id()
                .returning(|_| Err(anyhow::anyhow!("simulated asset repo failure")));
        });
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);

        let err = uc.get_account_details(&account_id).await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }

    // Closed loop: asset-repo Ok(None) (FK integrity violation) → DatabaseError
    #[tokio::test]
    async fn get_account_details_translates_closed_loop_missing_asset() {
        let pool = make_pool().await;
        let (account_svc, real_asset_svc) = setup(&pool).await;
        let (account_id, _) =
            seed_account_with_closed_holding(&pool, &account_svc, &real_asset_svc).await;

        let asset_svc = failing_asset_svc(|ar| {
            ar.expect_get_by_id().returning(|_| Ok(None));
        });
        let uc = AccountDetailsUseCase::new(account_svc, asset_svc);

        let err = uc.get_account_details(&account_id).await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }
}
