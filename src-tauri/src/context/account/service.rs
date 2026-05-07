use super::domain::{
    Account, AccountDomainError, AccountRepository, Holding, HoldingRepository, Transaction,
    TransactionRepository, UpdateFrequency,
};
use crate::core::{logger::BACKEND, Event, SideEffectEventBus};
use anyhow::Result;
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
    pub async fn get_all(&self) -> Result<Vec<Account>> {
        self.account_repo.get_all().await
    }

    /// Retrieves an account by ID.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<Account>> {
        self.account_repo.get_by_id(id).await
    }

    /// Creates a new account.
    pub async fn create(
        &self,
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
    ) -> Result<Account> {
        let account = Account::new(name, currency, update_frequency)?;
        if self
            .account_repo
            .find_by_name(&account.name)
            .await?
            .is_some()
        {
            return Err(AccountDomainError::NameAlreadyExists.into());
        }
        info!(target: BACKEND, account_id = %account.id, name = %account.name, "creating account");
        let created = self.account_repo.create(account).await?;
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
    ) -> Result<Account> {
        let account = Account::with_id(id, name, currency, update_frequency)?;
        if let Some(existing) = self.account_repo.find_by_name(&account.name).await? {
            if existing.id != account.id {
                return Err(AccountDomainError::NameAlreadyExists.into());
            }
        }
        info!(target: BACKEND, account_id = %account.id, name = %account.name, "updating account");
        let updated = self.account_repo.update(account).await?;
        self.emit_account_updated();
        Ok(updated)
    }

    /// Permanently deletes an account and cascades to its holdings (R5).
    pub async fn delete(&self, id: &str) -> Result<()> {
        info!(target: BACKEND, account_id = %id, "deleting account");
        self.account_repo.delete(id).await?;
        self.emit_account_updated();
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Holding reads
    // -------------------------------------------------------------------------

    /// Retrieves all holdings for a given account (ACD-022, ADR-004).
    pub async fn get_holdings_for_account(&self, account_id: &str) -> Result<Vec<Holding>> {
        self.holding_repo.get_by_account(account_id).await
    }

    /// Retrieves a single holding by account/asset pair, or None (B19).
    pub async fn get_holding_by_account_asset(
        &self,
        account_id: &str,
        asset_id: &str,
    ) -> Result<Option<Holding>> {
        self.holding_repo
            .get_by_account_asset(account_id, asset_id)
            .await
    }

    // -------------------------------------------------------------------------
    // Transaction reads
    // -------------------------------------------------------------------------

    /// Retrieves a transaction by ID.
    pub async fn get_transaction_by_id(&self, id: &str) -> Result<Option<Transaction>> {
        self.transaction_repo.get_by_id(id).await
    }

    /// Retrieves all transactions for an account/asset pair in chronological order (TRX-036).
    pub async fn get_transactions(
        &self,
        account_id: &str,
        asset_id: &str,
    ) -> Result<Vec<Transaction>> {
        self.transaction_repo
            .get_by_account_asset(account_id, asset_id)
            .await
    }

    /// Returns distinct asset IDs that have transactions for the given account (TXL-013).
    pub async fn get_asset_ids_for_account(&self, account_id: &str) -> Result<Vec<String>> {
        self.transaction_repo
            .get_asset_ids_for_account(account_id)
            .await
    }

    // -------------------------------------------------------------------------
    // Aggregate operations (B21 — thin orchestrators)
    // -------------------------------------------------------------------------

    /// Records a purchase of an asset into the account (TRX-020, TRX-026).
    ///
    /// Loads the Account aggregate, delegates to `Account::buy_holding`, saves atomically.
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
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, "buy_holding");
        let tx = account
            .buy_holding(
                asset_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )?
            .clone();
        self.account_repo.save(&mut account).await?;
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
    ) -> Result<Transaction> {
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, "sell_holding");
        let tx = account
            .sell_holding(
                asset_id,
                date,
                quantity,
                unit_price,
                exchange_rate,
                fees,
                note,
            )?
            .clone();
        self.account_repo.save(&mut account).await?;
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
    ) -> Result<Transaction> {
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, tx_id = %tx_id, "correct_transaction");
        let tx = account
            .correct_transaction(tx_id, date, quantity, unit_price, exchange_rate, fees, note)?
            .clone();
        self.account_repo.save(&mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Deletes a transaction and recalculates (or removes) the associated holding (TRX-034).
    ///
    /// Loads the Account aggregate, delegates to `Account::cancel_transaction`, saves atomically.
    pub async fn cancel_transaction(&self, account_id: &str, tx_id: &str) -> Result<()> {
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, tx_id = %tx_id, "cancel_transaction");
        account.cancel_transaction(tx_id)?;
        self.account_repo.save(&mut account).await?;
        self.emit_transaction_updated();
        Ok(())
    }

    /// Seeds a holding directly from a quantity and total cost (TRX-042, TRX-047).
    ///
    /// Asset existence and archived-status checks are the caller's responsibility
    /// (handled by OpenHoldingUseCase — TRX-050, TRX-056).
    pub async fn open_holding(
        &self,
        account_id: &str,
        asset_id: String,
        date: String,
        quantity: i64,
        total_cost: i64,
    ) -> Result<Transaction> {
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, asset_id = %asset_id, "open_holding");
        let tx = account
            .open_holding(asset_id, date, quantity, total_cost)?
            .clone();
        self.account_repo.save(&mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Records a Deposit (CSH-022) — cash inflow into the account.
    /// Loads the Account aggregate, delegates to `Account::record_deposit`, saves atomically.
    pub async fn record_deposit(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, amount = amount, "record_deposit");
        let tx = account.record_deposit(date, amount, note)?;
        self.account_repo.save(&mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    /// Records a Withdrawal (CSH-032) — cash outflow from the account.
    /// Loads the Account aggregate, delegates to `Account::record_withdrawal`, saves atomically.
    /// Raises `InsufficientCash` (CSH-080) when no Cash Holding exists or balance < amount.
    pub async fn record_withdrawal(
        &self,
        account_id: &str,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> Result<Transaction> {
        let mut account = self
            .account_repo
            .get_with_holdings_and_transactions(account_id)
            .await?
            .ok_or_else(|| AccountDomainError::AccountNotFound(account_id.to_string()))?;
        info!(target: BACKEND, account_id = %account_id, amount = amount, "record_withdrawal");
        let tx = account.record_withdrawal(date, amount, note)?;
        self.account_repo.save(&mut account).await?;
        self.emit_transaction_updated();
        Ok(tx)
    }

    // -------------------------------------------------------------------------
    // Cross-BC guard queries (called by use cases)
    // -------------------------------------------------------------------------

    /// Returns true if any account holds a non-zero quantity of the given asset.
    /// Used by the archive_asset use case to enforce OQ-6.
    pub async fn has_active_holdings_for_asset(&self, asset_id: &str) -> Result<bool> {
        self.holding_repo
            .has_active_holdings_for_asset(asset_id)
            .await
    }

    /// Returns true if any transaction references the given asset.
    /// Used by the delete_asset use case to block hard-deletion when history exists.
    pub async fn has_holding_entries_for_asset(&self, asset_id: &str) -> Result<bool> {
        self.transaction_repo
            .has_transactions_for_asset(asset_id)
            .await
    }

    /// Returns the count of active holdings and total transactions for an account (ACC-020).
    pub async fn get_deletion_summary(&self, account_id: &str) -> Result<(u32, u32)> {
        let (holding_count, transaction_count) = tokio::try_join!(
            self.holding_repo.count_active_for_account(account_id),
            self.transaction_repo.count_by_account(account_id),
        )?;
        Ok((holding_count, transaction_count))
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

#[cfg(test)]
mod tests {
    use super::*;
    // This module contains both SQLite-backed integration tests (real in-memory DB,
    // catch constraint violations) and mock-based unit tests (fast delegation checks).
    // SQLite tests are grouped first; mock-based unit tests follow after the section header.
    use crate::context::account::{
        AccountOperationError, MockAccountRepository, MockHoldingRepository,
        MockTransactionRepository, SqliteAccountRepository, SqliteHoldingRepository,
        SqliteTransactionRepository,
    };
    use sqlx::sqlite::SqlitePoolOptions;

    #[derive(Debug, thiserror::Error)]
    #[error("simulated DB failure")]
    struct SimulatedSaveError;

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
                err.downcast_ref::<AccountDomainError>(),
                Some(AccountDomainError::NameAlreadyExists)
            ),
            "got: {err}"
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
                err.downcast_ref::<AccountDomainError>(),
                Some(AccountDomainError::NameAlreadyExists)
            ),
            "got: {err}"
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
            err.downcast_ref::<AccountOperationError>()
                .map(|e| matches!(e, AccountOperationError::Oversell { .. }))
                .unwrap_or(false),
            "expected Oversell, got: {err}"
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
            err.downcast_ref::<AccountOperationError>()
                .map(|e| matches!(e, AccountOperationError::CascadingOversell))
                .unwrap_or(false),
            "expected CascadingOversell, got: {err}"
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
        assert!(
            err.downcast_ref::<SimulatedSaveError>()
                .map(|_| true)
                .unwrap_or(false),
            "buy_holding must propagate repository save errors unchanged, got: {err}"
        );
    }

    // -------------------------------------------------------------------------
    // open_holding service tests (TRX-042 through TRX-056)
    // -------------------------------------------------------------------------

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
        assert!(
            err.downcast_ref::<AccountDomainError>()
                .map(|e| matches!(e, AccountDomainError::AccountNotFound(_)))
                .unwrap_or(false),
            "expected AccountDomainError::AccountNotFound, got: {err}"
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
        assert!(
            err.downcast_ref::<TransactionDomainError>()
                .map(|e| matches!(e, TransactionDomainError::QuantityNotPositive))
                .unwrap_or(false),
            "expected QuantityNotPositive, got: {err}"
        );
    }

    // TRX-045 — open_holding propagates InvalidTotalCost through the service
    #[tokio::test]
    async fn test_open_holding_propagates_invalid_total_cost() {
        use crate::context::account::OpeningBalanceDomainError;
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
            err.downcast_ref::<OpeningBalanceDomainError>()
                .map(|e| matches!(e, OpeningBalanceDomainError::InvalidTotalCost))
                .unwrap_or(false),
            "expected InvalidTotalCost, got: {err}"
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
}
