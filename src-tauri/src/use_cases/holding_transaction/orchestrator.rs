use super::shared::ensure_cash_asset;
use crate::context::account::{
    AccountDomainError, AccountService, OpeningBalanceDomainError, Transaction,
};
use crate::context::asset::{AssetClass, AssetService};
use anyhow::Result;
use std::sync::Arc;

/// Single orchestrator for every operation that mutates a `Holding` through a `Transaction`:
/// opening balance, buy, sell, correct, cancel.
///
/// Injects `Arc<AccountService>` + `Arc<AssetService>` and shares them across all five methods.
/// `asset_service` is used today by `open_holding` for the archived-asset guard, and will also
/// drive the cross-BC `ensure_cash_asset` step inserted by the cash-tracking spec
/// (CSH-040 / CSH-050 / CSH-042 / CSH-024).
pub struct HoldingTransactionUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
}

impl HoldingTransactionUseCase {
    /// Creates a new HoldingTransactionUseCase.
    pub fn new(account_service: Arc<AccountService>, asset_service: Arc<AssetService>) -> Self {
        Self {
            account_service,
            asset_service,
        }
    }

    /// Seeds a holding from a known quantity and total cost (TRX-042).
    ///
    /// Cross-BC guard: rejects the request if the asset does not exist or is archived
    /// (TRX-050, TRX-056). Delegates the account-side write to `AccountService::open_holding`.
    pub async fn open_holding(
        &self,
        account_id: &str,
        asset_id: String,
        date: String,
        quantity: i64,
        total_cost: i64,
    ) -> Result<Transaction> {
        match self.asset_service.get_asset_by_id(&asset_id).await? {
            None => return Err(OpeningBalanceDomainError::AssetNotFound.into()),
            Some(a) if a.is_archived => return Err(OpeningBalanceDomainError::ArchivedAsset.into()),
            // CSH-061 — Cash Assets cannot be seeded via OpeningBalance; user records
            // initial cash via `record_deposit` instead.
            Some(a) if a.class == AssetClass::Cash => {
                return Err(OpeningBalanceDomainError::OpeningBalanceOnCashAsset.into())
            }
            Some(_) => {}
        }
        self.account_service
            .open_holding(account_id, asset_id, date, quantity, total_cost)
            .await
    }

    /// Records a purchase of an asset into an account (TRX-027).
    /// Seeds the system Cash Asset for the account's currency (CSH-010) before delegating;
    /// the aggregate replays the cash holding inside `Account::buy_holding` (CSH-040 / CSH-041).
    #[allow(clippy::too_many_arguments)]
    pub async fn buy_holding(
        &self,
        account_id: &str,
        asset_id: String,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        self.ensure_cash_for(account_id).await?;
        self.account_service
            .buy_holding(
                account_id,
                asset_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )
            .await
    }

    /// Records a sale of an asset from an account (SEL-012, SEL-021, SEL-023, SEL-024).
    /// Seeds the system Cash Asset (CSH-010); the aggregate lazy-creates the Cash Holding
    /// when this is the first cash-affecting transaction (CSH-050 / CSH-012).
    #[allow(clippy::too_many_arguments)]
    pub async fn sell_holding(
        &self,
        account_id: &str,
        asset_id: String,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        self.ensure_cash_for(account_id).await?;
        self.account_service
            .sell_holding(
                account_id,
                asset_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )
            .await
    }

    /// Corrects an existing transaction and recalculates the affected holding (TRX-031).
    /// Seeds the system Cash Asset; the aggregate replay re-evaluates the cash holding for
    /// any cash-affecting tx (CSH-042 / CSH-051) and may raise InsufficientCash.
    #[allow(clippy::too_many_arguments)]
    pub async fn correct_transaction(
        &self,
        account_id: &str,
        transaction_id: &str,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        self.ensure_cash_for(account_id).await?;
        self.account_service
            .correct_transaction(
                account_id,
                transaction_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )
            .await
    }

    /// Cancels a transaction and recalculates (or removes) the associated holding (TRX-034).
    /// The aggregate replay catches any chronologically-later violation (CSH-024 / CSH-051).
    pub async fn cancel_transaction(&self, account_id: &str, transaction_id: &str) -> Result<()> {
        self.ensure_cash_for(account_id).await?;
        self.account_service
            .cancel_transaction(account_id, transaction_id)
            .await
    }

