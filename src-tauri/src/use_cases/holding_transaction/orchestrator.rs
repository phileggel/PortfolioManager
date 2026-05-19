use super::error::{OpenHoldingApplicationError, OpenHoldingError};
use super::shared::ensure_cash_asset;
use crate::context::account::{
    AccountApplicationError, AccountService, HoldingTransactionError, Transaction,
};
use crate::context::asset::{AssetClass, AssetService};
use crate::core::logger::BACKEND;
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
    /// Cross-BC guard: rejects the request if the asset does not exist
    /// (TRX-056), is archived (TRX-050), or is a system Cash Asset (CSH-061).
    /// Delegates the account-side write to `AccountService::open_holding`.
    /// Returns the typed `OpenHoldingError` composite. Asset-side repo failures
    /// from `get_asset_by_id` are translated to `AccountApplicationError::DatabaseError`
    /// (matching the `ensure_cash_for` precedent) so the FE wire surface carries a
    /// single `{ code: "DatabaseError" }` shape rather than two indistinguishable arms.
    pub async fn open_holding(
        &self,
        account_id: &str,
        asset_id: String,
        date: String,
        quantity: i64,
        total_cost: i64,
    ) -> Result<Transaction, OpenHoldingError> {
        let asset = self
            .asset_service
            .get_asset_by_id(&asset_id)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, err = ?e, "open_holding: get_asset_by_id failed");
                AccountApplicationError::DatabaseError
            })?;
        match asset {
            None => return Err(OpenHoldingApplicationError::AssetNotFound.into()),
            Some(a) if a.is_archived => {
                return Err(OpenHoldingApplicationError::ArchivedAsset.into())
            }
            // CSH-061 — Cash Assets cannot be seeded via OpeningBalance; user records
            // initial cash via `record_deposit` instead.
            Some(a) if a.class == AssetClass::Cash => {
                return Err(OpenHoldingApplicationError::OpeningBalanceOnCashAsset.into())
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
    ) -> Result<Transaction, HoldingTransactionError> {
        self.ensure_cash_for(account_id, "buy_holding").await?;
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
    ) -> Result<Transaction, HoldingTransactionError> {
        self.ensure_cash_for(account_id, "sell_holding").await?;
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
    ) -> Result<Transaction, HoldingTransactionError> {
        self.ensure_cash_for(account_id, "correct_transaction")
            .await?;
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
    pub async fn cancel_transaction(
        &self,
        account_id: &str,
        transaction_id: &str,
    ) -> Result<(), HoldingTransactionError> {
        self.ensure_cash_for(account_id, "cancel_transaction")
            .await?;
        self.account_service
            .cancel_transaction(account_id, transaction_id)
            .await
    }

    /// Records a Deposit into an account (CSH-022).
    /// Seeds the system Cash Asset (CSH-010) before delegating; the aggregate
    /// lazy-creates the Cash Holding (CSH-012) and persists the Transaction.
    /// Returns a typed `HoldingTransactionError`: in-account and cross-BC
    /// asset-seed failures both surface through `Application(AccountApplicationError)`
    /// (see `ensure_cash_for`).
    pub async fn record_deposit(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction, HoldingTransactionError> {
        self.ensure_cash_for(account_id, "record_deposit").await?;
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
    ) -> Result<Transaction, HoldingTransactionError> {
        self.ensure_cash_for(account_id, "record_withdrawal")
            .await?;
        self.account_service
            .record_withdrawal(account_id, date, amount, note)
            .await
    }

    /// Loads the account, then ensures the system Cash Asset for its currency
    /// exists (CSH-010, CSH-011, CSH-017). Idempotent: safe to call on every
    /// cash-affecting command. Returns a typed `HoldingTransactionError` so
    /// callers can propagate via `?` and stay typed end-to-end.
    ///
    /// Both error sources flow through `HoldingTransactionError::Application(...)`:
    /// - **In-account failures**: `AccountNotFound { account_id }` when the row
    ///   is missing, `DatabaseError` when the account-repo call fails.
    /// - **Cross-BC asset-side failure** (`ensure_cash_asset` failure):
    ///   surfaced as `DatabaseError` after `tracing::error!` preserves the
    ///   asset-side diagnostic chain server-side.
    async fn ensure_cash_for(
        &self,
        account_id: &str,
        op: &str,
    ) -> Result<(), HoldingTransactionError> {
        let account = self
            .account_service
            .get_by_id(account_id)
            .await?
            .ok_or_else(|| AccountApplicationError::AccountNotFound {
                account_id: account_id.to_string(),
            })?;
        ensure_cash_asset(&self.asset_service, &account.currency)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, account_id = %account_id, op = %op, err = ?e, "ensure_cash_for: ensure_cash_asset failed");
                AccountApplicationError::DatabaseError.into()
            })
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
            matches!(
                err,
                OpenHoldingError::UseCase(OpenHoldingApplicationError::AssetNotFound)
            ),
            "expected UseCase(AssetNotFound), got: {err:?}"
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
            matches!(
                err,
                OpenHoldingError::UseCase(OpenHoldingApplicationError::ArchivedAsset)
            ),
            "expected UseCase(ArchivedAsset), got: {err:?}"
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
            matches!(
                err,
                OpenHoldingError::UseCase(OpenHoldingApplicationError::OpeningBalanceOnCashAsset)
            ),
            "expected UseCase(OpeningBalanceOnCashAsset), got: {err:?}"
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

    // -------------------------------------------------------------------------
    // Holding-tx orchestrator coverage (PR 3 — typed Result delegation)
    // -------------------------------------------------------------------------

    // TRX-027 — buy_holding happy path through the orchestrator: typed Result
    // flows from AccountService through the orchestrator unchanged.
    #[tokio::test]
    async fn buy_holding_orchestrator_happy_path() {
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
        // Seed cash through the orchestrator so ensure_cash_for has been exercised
        // before the buy.
        uc.record_deposit(&account.id, "2024-01-01".to_string(), micro(10_000), None)
            .await
            .unwrap();

        let tx = uc
            .buy_holding(
                &account.id,
                asset.id.clone(),
                "2024-01-15".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap();

        assert_eq!(tx.transaction_type, TransactionType::Purchase);
        assert_eq!(tx.total_amount, micro(200));
    }

    // When `buy_holding` is called for a nonexistent account, the
    // orchestrator's `ensure_cash_for` surfaces it as the typed
    // `Application(AccountNotFound { account_id })` — same shape every other
    // path raises for the same condition.
    #[tokio::test]
    async fn buy_holding_orchestrator_unknown_account_returns_application() {
        let pool = setup_pool().await;
        let (account_svc, asset_svc) = make_services(&pool);
        let uc = HoldingTransactionUseCase::new(account_svc, asset_svc);

        let err = uc
            .buy_holding(
                "nonexistent-account-id",
                "irrelevant-asset".to_string(),
                "2024-01-15".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap_err();

        match err {
            HoldingTransactionError::Application(AccountApplicationError::AccountNotFound {
                account_id,
            }) => {
                assert_eq!(account_id, "nonexistent-account-id");
            }
            other => panic!("expected Application(AccountNotFound), got: {other:?}"),
        }
    }

    // CSH-022 — record_deposit through the orchestrator: typed Result is
    // returned end-to-end (no anyhow at this boundary).
    #[tokio::test]
    async fn record_deposit_orchestrator_happy_path() {
        use crate::context::account::TransactionType;

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

        let tx = uc
            .record_deposit(&account.id, "2024-01-01".to_string(), micro(500), None)
            .await
            .unwrap();

        assert_eq!(tx.transaction_type, TransactionType::Deposit);
        assert_eq!(tx.total_amount, micro(500));
    }
}
