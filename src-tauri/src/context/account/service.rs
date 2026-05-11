use super::application::{AccountApplicationError, AccountCrudError, HoldingTransactionError};
use super::domain::{
    Account, AccountOperationError, AccountRepository, Holding, HoldingRepository, Transaction,
    TransactionDomainError, TransactionRepository, UpdateFrequency,
};
use crate::core::{logger::BACKEND, Event, SideEffectEventBus};
use crate::use_cases::holding_transaction::OpenHoldingError;
use std::result::Result as StdResult;
use std::sync::Arc;
use tracing::info;

/// Orchestrates business logic for the Account bounded context.
pub struct AccountService {
    account_repo: Box<dyn AccountRepository>,
    holding_repo: Box<dyn HoldingRepository>,
    transaction_repo: Box<dyn TransactionRepository>,
    event_bus: Option<Arc<SideEffectEventBus>>,
}

impl AccountService {
    /// Creates a new AccountService.
    pub fn new(
        account_repo: Box<dyn AccountRepository>,
        holding_repo: Box<dyn HoldingRepository>,
        transaction_repo: Box<dyn TransactionRepository>,
    ) -> Self {
        Self {
            account_repo,
            holding_repo,
            transaction_repo,
            event_bus: None,
        }
    }

    /// Attaches an event bus for side-effect notifications.
    pub fn with_event_bus(mut self, bus: Arc<SideEffectEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    // -------------------------------------------------------------------------
    // Account CRUD
    // -------------------------------------------------------------------------

    /// Retrieves all non-deleted accounts.
    pub async fn get_all(&self) -> StdResult<Vec<Account>, AccountApplicationError> {
        self.account_repo.get_all().await.map_err(|e| {
            tracing::error!(target: BACKEND, err = ?e, "get_all: repository failure");
            AccountApplicationError::DatabaseError
        })
    }

    /// Retrieves an account by ID.
    pub async fn get_by_id(&self, id: &str) -> StdResult<Option<Account>, AccountApplicationError> {
        self.account_repo.get_by_id(id).await.map_err(|e| {
            tracing::error!(target: BACKEND, account_id = %id, err = ?e, "get_by_id: repository failure");
            AccountApplicationError::DatabaseError
        })
    }

    /// Creates a new account.
    pub async fn create(
        &self,
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
    ) -> Result<Account, AccountCrudError> {
        let account = Account::new(name, currency, update_frequency)?;
        if find_account_by_name(&*self.account_repo, &account.name)
            .await?
            .is_some()
        {
            return Err(AccountApplicationError::NameAlreadyExists.into());
        }
        info!(target: BACKEND, account_id = %account.id, name = %account.name, "creating account");
        let created = self.account_repo.create(account).await.map_err(|e| {
            tracing::error!(target: BACKEND, err = ?e, "create: repository failure");
            AccountApplicationError::DatabaseError
        })?;
        self.emit_account_updated();
        Ok(created)
    }

    /// Updates an existing account.
    pub async fn update(
        &self,
        id: String,
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
    ) -> Result<Account, AccountCrudError> {
        let account = Account::with_id(id, name, currency, update_frequency)?;
        if let Some(existing) = find_account_by_name(&*self.account_repo, &account.name).await? {
            if existing.id != account.id {
                return Err(AccountApplicationError::NameAlreadyExists.into());
            }
        }
        info!(target: BACKEND, account_id = %account.id, name = %account.name, "updating account");
        let updated = self.account_repo.update(account).await.map_err(|e| {
            tracing::error!(target: BACKEND, err = ?e, "update: repository failure");
            AccountApplicationError::DatabaseError
        })?;
        self.emit_account_updated();
        Ok(updated)
    }

    /// Permanently deletes an account and cascades to its holdings (R5).
    pub async fn delete(&self, id: &str) -> StdResult<(), AccountApplicationError> {
        info!(target: BACKEND, account_id = %id, "deleting account");
        self.account_repo.delete(id).await.map_err(|e| {
            tracing::error!(target: BACKEND, account_id = %id, err = ?e, "delete: repository failure");
            AccountApplicationError::DatabaseError
        })?;
        self.emit_account_updated();
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Holding reads
    // -------------------------------------------------------------------------

    /// Retrieves all holdings for a given account (ACD-022, ADR-004).
    pub async fn get_holdings_for_account(
        &self,
        account_id: &str,
    ) -> StdResult<Vec<Holding>, AccountApplicationError> {
        self.holding_repo.get_by_account(account_id).await.map_err(|e| {
            tracing::error!(target: BACKEND, account_id = %account_id, err = ?e, "get_holdings_for_account: repository failure");
            AccountApplicationError::DatabaseError
        })
    }

    /// Retrieves a single holding by account/asset pair, or None (B19).
    pub async fn get_holding_by_account_asset(
        &self,
        account_id: &str,
        asset_id: &str,
    ) -> StdResult<Option<Holding>, AccountApplicationError> {
        self.holding_repo
            .get_by_account_asset(account_id, asset_id)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, err = ?e, "get_holding_by_account_asset: repository failure");
                AccountApplicationError::DatabaseError
            })
    }

    // -------------------------------------------------------------------------
    // Transaction reads
    // -------------------------------------------------------------------------

    /// Retrieves a transaction by ID.
    pub async fn get_transaction_by_id(
        &self,
        id: &str,
    ) -> StdResult<Option<Transaction>, AccountApplicationError> {
        self.transaction_repo.get_by_id(id).await.map_err(|e| {
            tracing::error!(target: BACKEND, transaction_id = %id, err = ?e, "get_transaction_by_id: repository failure");
            AccountApplicationError::DatabaseError
        })
    }

    /// Retrieves all transactions for an account/asset pair in chronological order (TRX-036).
    pub async fn get_transactions(
        &self,
        account_id: &str,
        asset_id: &str,
    ) -> StdResult<Vec<Transaction>, AccountApplicationError> {
        self.transaction_repo
            .get_by_account_asset(account_id, asset_id)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, err = ?e, "get_transactions: repository failure");
                AccountApplicationError::DatabaseError
            })
    }

    /// Returns distinct asset IDs that have transactions for the given account (TXL-013).
    pub async fn get_asset_ids_for_account(
        &self,
        account_id: &str,
    ) -> StdResult<Vec<String>, AccountApplicationError> {
        self.transaction_repo
            .get_asset_ids_for_account(account_id)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, account_id = %account_id, err = ?e, "get_asset_ids_for_account: repository failure");
                AccountApplicationError::DatabaseError
            })
    }

    // -------------------------------------------------------------------------
    // Aggregate operations (B21 — thin orchestrators)
    // -------------------------------------------------------------------------

    /// Records a purchase of an asset into the account (TRX-020, TRX-026).
    ///
    /// Loads the Account aggregate, delegates to `Account::buy_holding`, saves
    /// atomically. Returns a typed `HoldingTransactionError` — same composite as
    /// the cash methods, since cash deposit/withdrawal and asset buy/sell are
    /// all kinds of holding transaction.
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
        info!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, "buy_holding");
        let mut account = load_account(&*self.account_repo, account_id).await?;
        let tx = account
            .buy_holding(
                asset_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )
            .map_err(to_holding_tx_error)?
            .clone();
        save_account(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Records a sale of an asset from the account (SEL-012, SEL-021, SEL-023, SEL-024).
    ///
    /// Loads the Account aggregate, delegates to `Account::sell_holding`, saves atomically.
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
        info!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, "sell_holding");
        let mut account = load_account(&*self.account_repo, account_id).await?;
        let tx = account
            .sell_holding(
                asset_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )
            .map_err(to_holding_tx_error)?
            .clone();
        save_account(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Corrects an existing transaction and recalculates the affected holding (TRX-031, SEL-031).
    ///
    /// Loads the Account aggregate, delegates to `Account::correct_transaction`, saves atomically.
    #[allow(clippy::too_many_arguments)]
    pub async fn correct_transaction(
        &self,
        account_id: &str,
        tx_id: &str,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<Transaction, HoldingTransactionError> {
        info!(target: BACKEND, account_id = %account_id, tx_id = %tx_id, "correct_transaction");
        let mut account = load_account(&*self.account_repo, account_id).await?;
        let tx = account
            .correct_transaction(tx_id, date, quantity, unit_price, exchange_rate, fees, note)
            .map_err(to_holding_tx_error)?
            .clone();
        save_account(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Deletes a transaction and recalculates (or removes) the associated holding (TRX-034).
    ///
    /// Loads the Account aggregate, delegates to `Account::cancel_transaction`, saves atomically.
    pub async fn cancel_transaction(
        &self,
        account_id: &str,
        tx_id: &str,
    ) -> Result<(), HoldingTransactionError> {
        info!(target: BACKEND, account_id = %account_id, tx_id = %tx_id, "cancel_transaction");
        let mut account = load_account(&*self.account_repo, account_id).await?;
        account
            .cancel_transaction(tx_id)
            .map_err(to_holding_tx_error)?;
        save_account(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(())
    }

    /// Seeds a holding directly from a quantity and total cost (TRX-042, TRX-047).
    ///
    /// Asset existence and archived-status checks are the caller's responsibility
    /// (handled by `HoldingTransactionUseCase::open_holding` — TRX-050, TRX-056).
    /// Returns the use-case-owned `OpenHoldingError`; the service-internal slice
    /// (load + aggregate + save) raises `Application(AccountNotFound | DatabaseError)`,
    /// `Validation(InvalidTotalCost)`, or `TxValidation(...)`. Cross-BC asset
    /// rejections never reach this method — the orchestrator raises them before
    /// delegating.
    pub async fn open_holding(
        &self,
        account_id: &str,
        asset_id: String,
        date: String,
        quantity: i64,
        total_cost: i64,
    ) -> Result<Transaction, OpenHoldingError> {
        info!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, "open_holding");
        let mut account = load_account_for_open_holding(&*self.account_repo, account_id).await?;
        let tx = account
            .open_holding(asset_id, date, quantity, total_cost)
            .map_err(to_open_holding_error)?
            .clone();
        save_account_for_open_holding(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Records a Deposit (CSH-022) — cash inflow into the account.
    ///
    /// Application-layer composition: loads the Account, builds the Transaction
    /// via `Transaction::new_deposit` (TRX-020 enforced by the factory), applies
    /// it via `Account::apply_deposit` (CSH-080 enforced by the aggregate),
    /// then saves atomically. Returns a typed `HoldingTransactionError` — no
    /// `anyhow` at this boundary; the caller (orchestrator / api) propagates
    /// the typed enum directly.
    pub async fn record_deposit(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction, HoldingTransactionError> {
        info!(target: BACKEND, account_id = %account_id, amount = amount, "record_deposit");
        let mut account = load_account(&*self.account_repo, account_id).await?;
        let tx = Transaction::new_deposit(
            account.id.clone(),
            account.cash_asset_id(),
            date,
            amount,
            note,
        )?;
        let tx = account.apply_deposit(tx)?;
        save_account(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Records a Withdrawal (CSH-032) — cash outflow from the account.
    ///
    /// Application-layer composition mirroring `record_deposit`. Raises
    /// `InsufficientCash` (CSH-080) when no Cash Holding exists or its balance
    /// is below `amount` — the check lives inside `Account::apply_withdrawal`.
    pub async fn record_withdrawal(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction, HoldingTransactionError> {
        info!(target: BACKEND, account_id = %account_id, amount = amount, "record_withdrawal");
        let mut account = load_account(&*self.account_repo, account_id).await?;
        let tx = Transaction::new_withdrawal(
            account.id.clone(),
            account.cash_asset_id(),
            date,
            amount,
            note,
        )?;
        let tx = account.apply_withdrawal(tx)?;
        save_account(&*self.account_repo, &mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    // -------------------------------------------------------------------------
    // Cross-BC guard queries (called by use cases)
    // -------------------------------------------------------------------------

    /// Returns true if any account holds a non-zero quantity of the given asset.
    /// Used by the archive_asset use case to enforce OQ-6. Translates raw
    /// infra failure into `AccountApplicationError::DatabaseError` per the
    /// gold infra-translation rule.
    pub async fn has_active_holdings_for_asset(
        &self,
        asset_id: &str,
    ) -> StdResult<bool, AccountApplicationError> {
        self.holding_repo
            .has_active_holdings_for_asset(asset_id)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, asset_id = %asset_id, err = ?e, "has_active_holdings_for_asset: repository failure");
                AccountApplicationError::DatabaseError
            })
    }

    /// Returns true if any transaction references the given asset.
    /// Used by the delete_asset use case to block hard-deletion when history
    /// exists. Translates raw infra failure into
    /// `AccountApplicationError::DatabaseError` per the gold infra-translation rule.
    pub async fn has_holding_entries_for_asset(
        &self,
        asset_id: &str,
    ) -> StdResult<bool, AccountApplicationError> {
        self.transaction_repo
            .has_transactions_for_asset(asset_id)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, asset_id = %asset_id, err = ?e, "has_holding_entries_for_asset: repository failure");
                AccountApplicationError::DatabaseError
            })
    }

    /// Returns the count of active holdings and total transactions for an account (ACC-020).
    pub async fn get_deletion_summary(
        &self,
        account_id: &str,
    ) -> StdResult<(u32, u32), AccountApplicationError> {
        tokio::try_join!(
            self.holding_repo.count_active_for_account(account_id),
            self.transaction_repo.count_by_account(account_id),
        )
        .map_err(|e| {
            tracing::error!(target: BACKEND, account_id = %account_id, err = ?e, "get_deletion_summary: repository failure");
            AccountApplicationError::DatabaseError
        })
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    fn emit_account_updated(&self) {
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AccountUpdated);
        }
    }

    fn emit_transaction_updated(&self) {
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::TransactionUpdated);
        }
    }
}

/// Loads an Account aggregate (with holdings + transactions) for the
/// holding-transaction family. Translates repository failures into typed
/// `HoldingTransactionError::Application(...)` variants — `AccountNotFound` for
/// `Ok(None)`, `DatabaseError` for any anyhow error (logged at the same site).
async fn load_account(
    repo: &dyn AccountRepository,
    account_id: &str,
) -> Result<Account, HoldingTransactionError> {
    match repo.get_with_holdings_and_transactions(account_id).await {
        Ok(Some(acc)) => Ok(acc),
        Ok(None) => Err(AccountApplicationError::AccountNotFound {
            account_id: account_id.to_string(),
        }
        .into()),
        Err(e) => {
            tracing::error!(target: BACKEND, account_id = %account_id, err = ?e, "load_account: repository failure");
            Err(AccountApplicationError::DatabaseError.into())
        }
    }
}

/// Persists an Account aggregate's pending changes for the holding-transaction
/// family. Translates repository failures into
/// `AccountApplicationError::DatabaseError` (composed into
/// `HoldingTransactionError` via `#[from]`) after logging the underlying error.
async fn save_account(
    repo: &dyn AccountRepository,
    account: &mut Account,
) -> Result<(), HoldingTransactionError> {
    repo.save(account).await.map_err(|e| {
        tracing::error!(target: BACKEND, account_id = %account.id, err = ?e, "save_account: repository failure");
        AccountApplicationError::DatabaseError.into()
    })
}

/// CRUD-family parallel to the load/save helpers above. Wraps the
/// `find_by_name` uniqueness pre-check used by `create` and `update`,
/// translating any repository failure into
/// `AccountApplicationError::DatabaseError`.
///
/// Unlike `load_account`, `Ok(None)` is the **success** path here (the name
/// is available); the caller decides what to do with `Some(existing)`.
async fn find_account_by_name(
    repo: &dyn AccountRepository,
    name: &str,
) -> Result<Option<Account>, AccountCrudError> {
    repo.find_by_name(name).await.map_err(|e| {
        tracing::error!(target: BACKEND, name = %name, err = ?e, "find_by_name: repository failure");
        AccountApplicationError::DatabaseError.into()
    })
}

/// Open-holding parallel to `load_account`. Same shape; targets `OpenHoldingError`.
async fn load_account_for_open_holding(
    repo: &dyn AccountRepository,
    account_id: &str,
) -> Result<Account, OpenHoldingError> {
    match repo.get_with_holdings_and_transactions(account_id).await {
        Ok(Some(acc)) => Ok(acc),
        Ok(None) => Err(AccountApplicationError::AccountNotFound {
            account_id: account_id.to_string(),
        }
        .into()),
        Err(e) => {
            tracing::error!(target: BACKEND, account_id = %account_id, err = ?e, "load_account_for_open_holding: repository failure");
            Err(AccountApplicationError::DatabaseError.into())
        }
    }
}

/// Open-holding parallel to `save_account`. Same shape; targets `OpenHoldingError`.
async fn save_account_for_open_holding(
    repo: &dyn AccountRepository,
    account: &mut Account,
) -> Result<(), OpenHoldingError> {
    repo.save(account).await.map_err(|e| {
        tracing::error!(target: BACKEND, account_id = %account.id, err = ?e, "save_account_for_open_holding: repository failure");
        AccountApplicationError::DatabaseError.into()
    })
}

/// Converts the `anyhow::Error` returned by the buy/sell/correct/cancel
/// aggregate methods into a typed `HoldingTransactionError` leaf. Bridge for
/// the period before those aggregate methods are themselves migrated to typed
/// Result (planned follow-up: split each into factory + apply per the cash
/// pattern). Until then, this helper preserves the typed surface at the
/// service boundary by downcasting.
///
/// Errors that don't downcast to a known leaf are logged and surfaced as
/// `AccountApplicationError::DatabaseError` — the same translation target as
/// the load/save helpers above.
fn to_holding_tx_error(e: anyhow::Error) -> HoldingTransactionError {
    let e = match e.downcast::<AccountOperationError>() {
        Ok(err) => return err.into(),
        Err(e) => e,
    };
    let e = match e.downcast::<TransactionDomainError>() {
        Ok(err) => return err.into(),
        Err(e) => e,
    };
    tracing::error!(target: BACKEND, err = ?e, "unexpected error in holding-tx service method");
    AccountApplicationError::DatabaseError.into()
}

/// Bridge for the open_holding aggregate method, which still returns
/// `anyhow::Result` and can raise `OpeningBalanceDomainError::InvalidTotalCost`
/// or any `TransactionDomainError` reachable from `Transaction::new`. Same
/// shape as `to_holding_tx_error`; targets `OpenHoldingError` instead.
fn to_open_holding_error(e: anyhow::Error) -> OpenHoldingError {
    let e = match e.downcast::<super::domain::OpeningBalanceDomainError>() {
        Ok(err) => return err.into(),
        Err(e) => e,
    };
    let e = match e.downcast::<TransactionDomainError>() {
        Ok(err) => return err.into(),
        Err(e) => e,
    };
    tracing::error!(target: BACKEND, err = ?e, "unexpected error in open_holding service method");
    AccountApplicationError::DatabaseError.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    // This module contains both SQLite-backed integration tests (real in-memory DB,
    // catch constraint violations) and mock-based unit tests (fast delegation checks).
    // SQLite tests are grouped first; mock-based unit tests follow after the section header.
    use crate::context::account::{
        AccountApplicationError, AccountOperationError, Holding, HoldingTransactionError,
        MockAccountRepository, MockHoldingRepository, MockTransactionRepository,
        SqliteAccountRepository, SqliteHoldingRepository, SqliteTransactionRepository,
        TransactionDomainError,
    };
    use sqlx::sqlite::SqlitePoolOptions;

    #[derive(Debug, thiserror::Error)]
    #[error("simulated DB failure")]
    struct SimulatedSaveError;

    // to_holding_tx_error is the anyhow→typed bridge for the four
    // holding-tx aggregate methods (buy/sell/correct/cancel) that still return
    // `anyhow::Result`. One global test covers the three branches: known
    // domain leaves route to their typed variant; everything else translates
    // to Application(DatabaseError).
    #[test]
    fn to_holding_tx_error_maps_every_branch() {
        // AccountOperationError leaf → Operation
        let op_err = AccountOperationError::Oversell {
            available: 10,
            requested: 99,
        };
        assert!(matches!(
            to_holding_tx_error(anyhow::Error::new(op_err)),
            HoldingTransactionError::Operation(AccountOperationError::Oversell {
                available: 10,
                requested: 99
            })
        ));

        // TransactionDomainError leaf → Validation
        assert!(matches!(
            to_holding_tx_error(anyhow::Error::new(TransactionDomainError::DateInFuture)),
            HoldingTransactionError::Validation(TransactionDomainError::DateInFuture)
        ));

        // Anything else → Application(DatabaseError) (the catch-all path)
        assert!(matches!(
            to_holding_tx_error(anyhow::anyhow!("synthetic infra failure")),
            HoldingTransactionError::Application(AccountApplicationError::DatabaseError)
        ));
    }

    // to_open_holding_error is the anyhow→typed bridge for `Account::open_holding`
    // (which still returns `anyhow::Result`). One global test covers the three
    // branches: known domain leaves route to their typed variants; unrecognized
    // errors translate to Application(DatabaseError).
    #[test]
    fn to_open_holding_error_maps_every_branch() {
        use crate::context::account::OpeningBalanceDomainError;
        use OpenHoldingError;

        // OpeningBalanceDomainError leaf → Validation
        assert!(matches!(
            to_open_holding_error(anyhow::Error::new(
                OpeningBalanceDomainError::InvalidTotalCost
            )),
            OpenHoldingError::Validation(OpeningBalanceDomainError::InvalidTotalCost)
        ));

        // TransactionDomainError leaf → TxValidation
        assert!(matches!(
            to_open_holding_error(anyhow::Error::new(
                TransactionDomainError::QuantityNotPositive
            )),
            OpenHoldingError::TxValidation(TransactionDomainError::QuantityNotPositive)
        ));

        // Anything else → Application(DatabaseError) (the catch-all path)
        assert!(matches!(
            to_open_holding_error(anyhow::anyhow!("synthetic infra failure")),
            OpenHoldingError::Application(AccountApplicationError::DatabaseError)
        ));
    }

    async fn setup(pool: &sqlx::Pool<sqlx::Sqlite>) -> (AccountService, String) {
        let svc = AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        );
        let asset_id = "test-asset-id".to_string();
        sqlx::query(
            "INSERT INTO assets (id, name, reference, asset_class, category_id, currency, risk_level)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&asset_id)
        .bind("TestAsset")
        .bind("TST")
        .bind("Stocks")
        .bind("default-uncategorized")
        .bind("USD")
        .bind(1_i64)
        .execute(pool)
        .await
        .expect("seed asset row");
        (svc, asset_id)
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

    async fn setup_service() -> AccountService {
        let pool = make_pool().await;
        AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        )
    }

    /// Seeds the system Cash Asset row + a large Deposit so existing buy/sell tests can
    /// satisfy CSH-041 (purchase eligibility). Bypasses `AssetService` because these tests
    /// only construct an `AccountService`.
    async fn seed_cash_for_account(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        svc: &AccountService,
        account_id: &str,
        currency: &str,
    ) {
        let cash_asset_id = format!("system-cash-{}", currency.to_lowercase());
        sqlx::query(
            "INSERT OR IGNORE INTO categories (id, name, is_deleted) VALUES ('system-cash-category', 'cash', 0)",
        )
        .execute(pool)
        .await
        .expect("seed cash category");
        sqlx::query(
            "INSERT OR IGNORE INTO assets (id, name, reference, asset_class, category_id, currency, risk_level) \
             VALUES (?, ?, ?, 'Cash', 'system-cash-category', ?, 1)",
        )
        .bind(&cash_asset_id)
        .bind(format!("Cash {}", currency.to_uppercase()))
        .bind(currency.to_uppercase())
        .bind(currency)
        .execute(pool)
        .await
        .expect("seed cash asset");
        svc.record_deposit(
            account_id,
            "2020-01-01".to_string(),
            1_000_000_000_000,
            None,
        )
        .await
        .expect("seed cash deposit");
    }

    // R3 — duplicate name (case-insensitive) is rejected at creation
    #[tokio::test]
    async fn test_create_rejects_duplicate_name_case_insensitive() {
        let svc = setup_service().await;
        svc.create(
            "Alpha".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
        )
        .await
        .unwrap();
        let err = svc
            .create(
                "alpha".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                AccountCrudError::Application(AccountApplicationError::NameAlreadyExists)
            ),
            "got: {err:?}"
        );
    }

    // R3 — updating an account to a name used by another account is rejected
    #[tokio::test]
    async fn test_update_rejects_name_collision_with_other_account() {
        let svc = setup_service().await;
        svc.create(
            "Alpha".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
        )
        .await
        .unwrap();
        let beta = svc
            .create(
                "Beta".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let err = svc
            .update(
                beta.id,
                "ALPHA".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                AccountCrudError::Application(AccountApplicationError::NameAlreadyExists)
            ),
            "got: {err:?}"
        );
    }

    // R3 — updating an account with its own name (same case) must succeed
    #[tokio::test]
    async fn test_update_allows_same_name_on_same_account() {
        let svc = setup_service().await;
        let account = svc
            .create(
                "Alpha".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        let result = svc
            .update(
                account.id,
                "Alpha".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualDay,
            )
            .await;
        assert!(result.is_ok());
    }

    fn micro(v: i64) -> i64 {
        v * 1_000_000
    }

    // TRX-026 / TRX-030 — buy_holding persists transaction and updates holding VWAP
    #[tokio::test]
    async fn test_buy_holding_persists_transaction_and_holding() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;
        let tx = svc
            .buy_holding(
                &account.id,
                asset_id.clone(),
                "2024-01-01".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap();
        assert_eq!(tx.account_id, account.id);
        assert_eq!(tx.asset_id, asset_id);
        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        let asset_holding = holdings
            .iter()
            .find(|h| h.asset_id == asset_id)
            .expect("asset holding present");
        assert_eq!(asset_holding.quantity, micro(2));
        assert_eq!(asset_holding.average_price, micro(100));
    }

    // SEL-021 — sell_holding rejects oversell via AccountOperationError
    #[tokio::test]
    async fn test_sell_holding_rejects_oversell() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;
        svc.buy_holding(
            &account.id,
            asset_id.clone(),
            "2024-01-01".to_string(),
            micro(1),
            micro(100),
            micro(1),
            0,
            None,
        )
        .await
        .unwrap();
        let err = svc
            .sell_holding(
                &account.id,
                asset_id,
                "2024-06-01".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Operation(AccountOperationError::Oversell { .. })
            ),
            "expected Oversell, got: {err:?}"
        );
    }

    // TRX-034 — cancel_transaction removes the holding when it was the last transaction
    #[tokio::test]
    async fn test_cancel_transaction_removes_holding_when_last() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;
        let tx = svc
            .buy_holding(
                &account.id,
                asset_id.clone(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap();
        svc.cancel_transaction(&account.id, &tx.id).await.unwrap();
        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        assert!(
            holdings.iter().all(|h| h.asset_id != asset_id),
            "asset holding should be removed after cancel"
        );
        let txs = svc.get_transactions(&account.id, &asset_id).await.unwrap();
        assert!(
            txs.is_empty(),
            "transactions for the asset should be removed after cancel"
        );
    }

    // SEL-026 — full sell retains holding at quantity=0 with VWAP preserved
    #[tokio::test]
    async fn test_full_sell_retains_holding_at_zero_with_last_vwap() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;
        svc.buy_holding(
            &account.id,
            asset_id.clone(),
            "2024-01-01".to_string(),
            micro(2),
            micro(100),
            micro(1),
            0,
            None,
        )
        .await
        .unwrap();
        svc.sell_holding(
            &account.id,
            asset_id.clone(),
            "2024-06-01".to_string(),
            micro(2),
            micro(120),
            micro(1),
            0,
            None,
        )
        .await
        .unwrap();
        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        let h = holdings
            .iter()
            .find(|h| h.asset_id == asset_id)
            .expect("holding should exist after full sell");
        assert_eq!(h.quantity, 0, "holding should be retained at qty=0");
        assert_eq!(h.average_price, micro(100), "VWAP should be preserved");
    }

    // SEL-032 — correcting a purchase to a lower qty that would cause a cascading oversell is rejected
    #[tokio::test]
    async fn test_correct_purchase_rejected_when_causes_cascading_oversell() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;
        let buy = svc
            .buy_holding(
                &account.id,
                asset_id.clone(),
                "2024-01-01".to_string(),
                micro(3),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap();
        svc.sell_holding(
            &account.id,
            asset_id.clone(),
            "2024-06-01".to_string(),
            micro(2),
            micro(120),
            micro(1),
            0,
            None,
        )
        .await
        .unwrap();
        let err = svc
            .correct_transaction(
                &account.id,
                &buy.id,
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Operation(AccountOperationError::CascadingOversell)
            ),
            "expected CascadingOversell, got: {err:?}"
        );
    }

    // TRX-027 — buy_holding propagates save failure so no partial state is committed
    #[tokio::test]
    async fn test_buy_holding_returns_error_when_save_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| {
                let mut acc = Account::new(
                    "Test".to_string(),
                    "EUR".to_string(),
                    UpdateFrequency::ManualMonth,
                )
                .unwrap();
                // Seed enough cash so CSH-041 doesn't short-circuit before save() is called.
                acc.record_deposit("2020-01-01".to_string(), 1_000_000_000_000, None)
                    .unwrap();
                acc.pending_changes.clear();
                Ok(Some(acc))
            });
        mock_ar
            .expect_save()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));

        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );

        let result = svc
            .buy_holding(
                "any-account-id",
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await;

        let err = result.unwrap_err();
        // The repo save error is opaqued at the service boundary — translated
        // to AccountApplicationError::DatabaseError; the hint is preserved
        // server-side via tracing::error! at the same site.
        assert!(
            matches!(
                err,
                HoldingTransactionError::Application(AccountApplicationError::DatabaseError)
            ),
            "buy_holding must surface save failures as Application(DatabaseError), got: {err:?}"
        );
    }

    // -------------------------------------------------------------------------
    // open_holding service tests (TRX-042 through TRX-056)
    // -------------------------------------------------------------------------

    // open_holding propagates save failure as Application(DatabaseError) — mirrors
    // test_buy_holding_returns_error_when_save_fails for the typed-Result path
    // (save_account_for_open_holding Err branch).
    #[tokio::test]
    async fn test_open_holding_returns_database_error_when_save_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| {
                let acc = Account::new(
                    "Test".to_string(),
                    "EUR".to_string(),
                    UpdateFrequency::ManualMonth,
                )
                .unwrap();
                Ok(Some(acc))
            });
        mock_ar
            .expect_save()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));

        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );

        let result = svc
            .open_holding(
                "any-account-id",
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .await;

        let err = result.unwrap_err();
        use crate::use_cases::holding_transaction::OpenHoldingError;
        assert!(
            matches!(
                err,
                OpenHoldingError::Application(AccountApplicationError::DatabaseError)
            ),
            "open_holding must surface save failures as Application(DatabaseError), got: {err:?}"
        );
    }

    // open_holding propagates repo load failure as Application(DatabaseError).
    // Distinct from test_open_holding_returns_account_not_found (which exercises
    // Ok(None) → Application(AccountNotFound)). This covers the Err branch of
    // load_account_for_open_holding.
    #[tokio::test]
    async fn test_open_holding_returns_database_error_when_load_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));

        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );

        let result = svc
            .open_holding(
                "any-account-id",
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .await;

        let err = result.unwrap_err();
        use crate::use_cases::holding_transaction::OpenHoldingError;
        assert!(
            matches!(
                err,
                OpenHoldingError::Application(AccountApplicationError::DatabaseError)
            ),
            "open_holding must surface load failures as Application(DatabaseError), got: {err:?}"
        );
    }

    // TRX-056 — open_holding returns AccountNotFound when account does not exist
    #[tokio::test]
    async fn test_open_holding_returns_account_not_found() {
        let svc = setup_service().await;
        let err = svc
            .open_holding(
                "nonexistent-account-id",
                "some-asset-id".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .await
            .unwrap_err();
        use OpenHoldingError;
        assert!(
            matches!(
                err,
                OpenHoldingError::Application(AccountApplicationError::AccountNotFound { .. })
            ),
            "expected Application(AccountNotFound), got: {err:?}"
        );
    }

    // TRX-044 — open_holding propagates QuantityNotPositive through the service
    #[tokio::test]
    async fn test_open_holding_propagates_quantity_not_positive() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let err = svc
            .open_holding(
                &account.id,
                asset_id,
                "2024-01-01".to_string(),
                0, // quantity ≤ 0
                micro(100),
            )
            .await
            .unwrap_err();

        use crate::context::account::TransactionDomainError;
        use OpenHoldingError;
        assert!(
            matches!(
                err,
                OpenHoldingError::TxValidation(TransactionDomainError::QuantityNotPositive)
            ),
            "expected TxValidation(QuantityNotPositive), got: {err:?}"
        );
    }

    // TRX-045 — open_holding propagates InvalidTotalCost through the service
    #[tokio::test]
    async fn test_open_holding_propagates_invalid_total_cost() {
        use crate::context::account::OpeningBalanceDomainError;
        use OpenHoldingError;
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let err = svc
            .open_holding(
                &account.id,
                asset_id,
                "2024-01-01".to_string(),
                micro(1),
                0, // total_cost ≤ 0
            )
            .await
            .unwrap_err();

        assert!(
            matches!(
                err,
                OpenHoldingError::Validation(OpeningBalanceDomainError::InvalidTotalCost)
            ),
            "expected Validation(InvalidTotalCost), got: {err:?}"
        );
    }

    // TRX-047 — open_holding persists transaction and holding with correct fields
    #[tokio::test]
    async fn test_open_holding_persists_transaction_and_holding() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        let tx = svc
            .open_holding(
                &account.id,
                asset_id.clone(),
                "2024-01-01".to_string(),
                micro(2),
                micro(200),
            )
            .await
            .unwrap();

        use crate::context::account::TransactionType;
        assert_eq!(tx.transaction_type, TransactionType::OpeningBalance);
        assert_eq!(tx.total_amount, micro(200), "total_amount = total_cost");
        assert_eq!(tx.fees, 0, "fees = 0");
        assert_eq!(tx.exchange_rate, 1_000_000, "exchange_rate = 1.0");
        // unit_price = floor(200_000_000 * 1_000_000 / 2_000_000) = 100_000_000
        assert_eq!(
            tx.unit_price,
            micro(100),
            "unit_price = total_cost / quantity"
        );

        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        assert_eq!(holdings.len(), 1);
        assert_eq!(holdings[0].quantity, micro(2));
        assert_eq!(holdings[0].average_price, micro(100));
    }

    // TRX-048 — open_holding participates in VWAP alongside Purchase
    #[tokio::test]
    async fn test_open_holding_participates_in_vwap_alongside_purchase() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;

        // OpeningBalance: 2 units, total_cost = 200
        svc.open_holding(
            &account.id,
            asset_id.clone(),
            "2024-01-01".to_string(),
            micro(2),
            micro(200),
        )
        .await
        .unwrap();

        // Purchase: 2 units @ 100 → total = 200
        svc.buy_holding(
            &account.id,
            asset_id.clone(),
            "2024-02-01".to_string(),
            micro(2),
            micro(100),
            micro(1),
            0,
            None,
        )
        .await
        .unwrap();

        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        let h = holdings
            .iter()
            .find(|h| h.asset_id == asset_id)
            .expect("holding must exist after VWAP test operations");
        // VWAP = (200 + 200) / 4 = 100
        assert_eq!(h.quantity, micro(4));
        assert_eq!(
            h.average_price,
            micro(100),
            "VWAP must include OpeningBalance totals"
        );
    }

    // TRX-049 — multiple open_holding entries for same pair are all persisted
    #[tokio::test]
    async fn test_open_holding_allows_multiple_for_same_pair() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();

        svc.open_holding(
            &account.id,
            asset_id.clone(),
            "2023-01-01".to_string(),
            micro(1),
            micro(100),
        )
        .await
        .unwrap();
        svc.open_holding(
            &account.id,
            asset_id.clone(),
            "2023-06-01".to_string(),
            micro(2),
            micro(200),
        )
        .await
        .unwrap();

        let txs = svc.get_transactions(&account.id, &asset_id).await.unwrap();
        assert_eq!(txs.len(), 2, "both opening balance rows must be persisted");

        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        assert_eq!(holdings[0].quantity, micro(3), "quantities must accumulate");
    }

    // TRX-031 — correct_transaction updates the persisted holding
    #[tokio::test]
    async fn test_correct_transaction_updates_holding() {
        let pool = make_pool().await;
        let (svc, asset_id) = setup(&pool).await;
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;
        let tx = svc
            .buy_holding(
                &account.id,
                asset_id.clone(),
                "2024-01-01".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .await
            .unwrap();
        svc.correct_transaction(
            &account.id,
            &tx.id,
            "2024-01-01".to_string(),
            micro(2),
            micro(200),
            micro(1),
            0,
            None,
        )
        .await
        .unwrap();
        let holdings = svc.get_holdings_for_account(&account.id).await.unwrap();
        let asset_holding = holdings
            .iter()
            .find(|h| h.asset_id == asset_id)
            .expect("asset holding present");
        assert_eq!(
            asset_holding.average_price,
            micro(200),
            "VWAP should update to 200"
        );
    }

    // Note: pure delegate methods — read-side (get_all, get_by_id,
    // get_holdings_for_account, get_holding_by_account_asset,
    // get_transaction_by_id, get_transactions, get_asset_ids_for_account)
    // and write-side (delete) — are exercised end-to-end against a real
    // SQLite repository in tests/account_service_crud.rs
    // (B33 — avoid trivial mock-passthrough tests).

    // CSH-100 — record_deposit and record_withdrawal publish TransactionUpdated.
    // Frontend reactivity (ACD-039, MKT-036) re-fetches on this signal.
    //
    // Pattern: do all setup first, THEN subscribe to the bus. New subscribers
    // see the latest value but `changed()` only fires on subsequent updates,
    // so this avoids racing against events emitted during setup.
    #[tokio::test]
    async fn csh_100_record_deposit_publishes_transaction_updated_event() {
        use std::time::Duration;
        let pool = make_pool().await;
        let bus = Arc::new(SideEffectEventBus::new());
        let svc = AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        )
        .with_event_bus(Arc::clone(&bus));
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;

        // Subscribe AFTER setup — `changed()` fires only on the next publish.
        let mut rx = bus.subscribe();
        svc.record_deposit(&account.id, "2020-02-01".to_string(), 50_000_000, None)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(200), rx.changed())
            .await
            .expect("TransactionUpdated event not received within 200ms")
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::TransactionUpdated);
    }

    #[tokio::test]
    async fn csh_100_record_withdrawal_publishes_transaction_updated_event() {
        use std::time::Duration;
        let pool = make_pool().await;
        let bus = Arc::new(SideEffectEventBus::new());
        let svc = AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        )
        .with_event_bus(Arc::clone(&bus));
        let account = svc
            .create(
                "Acc".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap();
        seed_cash_for_account(&pool, &svc, &account.id, "EUR").await;

        // Subscribe AFTER setup so we only observe the withdrawal's event.
        let mut rx = bus.subscribe();
        svc.record_withdrawal(&account.id, "2020-02-01".to_string(), 100_000_000, None)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(200), rx.changed())
            .await
            .expect("TransactionUpdated event not received within 200ms")
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::TransactionUpdated);
    }

    // -------------------------------------------------------------------------
    // Typed cash service error paths (B34) — mock-based unit tests for
    // record_deposit / record_withdrawal covering all four typed-Result
    // variants of HoldingTransactionError. Happy paths are covered by the SQLite
    // csh_100_* tests above.
    // -------------------------------------------------------------------------

    fn mock_cash_svc(ar: MockAccountRepository) -> AccountService {
        AccountService::new(
            Box::new(ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        )
    }

    // CSH-021 — non-positive deposit amount surfaces from `Transaction::new_deposit`
    // (the cash factory's input validation, per Rule B').
    #[tokio::test]
    async fn record_deposit_returns_amount_not_positive_on_zero() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| {
                Ok(Some(
                    Account::new(
                        "Test".to_string(),
                        "EUR".to_string(),
                        UpdateFrequency::ManualMonth,
                    )
                    .unwrap(),
                ))
            });
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_deposit("acc", "2020-01-01".to_string(), 0, None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Validation(TransactionDomainError::AmountNotPositive)
            ),
            "got: {err:?}"
        );
    }

    // load_account translates Ok(None) → AccountApplicationError::AccountNotFound.
    #[tokio::test]
    async fn record_deposit_returns_account_not_found_when_repo_returns_none() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| Ok(None));
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_deposit("missing", "2020-01-01".to_string(), 100, None)
            .await
            .unwrap_err();
        match err {
            HoldingTransactionError::Application(AccountApplicationError::AccountNotFound {
                account_id,
            }) => {
                assert_eq!(account_id, "missing");
            }
            other => panic!("expected AccountNotFound{{missing}}, got: {other:?}"),
        }
    }

    // load_account translates a repo Err → HoldingTransactionError::Application(AccountApplicationError::DatabaseError).
    #[tokio::test]
    async fn record_deposit_returns_infrastructure_when_load_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_deposit("acc", "2020-01-01".to_string(), 100, None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Application(AccountApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // save_account translates a repo Err → HoldingTransactionError::Application(AccountApplicationError::DatabaseError).
    #[tokio::test]
    async fn record_deposit_returns_infrastructure_when_save_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| {
                let acc = Account::new(
                    "Test".to_string(),
                    "EUR".to_string(),
                    UpdateFrequency::ManualMonth,
                )
                .unwrap();
                Ok(Some(acc))
            });
        mock_ar
            .expect_save()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_deposit("acc", "2020-01-01".to_string(), 100, None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Application(AccountApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // CSH-031 — non-positive withdrawal amount surfaces from
    // `Transaction::new_withdrawal` (the cash factory's input validation).
    #[tokio::test]
    async fn record_withdrawal_returns_amount_not_positive_on_zero() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| {
                Ok(Some(
                    Account::new(
                        "Test".to_string(),
                        "EUR".to_string(),
                        UpdateFrequency::ManualMonth,
                    )
                    .unwrap(),
                ))
            });
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_withdrawal("acc", "2020-01-01".to_string(), 0, None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Validation(TransactionDomainError::AmountNotPositive)
            ),
            "got: {err:?}"
        );
    }

    // load_account translates Ok(None) → AccountApplicationError::AccountNotFound.
    #[tokio::test]
    async fn record_withdrawal_returns_account_not_found_when_repo_returns_none() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| Ok(None));
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_withdrawal("missing", "2020-01-01".to_string(), 100, None)
            .await
            .unwrap_err();
        match err {
            HoldingTransactionError::Application(AccountApplicationError::AccountNotFound {
                account_id,
            }) => {
                assert_eq!(account_id, "missing");
            }
            other => panic!("expected AccountNotFound{{missing}}, got: {other:?}"),
        }
    }

    // load_account translates a repo Err → HoldingTransactionError::Application(AccountApplicationError::DatabaseError).
    #[tokio::test]
    async fn record_withdrawal_returns_infrastructure_when_load_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_withdrawal("acc", "2020-01-01".to_string(), 100, None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Application(AccountApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // save_account translates a repo Err → HoldingTransactionError::Application(AccountApplicationError::DatabaseError).
    #[tokio::test]
    async fn record_withdrawal_returns_infrastructure_when_save_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_with_holdings_and_transactions()
            .once()
            .returning(|_| {
                // Seed both a Cash Holding AND a matching Deposit Transaction
                // via `Account::restore_with_positions`. Both are required:
                // `apply_withdrawal` first checks `cash_holding_quantity()` (which
                // reads from `holdings`), then runs `replay_cash_holding()` (which
                // rebuilds the running balance from `transactions`). A holding
                // without a corresponding deposit would pass the snapshot check
                // but trip the chronological replay.
                let acc = Account::new(
                    "Test".to_string(),
                    "EUR".to_string(),
                    UpdateFrequency::ManualMonth,
                )
                .unwrap();
                // CSH-080 only fails when current cash < requested amount. Seed
                // micro(1_000) (≈ €1,000) — comfortably above the test's micro(100)
                // withdrawal. Exact value isn't load-bearing; only the inequality.
                let cash_holding = Holding::restore(
                    "h-cash".to_string(),
                    acc.id.clone(),
                    acc.cash_asset_id(),
                    micro(1_000),
                    1_000_000,
                    0,
                    None,
                );
                let seed_deposit = Transaction::new_deposit(
                    acc.id.clone(),
                    acc.cash_asset_id(),
                    "2020-01-01".to_string(),
                    micro(1_000),
                    None,
                )
                .expect("seed deposit must validate");
                Ok(Some(Account::restore_with_positions(
                    acc.id,
                    acc.name,
                    acc.currency,
                    acc.update_frequency,
                    vec![cash_holding],
                    vec![seed_deposit],
                )))
            });
        mock_ar
            .expect_save()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = mock_cash_svc(mock_ar);
        let err = svc
            .record_withdrawal("acc", "2020-02-01".to_string(), micro(100), None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                HoldingTransactionError::Application(AccountApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // -------------------------------------------------------------------------
    // Account CRUD typed-error coverage (PR 5)
    // -------------------------------------------------------------------------

    // create surfaces find_by_name repo failure as Application(DatabaseError).
    #[tokio::test]
    async fn test_create_returns_database_error_when_find_by_name_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_find_by_name()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );
        let err = svc
            .create(
                "Test".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                AccountCrudError::Application(AccountApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // create surfaces repo.create failure (after passing the uniqueness
    // pre-check) as Application(DatabaseError).
    #[tokio::test]
    async fn test_create_returns_database_error_when_repo_create_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar.expect_find_by_name().once().returning(|_| Ok(None));
        mock_ar
            .expect_create()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );
        let err = svc
            .create(
                "Test".to_string(),
                "EUR".to_string(),
                UpdateFrequency::ManualMonth,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                AccountCrudError::Application(AccountApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // delete surfaces repo failure as AccountApplicationError::DatabaseError.
    #[tokio::test]
    async fn test_delete_returns_database_error_when_repo_fails() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_delete()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );
        let err = svc.delete("any-id").await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }

    // get_by_id translates raw repo failure to AccountApplicationError::DatabaseError.
    #[tokio::test]
    async fn get_by_id_translates_repo_failure_to_database_error() {
        let mut mock_ar = MockAccountRepository::new();
        mock_ar
            .expect_get_by_id()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = AccountService::new(
            Box::new(mock_ar),
            Box::new(MockHoldingRepository::new()),
            Box::new(MockTransactionRepository::new()),
        );
        let err = svc.get_by_id("any-id").await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }

    // get_holdings_for_account translates raw repo failure to AccountApplicationError::DatabaseError.
    #[tokio::test]
    async fn get_holdings_for_account_translates_repo_failure_to_database_error() {
        let mut mock_hr = MockHoldingRepository::new();
        mock_hr
            .expect_get_by_account()
            .once()
            .returning(|_| Err(SimulatedSaveError.into()));
        let svc = AccountService::new(
            Box::new(MockAccountRepository::new()),
            Box::new(mock_hr),
            Box::new(MockTransactionRepository::new()),
        );
        let err = svc.get_holdings_for_account("any-id").await.unwrap_err();
        assert!(
            matches!(err, AccountApplicationError::DatabaseError),
            "got: {err:?}"
        );
    }
}
