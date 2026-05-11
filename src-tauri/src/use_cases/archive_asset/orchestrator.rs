use crate::context::account::AccountService;
use crate::context::asset::AssetService;
use anyhow::Result;
use std::sync::Arc;

/// Typed error for the ArchiveAssetUseCase.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveAssetError {
    /// Asset still has non-zero holdings in at least one account (OQ-6).
    #[error("Cannot archive an asset with active holdings")]
    ActiveHoldings,
}

/// Guards and delegates asset archiving across the asset and account bounded contexts (OQ-6).
pub struct ArchiveAssetUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
}

impl ArchiveAssetUseCase {
    /// Creates a new ArchiveAssetUseCase.
    pub fn new(account_service: Arc<AccountService>, asset_service: Arc<AssetService>) -> Self {
        Self {
            account_service,
            asset_service,
        }
    }

    /// Archives an asset, rejecting the request if any account holds an active position (OQ-6).
    pub async fn archive_asset(&self, asset_id: &str) -> Result<()> {
        if self
            .account_service
            .has_active_holdings_for_asset(asset_id)
            .await?
        {
            return Err(ArchiveAssetError::ActiveHoldings.into());
        }
        self.asset_service
            .archive_asset(asset_id)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::account::{
        AccountService, SqliteAccountRepository, SqliteHoldingRepository,
        SqliteTransactionRepository, UpdateFrequency,
    };
    use crate::context::asset::{
        AssetClass, AssetService, CreateAssetDTO, SqliteAssetCategoryRepository,
        SqliteAssetPriceRepository, SqliteAssetRepository, SYSTEM_CATEGORY_ID,
    };
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> sqlx::Pool<sqlx::Sqlite> {
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

    fn make_services(pool: &sqlx::Pool<sqlx::Sqlite>) -> (Arc<AccountService>, Arc<AssetService>) {
        let account_svc = Arc::new(AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        ));
        let asset_svc = Arc::new(AssetService::new(
            Box::new(SqliteAssetRepository::new(pool.clone())),
            Box::new(SqliteAssetCategoryRepository::new(pool.clone())),
            Box::new(SqliteAssetPriceRepository::new(pool.clone())),
        ));
        (account_svc, asset_svc)
    }

    fn base_asset_dto() -> CreateAssetDTO {
        CreateAssetDTO {
            name: "Test Asset".to_string(),
            reference: "TST".to_string(),
            class: AssetClass::Stocks,
            currency: "USD".to_string(),
            risk_level: 1,
            category_id: SYSTEM_CATEGORY_ID.to_string(),
        }
    }

    // OQ-6 — archive rejected when active holding exists
    #[tokio::test]
    async fn archive_rejected_when_active_holdings() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);

        let asset = asset_svc.create_asset(base_asset_dto()).await.unwrap();
        let account = account_svc
            .create(
                "Test Account".to_string(),
                "USD".to_string(),
                UpdateFrequency::ManualDay,
            )
            .await
            .unwrap();
        // Cash is a Holding (CSH-090): seed cash before any purchase so CSH-041 holds.
        asset_svc.seed_cash_asset("USD").await.unwrap();
        account_svc
            .record_deposit(
                &account.id,
                "2020-01-01".to_string(),
                1_000_000_000_000,
                None,
            )
            .await
            .unwrap();
        account_svc
            .buy_holding(
                &account.id,
                asset.id.clone(),
                "2026-01-01".to_string(),
                1_000_000,
                10_000_000,
                1_000_000,
                0,
                None,
            )
            .await
            .unwrap();

        let uc = ArchiveAssetUseCase::new(account_svc, asset_svc);
        let err = uc.archive_asset(&asset.id).await.unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<ArchiveAssetError>(),
                Some(ArchiveAssetError::ActiveHoldings)
            ),
            "got: {err}"
        );
    }

    // OQ-6 — archive succeeds when no active holdings exist
    #[tokio::test]
    async fn archive_succeeds_when_no_active_holdings() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let asset = asset_svc.create_asset(base_asset_dto()).await.unwrap();

        let uc = ArchiveAssetUseCase::new(account_svc, asset_svc);
        uc.archive_asset(&asset.id).await.unwrap();
    }
}