    /// Records a Deposit into an account (CSH-022).
    /// Seeds the system Cash Asset (CSH-010) before delegating; the aggregate
    /// lazy-creates the Cash Holding (CSH-012) and persists the Transaction.
    pub async fn record_deposit(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        self.ensure_cash_for(account_id).await?;
        self.account_service
            .record_deposit(account_id, date, amount, note)
            .await
    }

    /// Records a Withdrawal from an account (CSH-032).
    /// Raises InsufficientCash (CSH-080) when no Cash Holding exists or balance < amount.
    pub async fn record_withdrawal(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        self.ensure_cash_for(account_id).await?;
        self.account_service
            .record_withdrawal(account_id, date, amount, note)
            .await
    }

    /// Loads the account's currency and ensures the system Cash Asset for that currency
    /// exists (CSH-010, CSH-011, CSH-017). Idempotent: safe to call on every cash-affecting
    /// command.
    async fn ensure_cash_for(&self, account_id: &str) -> Result<()> {
        let account = self
            .account_service
            .get_by_id(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        ensure_cash_asset(&self.asset_service, &account.currency).await
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

    fn micro(v: i64) -> i64 {
        v * 1_000_000
    }

    // TRX-056 — AssetNotFound when asset does not exist
    #[tokio::test]
    async fn open_holding_rejects_unknown_asset() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let account = account_svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let uc = HoldingTransactionUseCase::new(account_svc, asset_svc);
        let err = uc
            .open_holding(
                &account.id,
                "nonexistent-asset".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .await
            .unwrap_err();

        assert!(
            err.downcast_ref::<OpeningBalanceDomainError>()
                .map(|e| matches!(e, OpeningBalanceDomainError::AssetNotFound))
                .unwrap_or(false),
            "expected AssetNotFound, got: {err}"
        );
    }

    // TRX-050 — ArchivedAsset when asset is archived
    #[tokio::test]
    async fn open_holding_rejects_archived_asset() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let asset = asset_svc.create_asset(base_asset_dto()).await.unwrap();
        asset_svc.archive_asset(&asset.id).await.unwrap();
        let account = account_svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let uc = HoldingTransactionUseCase::new(account_svc, asset_svc);
        let err = uc
            .open_holding(
                &account.id,
                asset.id,
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .await
            .unwrap_err();

        assert!(
            err.downcast_ref::<OpeningBalanceDomainError>()
                .map(|e| matches!(e, OpeningBalanceDomainError::ArchivedAsset))
                .unwrap_or(false),
            "expected ArchivedAsset, got: {err}"
        );
    }

    // CSH-061 — open_holding rejects an OpeningBalance against a Cash Asset
    // (user must record initial cash via record_deposit instead).
    #[tokio::test]
    async fn open_holding_rejects_cash_asset() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let cash_asset = asset_svc.seed_cash_asset("EUR").await.unwrap();
        let account = account_svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let uc = HoldingTransactionUseCase::new(account_svc, asset_svc);
        let err = uc
            .open_holding(
                &account.id,
                cash_asset.id,
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .await
            .unwrap_err();

        assert!(
            err.downcast_ref::<OpeningBalanceDomainError>()
                .map(|e| matches!(e, OpeningBalanceDomainError::OpeningBalanceOnCashAsset))
                .unwrap_or(false),
            "expected OpeningBalanceOnCashAsset, got: {err}"
        );
    }

    // TRX-047 — happy path: transaction and holding created with correct fields
    #[tokio::test]
    async fn open_holding_happy_path() {
        use crate::context::account::TransactionType;

        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let asset = asset_svc.create_asset(base_asset_dto()).await.unwrap();
        let account = account_svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let uc = HoldingTransactionUseCase::new(Arc::clone(&account_svc), asset_svc);
        let tx = uc
            .open_holding(
                &account.id,
                asset.id.clone(),
                "2024-01-01".to_string(),
                micro(2),
                micro(200),
            )
            .await
            .unwrap();

        assert_eq!(tx.transaction_type, TransactionType::OpeningBalance);
        assert_eq!(tx.total_amount, micro(200));
        assert_eq!(tx.fees, 0);
        assert_eq!(tx.exchange_rate, 1_000_000);
        assert_eq!(tx.unit_price, micro(100));

        let holdings = account_svc
            .get_holdings_for_account(&account.id)
            .await
            .unwrap();
        assert_eq!(holdings.len(), 1);
        assert_eq!(holdings[0].quantity, micro(2));
        assert_eq!(holdings[0].average_price, micro(100));
    }
}
