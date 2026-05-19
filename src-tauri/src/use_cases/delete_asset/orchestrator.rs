use super::error::{DeleteAssetApplicationError, DeleteAssetError};
use crate::context::account::AccountService;
use crate::context::asset::AssetService;
use std::result::Result as StdResult;
use std::sync::Arc;

/// Guards and delegates asset hard-deletion across the asset and account bounded contexts.
/// Blocks deletion if any transaction references the asset (preserves history integrity).
pub struct DeleteAssetUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
}

impl DeleteAssetUseCase {
    /// Creates a new DeleteAssetUseCase.
    pub fn new(account_service: Arc<AccountService>, asset_service: Arc<AssetService>) -> Self {
        Self {
            account_service,
            asset_service,
        }
    }

    /// Deletes an asset, rejecting the request if any transaction references it.
    pub async fn delete_asset(&self, asset_id: &str) -> StdResult<(), DeleteAssetError> {
        if self
            .account_service
            .has_holding_entries_for_asset(asset_id)
            .await?
        {
            return Err(DeleteAssetApplicationError::ExistingTransactions.into());
        }
        self.asset_service.delete_asset(asset_id).await?;
        Ok(())
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
            exchange: None,
        }
    }

    // delete blocked when transactions exist
    #[tokio::test]
    async fn delete_rejected_when_transactions_exist() {
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

        let uc = DeleteAssetUseCase::new(account_svc, asset_svc);
        let err = uc.delete_asset(&asset.id).await.unwrap_err();
        assert!(
            matches!(
                err,
                DeleteAssetError::Application(DeleteAssetApplicationError::ExistingTransactions)
            ),
            "got: {err:?}"
        );
    }

    // delete succeeds when no transactions exist
    #[tokio::test]
    async fn delete_succeeds_when_no_transactions() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let asset = asset_svc.create_asset(base_asset_dto()).await.unwrap();

        let uc = DeleteAssetUseCase::new(account_svc, asset_svc);
        uc.delete_asset(&asset.id).await.unwrap();
    }
}
