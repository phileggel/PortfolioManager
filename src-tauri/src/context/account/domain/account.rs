use super::error::{AccountDomainError, AccountOperationError, OpeningBalanceDomainError};
use super::holding::Holding;
use super::transaction::{Transaction, TransactionType};
use super::transaction_error::TransactionDomainError;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use iso_currency::Currency;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::result::Result as StdResult;
use std::str::FromStr;
use uuid::Uuid;

/// Defines how often an account's data should be updated.
#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Type,
    PartialEq,
    Eq,
    Default,
    strum_macros::Display,
    strum_macros::EnumString,
)]
pub enum UpdateFrequency {
    /// Automatic updates (e.g. via API)
    Automatic,
    /// Manual update daily
    ManualDay,
    /// Manual update weekly
    ManualWeek,
    /// Manual update monthly
    #[default]
    ManualMonth,
    /// Manual update yearly
    ManualYear,
}

/// A single change produced by an aggregate operation, applied atomically by the repository.
#[derive(Debug, Clone)]
pub enum AccountChange {
    /// A new transaction was created.
    TransactionInserted(Transaction),
    /// An existing transaction's fields were updated.
    TransactionUpdated(Transaction),
    /// A transaction was permanently removed.
    TransactionDeleted(String),
    /// A holding was created or updated (upsert).
    HoldingUpserted(Holding),
    /// A holding was removed (no transactions remain for the pair).
    HoldingDeleted {
        /// Account the holding belonged to.
        account_id: String,
        /// Asset the holding represented.
        asset_id: String,
    },
}

/// Represents a financial account — the Aggregate Root of the Account bounded context.
/// Owns all holdings and transactions for this account.
///
/// The `holdings`, `transactions`, and `pending_changes` fields are populated only
/// when the aggregate is loaded for mutation via `AccountRepository::get_with_holdings_and_transactions`.
/// They are excluded from Tauri serialization and TypeScript bindings.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Account {
    /// Unique identifier (uuid).
    pub id: String,
    /// User defined name.
    pub name: String,
    /// ISO 4217 currency code for this account (TRX-021).
    pub currency: String,
    /// How often this account is updated.
    pub update_frequency: UpdateFrequency,
    /// Holdings owned by this account. Populated only in aggregate load — excluded from bindings.
    #[serde(skip)]
    #[specta(skip)]
    pub holdings: Vec<Holding>,
    /// Transactions owned by this account. Populated only in aggregate load — excluded from bindings.
    #[serde(skip)]
    #[specta(skip)]
    pub transactions: Vec<Transaction>,
    /// Pending changes to persist atomically. Drained by `AccountRepository::save` on success.
    #[serde(skip)]
    #[specta(skip)]
    pub(crate) pending_changes: Vec<AccountChange>,
}

impl Account {
    /// Creates a new Account. Trims the name before validation and storage (R1).
    pub fn new(
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
    ) -> StdResult<Self, AccountDomainError> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AccountDomainError::NameEmpty);
        }
        Self::validate_currency(&currency)?;
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name,
            currency,
            update_frequency,
            holdings: Vec::new(),
            transactions: Vec::new(),
            pending_changes: Vec::new(),
        })
    }

    /// Updates an existing Account. Trims and validates identically to new() (R1, R2).
    pub fn with_id(
        id: String,
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
    ) -> StdResult<Self, AccountDomainError> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AccountDomainError::NameEmpty);
        }
        Self::validate_currency(&currency)?;
        Ok(Self {
            id,
            name,
            currency,
            update_frequency,
            holdings: Vec::new(),
            transactions: Vec::new(),
            pending_changes: Vec::new(),
        })
    }

    /// Reconstructs a thin Account from storage without validation (CRUD load — no aggregate data).
    pub fn restore(
        id: String,
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
    ) -> Self {
        Self {
            id,
            name,
            currency,
            update_frequency,
            holdings: Vec::new(),
            transactions: Vec::new(),
            pending_changes: Vec::new(),
        }
    }

    /// Reconstructs an Account with its full aggregate state from storage.
    /// Used exclusively by `AccountRepository::get_with_holdings_and_transactions`.
    pub fn restore_with_positions(
        id: String,
        name: String,
        currency: String,
        update_frequency: UpdateFrequency,
        holdings: Vec<Holding>,
        transactions: Vec<Transaction>,
    ) -> Self {
        Self {
            id,
            name,
            currency,
            update_frequency,
            holdings,
            transactions,
            pending_changes: Vec::new(),
        }
    }

    /// Returns the pending changes accumulated by aggregate operations since last save.
    pub fn pending_changes(&self) -> &[AccountChange] {
        &self.pending_changes
    }

    // -------------------------------------------------------------------------
    // Aggregate Root methods (B28 — domain/business vocabulary)
    // -------------------------------------------------------------------------

    /// Records a purchase of an asset into this account (TRX-020, TRX-026).
    ///
    /// Creates a Transaction internally, then upserts the Holding with the updated
    /// VWAP and quantity. Enqueues the changes for atomic persistence.
    #[allow(clippy::too_many_arguments)]
    pub fn buy_holding(
        &mut self,
        asset_id: String,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<&Transaction> {
        let total_amount = Self::compute_purchase_total(quantity, unit_price, exchange_rate, fees);
        let tx = Transaction::new(
            self.id.clone(),
            asset_id.clone(),
            TransactionType::Purchase,
            date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            note,
            None,
        )?;
        self.transactions.push(tx);
        let tx_ref = self
            .transactions
            .last()
            .ok_or_else(|| anyhow!("BUG: tx list empty after push in account {}", self.id))?;

        let pair_txs: Vec<&Transaction> = self
            .transactions
            .iter()
            .filter(|t| t.asset_id == asset_id)
            .collect();
        let (holding, _) = self.recalculate_holding(&asset_id, &pair_txs)?;

        self.pending_changes
            .push(AccountChange::TransactionInserted(tx_ref.clone()));
        self.pending_changes
            .push(AccountChange::HoldingUpserted(holding.clone()));
        self.upsert_holding_in_memory(holding);

        // CSH-040 — Purchase debits cash. CSH-041 raises InsufficientCash here when needed.
        self.replay_cash_holding()?;

        self.transactions
            .last()
            .ok_or_else(|| anyhow!("BUG: tx list empty after push in account {}", self.id))
    }

    /// Records a sale of an asset from this account (SEL-012, SEL-021, SEL-023, SEL-024).
    ///
    /// Validates the position is open and the quantity is available, creates a Transaction,
    /// updates the Holding with the recalculated VWAP and realized P&L.
    #[allow(clippy::too_many_arguments)]
    pub fn sell_holding(
        &mut self,
        asset_id: String,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<&Transaction> {
        // SEL-012 — closed position guard
        let current_qty = self.holding_quantity(&asset_id);
        if current_qty == 0 {
            return Err(AccountOperationError::ClosedPosition.into());
        }
        // SEL-021 — oversell guard
        if quantity > current_qty {
            return Err(AccountOperationError::Oversell {
                available: current_qty,
                requested: quantity,
            }
            .into());
        }

        let total_amount = Self::compute_sell_total(quantity, unit_price, exchange_rate, fees);
        let tx = Transaction::new(
            self.id.clone(),
            asset_id.clone(),
            TransactionType::Sell,
            date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            note,
            None, // realized_pnl computed below
        )?;
        self.transactions.push(tx);

        let pair_txs: Vec<&Transaction> = self
            .transactions
            .iter()
            .filter(|t| t.asset_id == asset_id)
            .collect();
        let (holding, pnl_map) = self.recalculate_holding(&asset_id, &pair_txs)?;

        // Attach computed realized_pnl to the new sell transaction
        let tx_ref = self
            .transactions
            .last_mut()
            .ok_or_else(|| anyhow!("BUG: tx list empty after push in account {}", self.id))?;
        let realized_pnl = pnl_map.get(&tx_ref.id).copied();
        tx_ref.realized_pnl = realized_pnl;
        let tx_snapshot = tx_ref.clone();

        self.pending_changes
            .push(AccountChange::TransactionInserted(tx_snapshot));
        self.pending_changes
            .push(AccountChange::HoldingUpserted(holding.clone()));
        self.upsert_holding_in_memory(holding);

        // CSH-050 — Sell credits cash; lazy-creates the Cash Holding when this is the first
        // cash-affecting transaction (CSH-012). Sell never raises InsufficientCash.
        self.replay_cash_holding()?;

        self.transactions
            .last()
            .ok_or_else(|| anyhow!("BUG: tx list empty after push in account {}", self.id))
    }

    /// Corrects the fields of an existing transaction and recalculates the affected holding
    /// (TRX-031, SEL-031, SEL-032).
    ///
    /// The transaction type is immutable — `correct_transaction` preserves it.
    /// Performs a cascading oversell check after recalculation.
    #[allow(clippy::too_many_arguments)]
    pub fn correct_transaction(
        &mut self,
        tx_id: &str,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        note: Option<String>,
    ) -> Result<&Transaction> {
        let existing = self
            .transactions
            .iter()
            .find(|t| t.id == tx_id)
            .ok_or(AccountOperationError::TransactionNotFound)?;

        let tx_type = existing.transaction_type;
        let asset_id = existing.asset_id.clone();
        let created_at = existing.created_at.clone();

        let total_amount = match tx_type {
            TransactionType::Purchase => {
                Self::compute_purchase_total(quantity, unit_price, exchange_rate, fees)
            }
            TransactionType::Sell => {
                Self::compute_sell_total(quantity, unit_price, exchange_rate, fees)
            }
            TransactionType::OpeningBalance => {
                Self::compute_opening_balance_total(quantity, unit_price)
            }
            // CSH-022 / CSH-032: cash transactions carry total_amount == quantity (no fees, no FX).
            TransactionType::Deposit | TransactionType::Withdrawal => quantity,
        };

        let updated_tx = Transaction::with_id(
            tx_id.to_string(),
            self.id.clone(),
            asset_id.clone(),
            tx_type,
            date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            note,
            None, // realized_pnl recomputed below
            created_at,
        )?;

        // Replace the transaction in-memory
        if let Some(slot) = self.transactions.iter_mut().find(|t| t.id == tx_id) {
            *slot = updated_tx;
        } else {
            return Err(AccountOperationError::TransactionNotFound.into());
        }

        // Full recalculation for the (account, asset) pair — SEL-032 cascading check inside
        let pair_txs: Vec<&Transaction> = self
            .transactions
            .iter()
            .filter(|t| t.asset_id == asset_id)
            .collect();
        let (holding, pnl_map) = self.recalculate_holding(&asset_id, &pair_txs)?;

        // Attach updated realized_pnl to all sells in the pair (excluding the corrected tx itself,
        // which is handled unconditionally below to cover the Purchase case too)
        for tx in self
            .transactions
            .iter_mut()
            .filter(|t| t.asset_id == asset_id && t.id != tx_id)
        {
            if tx.transaction_type == TransactionType::Sell {
                tx.realized_pnl = pnl_map.get(&tx.id).copied();
                self.pending_changes
                    .push(AccountChange::TransactionUpdated(tx.clone()));
            }
        }
        // The corrected transaction itself — always record so the repository gets the latest state
        let corrected = self
            .transactions
            .iter()
            .find(|t| t.id == tx_id)
            .ok_or_else(|| {
                anyhow!(
                    "BUG: tx {} missing after update in account {}",
                    tx_id,
                    self.id
                )
            })?;
        // Ensure the corrected tx is always recorded (re-push to overwrite any earlier entry;
        // repository applies changes in order so the last write wins)
        self.pending_changes
            .push(AccountChange::TransactionUpdated(corrected.clone()));

        self.pending_changes
            .push(AccountChange::HoldingUpserted(holding.clone()));
        self.upsert_holding_in_memory(holding);

        // CSH-042 / CSH-051 — chronological replay over Deposit / Withdrawal / Purchase / Sell.
        // OpeningBalance corrections do not touch cash (CSH-060), so the replay is harmless on them.
        self.replay_cash_holding()?;

        self.transactions
            .iter()
            .find(|t| t.id == tx_id)
            .ok_or_else(|| {
                anyhow!(
                    "BUG: tx {} missing after update in account {}",
                    tx_id,
                    self.id
                )
            })
    }

    /// Deletes an existing transaction and recalculates (or removes) the associated holding
    /// (TRX-034, SEL-033, SEL-026).
    pub fn cancel_transaction(&mut self, tx_id: &str) -> Result<()> {
        let asset_id = self
            .transactions
            .iter()
            .find(|t| t.id == tx_id)
            .ok_or(AccountOperationError::TransactionNotFound)?
            .asset_id
            .clone();
        let pos = self
            .transactions
            .iter()
            .position(|t| t.id == tx_id)
            .ok_or(AccountOperationError::TransactionNotFound)?;
        self.transactions.remove(pos);
        self.pending_changes
            .push(AccountChange::TransactionDeleted(tx_id.to_string()));

        let remaining: Vec<&Transaction> = self
            .transactions
            .iter()
            .filter(|t| t.asset_id == asset_id)
            .collect();

        if remaining.is_empty() {
            // Remove the holding — no transactions left for this pair
            self.holdings
                .retain(|h| !(h.account_id == self.id && h.asset_id == asset_id));
            self.pending_changes.push(AccountChange::HoldingDeleted {
                account_id: self.id.clone(),
                asset_id,
            });
        } else {
            // SEL-033 — full recalculation updates realized_pnl on remaining sells
            let (holding, pnl_map) = self.recalculate_holding(&asset_id, &remaining)?;
            for tx in self
                .transactions
                .iter_mut()
                .filter(|t| t.asset_id == asset_id && t.transaction_type == TransactionType::Sell)
            {
                tx.realized_pnl = pnl_map.get(&tx.id).copied();
                self.pending_changes
                    .push(AccountChange::TransactionUpdated(tx.clone()));
            }
            self.pending_changes
                .push(AccountChange::HoldingUpserted(holding.clone()));
            self.upsert_holding_in_memory(holding);
        }

        // CSH-024 / CSH-051 — replay cash after the cancellation. Cancelling a Deposit, Buy, or
        // Sell can change the cash trajectory; cancelling a Withdrawal only ever raises the
        // running balance and never trips InsufficientCash. OpeningBalance cancels are harmless.
        self.replay_cash_holding()?;

        Ok(())
    }

    /// Seeds a holding directly from a quantity and total cost, without full transaction history
    /// (TRX-042, TRX-047, TRX-048).
    ///
    /// `total_amount = total_cost` (direct). `unit_price = floor(total_cost * MICRO / quantity)`.
    /// `fees = 0`, `exchange_rate = 1_000_000`. TRX-026 formula does not apply.
    /// OpeningBalance rows participate in VWAP identically to Purchase (TRX-048).
    pub fn open_holding(
        &mut self,
        asset_id: String,
        date: String,
        quantity: i64,
        total_cost: i64,
    ) -> Result<&Transaction> {
        if quantity <= 0 {
            return Err(TransactionDomainError::QuantityNotPositive.into());
        }
        if total_cost <= 0 {
            return Err(OpeningBalanceDomainError::InvalidTotalCost.into());
        }
        const MICRO: i128 = 1_000_000;
        let unit_price = (total_cost as i128 * MICRO / quantity as i128) as i64;
        let tx = Transaction::new(
            self.id.clone(),
            asset_id.clone(),
            TransactionType::OpeningBalance,
            date,
            quantity,
            unit_price,
            1_000_000, // exchange_rate = 1.0 (TRX-047)
            0,         // fees = 0 (TRX-047)
            total_cost,
            None, // no note (TRX-043)
            None, // realized_pnl not applicable
        )?;
        self.transactions.push(tx);
        let tx_ref = self
            .transactions
            .last()
            .ok_or_else(|| anyhow!("BUG: tx list empty after push in account {}", self.id))?;

        let pair_txs: Vec<&Transaction> = self
            .transactions
            .iter()
            .filter(|t| t.asset_id == asset_id)
            .collect();
        let (holding, _) = self.recalculate_holding(&asset_id, &pair_txs)?;

        self.pending_changes
            .push(AccountChange::TransactionInserted(tx_ref.clone()));
        self.pending_changes
            .push(AccountChange::HoldingUpserted(holding.clone()));
        self.upsert_holding_in_memory(holding);

        self.transactions
            .last()
            .ok_or_else(|| anyhow!("BUG: tx list empty after push in account {}", self.id))
    }

    // -------------------------------------------------------------------------
    // Cash transactions (CSH spec)
    // -------------------------------------------------------------------------

    /// Returns the deterministic asset_id of the system Cash Asset for this account's currency
    /// (CSH-011). Format: `system-cash-{ccy_lower}` (e.g. `system-cash-eur`).
    pub fn cash_asset_id(&self) -> String {
        crate::core::cash::system_cash_asset_id(&self.currency)
    }

    /// Returns the current cash balance for this account, or 0 if no Cash Holding exists yet.
    pub fn cash_holding_quantity(&self) -> i64 {
        self.holding_quantity(&self.cash_asset_id())
    }

    /// Aggregate-root method: applies a pre-built Deposit transaction to this
    /// account (CSH-022). The transaction must have been built via
    /// `Transaction::new_deposit` so TRX-020 is already validated. Pushes to
    /// history, queues the `TransactionInserted` change, and replays the cash
    /// holding (CSH-012 lazy-creates the Cash Holding on first deposit).
    ///
    /// Returns `AccountOperationError` only if the chronological replay surfaces
    /// `InsufficientCash` (e.g. an out-of-order back-dated deposit interleaved
    /// with prior withdrawals would briefly drive the running balance negative).
    pub fn apply_deposit(
        &mut self,
        tx: Transaction,
    ) -> StdResult<Transaction, AccountOperationError> {
        self.transactions.push(tx.clone());
        self.pending_changes
            .push(AccountChange::TransactionInserted(tx.clone()));
        if let Err(e) = self.replay_cash_holding() {
            self.transactions.pop();
            self.pending_changes.pop();
            return Err(e);
        }
        Ok(tx)
    }

    /// Aggregate-root method: applies a pre-built Withdrawal transaction to
    /// this account (CSH-032). The transaction must have been built via
    /// `Transaction::new_withdrawal`. Enforces CSH-080 (insufficient cash)
    /// before any mutation so a rejected transaction is never left in
    /// `self.transactions`. Withdrawals do not lazy-create the Cash Holding —
    /// only Deposit and Sell do (CSH-012).
    pub fn apply_withdrawal(
        &mut self,
        tx: Transaction,
    ) -> StdResult<Transaction, AccountOperationError> {
        let current = self.cash_holding_quantity();
        // Compare against `total_amount` to match `replay_cash_holding`'s deduction
        // field. For cash withdrawals built via `Transaction::new_withdrawal` the
        // two are equal, but a future caller wiring through `Transaction::new`
        // directly would still see a consistent guard.
        if current < tx.total_amount {
            return Err(AccountOperationError::InsufficientCash {
                current_balance_micros: current,
                currency: self.currency.clone(),
            });
        }
        self.transactions.push(tx.clone());
        self.pending_changes
            .push(AccountChange::TransactionInserted(tx.clone()));
        if let Err(e) = self.replay_cash_holding() {
            self.transactions.pop();
            self.pending_changes.pop();
            return Err(e);
        }
        Ok(tx)
    }

    // Cash deposit / withdrawal recording is composed at the application layer
    // (see `AccountService::record_deposit` / `record_withdrawal`) by chaining
    // `Transaction::new_deposit` / `new_withdrawal` (TRX-020) and `apply_deposit`
    // / `apply_withdrawal` (CSH-080). The legacy `Account::record_*` wrappers —
    // which used to live here and return the now-deleted `CashOperationError`
    // composite — survive only as `#[cfg(test)]` test ergonomics helpers (see
    // the `cfg(test)` impl block at the bottom of this file). Production code
    // MUST go through the service.

    /// Replays the cash holding from scratch over all cash-affecting transactions
    /// (Deposit, Withdrawal, Purchase, Sell — OpeningBalance is excluded per CSH-060)
    /// in `(date ASC, created_at ASC)` order. Validates running balance is never strictly
    /// negative; raises `InsufficientCash` otherwise (CSH-080).
    ///
    /// On success, queues the appropriate `AccountChange` (Upserted or Deleted) and updates
    /// `self.holdings` in memory. CSH-013: the Cash Holding is deleted when no
    /// Deposit / Withdrawal transactions remain for this account.
    ///
    /// Returns a typed `AccountOperationError` rather than `anyhow::Result` because
    /// `InsufficientCash` is the only failure mode and callers benefit from knowing it
    /// statically.
    fn replay_cash_holding(&mut self) -> Result<(), AccountOperationError> {
        let cash_asset_id = self.cash_asset_id();

        // Walk cash-affecting transactions chronologically.
        let mut cash_txs: Vec<&Transaction> = self
            .transactions
            .iter()
            .filter(|t| {
                matches!(
                    t.transaction_type,
                    TransactionType::Deposit
                        | TransactionType::Withdrawal
                        | TransactionType::Purchase
                        | TransactionType::Sell
                )
            })
            .collect();
        cash_txs.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        let mut running: i64 = 0;
        for t in &cash_txs {
            match t.transaction_type {
                TransactionType::Deposit | TransactionType::Sell => {
                    running = running.saturating_add(t.total_amount);
                }
                TransactionType::Withdrawal | TransactionType::Purchase => {
                    if running < t.total_amount {
                        return Err(AccountOperationError::InsufficientCash {
                            current_balance_micros: running,
                            currency: self.currency.clone(),
                        });
                    }
                    running -= t.total_amount;
                }
                _ => {}
            }
        }

        // CSH-013 / TRX-034 cleanup: when no Deposit / Withdrawal remain *and* the running
        // balance is zero, drop the Cash Holding. (The "pair" for cash, by analogy with
        // TRX-034, is Deposit + Withdrawal; Purchase / Sell touch cash via side-effect but
        // are owned by their non-cash asset's pair.)
        let cash_pair_remains = self.transactions.iter().any(|t| {
            matches!(
                t.transaction_type,
                TransactionType::Deposit | TransactionType::Withdrawal
            )
        });
        let existing_cash_holding = self.holdings.iter().find(|h| h.asset_id == cash_asset_id);
        if running == 0 && !cash_pair_remains {
            if existing_cash_holding.is_some() {
                self.holdings.retain(|h| h.asset_id != cash_asset_id);
                self.pending_changes.push(AccountChange::HoldingDeleted {
                    account_id: self.id.clone(),
                    asset_id: cash_asset_id,
                });
            }
            return Ok(());
        }

        // Upsert the Cash Holding with average_price = 1.0, total_realized_pnl = 0,
        // last_sold_date = None — invariants from the spec entity definition.
        // `Holding::restore` skips validation: `running` is guaranteed >= 0 by the
        // replay invariant above, and the constant 1_000_000 average_price is positive,
        // so the typical `Holding::new` validation would always succeed.
        let holding_id = existing_cash_holding
            .map(|h| h.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let holding = Holding::restore(
            holding_id,
            self.id.clone(),
            cash_asset_id,
            running,
            1_000_000,
            0,
            None,
        );
        self.pending_changes
            .push(AccountChange::HoldingUpserted(holding.clone()));
        self.upsert_holding_in_memory(holding);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Returns the current quantity for a (account, asset) pair, or 0 if no holding exists.
    fn holding_quantity(&self, asset_id: &str) -> i64 {
        self.holdings
            .iter()
            .find(|h| h.asset_id == asset_id)
            .map(|h| h.quantity)
            .unwrap_or(0)
    }

    /// Upserts a holding in the in-memory list.
    fn upsert_holding_in_memory(&mut self, holding: Holding) {
        if let Some(existing) = self
            .holdings
            .iter_mut()
            .find(|h| h.asset_id == holding.asset_id)
        {
            *existing = holding;
        } else {
            self.holdings.push(holding);
        }
    }

    /// Full chronological recalculation of Holding state and realized P&L for the given
    /// transaction slice (TRX-030, SEL-024, SEL-025, SEL-026, SEL-027, SEL-032).
    ///
    /// Returns `(updated_holding, sell_tx_id → realized_pnl)`.
    /// Returns `AccountOperationError::CascadingOversell` if any sell exceeds running qty.
    fn recalculate_holding(
        &self,
        asset_id: &str,
        transactions: &[&Transaction],
    ) -> Result<(Holding, std::collections::HashMap<String, i64>)> {
        use std::collections::HashMap;
        const MICRO: i128 = 1_000_000;

        let mut total_quantity: i128 = 0;
        let mut vwap_numerator: i128 = 0;
        let mut last_vwap: i64 = 0;
        let mut pnl_map: HashMap<String, i64> = HashMap::new();
        let mut total_realized_pnl: i64 = 0;
        let mut last_sold_date: Option<String> = None;

        for t in transactions {
            match t.transaction_type {
                TransactionType::Purchase | TransactionType::OpeningBalance => {
                    let qty = t.quantity as i128;
                    total_quantity += qty;
                    vwap_numerator += t.total_amount as i128 * MICRO;
                }
                TransactionType::Sell => {
                    if t.quantity as i128 > total_quantity {
                        return Err(AccountOperationError::CascadingOversell.into());
                    }
                    let vwap_before: i64 = if total_quantity > 0 {
                        (vwap_numerator / total_quantity) as i64
                    } else {
                        0
                    };
                    last_vwap = vwap_before;
                    let pnl = Self::compute_realized_pnl(t.total_amount, vwap_before, t.quantity);
                    pnl_map.insert(t.id.clone(), pnl);
                    total_realized_pnl += pnl;
                    if last_sold_date.as_deref() < Some(t.date.as_str()) {
                        last_sold_date = Some(t.date.clone());
                    }
                    let qty = t.quantity as i128;
                    vwap_numerator -= vwap_before as i128 * qty;
                    total_quantity -= qty;
                }
                // CSH-022: a Deposit credits cash quantity by total_amount; vwap stays at 1.0.
                // unit_price and exchange_rate are both 1_000_000, so the vwap_numerator
                // contribution equals total_amount * MICRO, matching Purchase math.
                TransactionType::Deposit => {
                    let qty = t.quantity as i128;
                    total_quantity += qty;
                    vwap_numerator += t.total_amount as i128 * MICRO;
                }
                // CSH-032: a Withdrawal debits cash quantity by total_amount; never realises P&L
                // and never tracks last_sold_date. CSH-080's eligibility guard runs in
                // `replay_cash_holding` (insufficient-cash check), not here — `recalculate_holding`
                // is shared with Sell oversell which is a CascadingOversell, a different error.
                TransactionType::Withdrawal => {
                    if t.quantity as i128 > total_quantity {
                        return Err(AccountOperationError::InsufficientCash {
                            current_balance_micros: total_quantity as i64,
                            currency: self.currency.clone(),
                        }
                        .into());
                    }
                    let qty = t.quantity as i128;
                    total_quantity -= qty;
                    // For a Withdrawal we shrink the running vwap_numerator proportionally so the
                    // average_price stays at 1.0 (cash is its own unit).
                    if total_quantity > 0 {
                        vwap_numerator = total_quantity * MICRO;
                    } else {
                        vwap_numerator = 0;
                    }
                }
            }
        }

        // SEL-026 / TRX-040 — retain holding at qty=0, preserve last VWAP
        let average_price: i64 = if total_quantity > 0 {
            (vwap_numerator / total_quantity) as i64
        } else {
            last_vwap
        };
        let quantity = total_quantity as i64;

        let holding = match self.holdings.iter().find(|h| h.asset_id == asset_id) {
            Some(existing) => Holding::with_id(
                existing.id.clone(),
                self.id.clone(),
                asset_id.to_string(),
                quantity,
                average_price,
                total_realized_pnl,
                last_sold_date,
            )?,
            None => Holding::new(
                self.id.clone(),
                asset_id.to_string(),
                quantity,
                average_price,
                total_realized_pnl,
                last_sold_date,
            )?,
        };

        Ok((holding, pnl_map))
    }

    /// Computes total_amount for a Purchase (TRX-026).
    /// Formula: floor(floor(qty × price / MICRO) × rate / MICRO) + fees
    fn compute_purchase_total(
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
    ) -> i64 {
        const MICRO: i128 = 1_000_000;
        let qty = quantity as i128;
        let price = unit_price as i128;
        let rate = exchange_rate as i128;
        ((qty * price / MICRO) * rate / MICRO) as i64 + fees
    }

    /// Computes total_amount for a Sell (SEL-023).
    /// Formula: floor(floor(qty × price / MICRO) × rate / MICRO) - fees
    fn compute_sell_total(quantity: i64, unit_price: i64, exchange_rate: i64, fees: i64) -> i64 {
        const MICRO: i128 = 1_000_000;
        let qty = quantity as i128;
        let price = unit_price as i128;
        let rate = exchange_rate as i128;
        ((qty * price / MICRO) * rate / MICRO) as i64 - fees
    }

    /// Computes total_amount for an OpeningBalance correction (TRX-051).
    /// Formula: floor(qty × unit_price / MICRO) — no exchange_rate factor.
    fn compute_opening_balance_total(quantity: i64, unit_price: i64) -> i64 {
        const MICRO: i128 = 1_000_000;
        (quantity as i128 * unit_price as i128 / MICRO) as i64
    }

    /// Computes realized P&L for a sell (SEL-024).
    /// realized_pnl = total_sell_amount - floor(vwap_before_sell × sold_quantity / MICRO)
    fn compute_realized_pnl(
        total_sell_amount: i64,
        vwap_before_sell: i64,
        sold_quantity: i64,
    ) -> i64 {
        const MICRO: i128 = 1_000_000;
        let cost_basis = (vwap_before_sell as i128 * sold_quantity as i128 / MICRO) as i64;
        total_sell_amount - cost_basis
    }

    fn validate_currency(currency: &str) -> StdResult<(), AccountDomainError> {
        if Currency::from_str(currency).is_err() {
            return Err(AccountDomainError::InvalidCurrency {
                currency: currency.to_string(),
            });
        }
        Ok(())
    }
}

/// Interface for account persistence.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Fetches all accounts.
    async fn get_all(&self) -> Result<Vec<Account>>;
    /// Fetches an account by its ID.
    async fn get_by_id(&self, id: &str) -> Result<Option<Account>>;
    /// Finds an account by name (case-insensitive, R3).
    async fn find_by_name(&self, name: &str) -> Result<Option<Account>>;
    /// Persists a new account.
    async fn create(&self, account: Account) -> Result<Account>;
    /// Updates an existing account.
    async fn update(&self, account: Account) -> Result<Account>;
    /// Permanently deletes an account and cascades to its holdings (R5).
    async fn delete(&self, id: &str) -> Result<()>;
    /// Loads the full aggregate: account + all holdings + all transactions (ordered by date, created_at).
    async fn get_with_holdings_and_transactions(&self, id: &str) -> Result<Option<Account>>;
    /// Atomically applies all pending changes accumulated by aggregate operations.
    /// Clears `pending_changes` on the aggregate after a successful commit.
    async fn save(&self, account: &mut Account) -> Result<()>;
}

/// Test-only convenience helpers for cash recording. Production code composes
/// the same logic at the application layer (`AccountService::record_deposit` /
/// `record_withdrawal`); these helpers exist purely so existing tests can
/// continue to call `acc.record_deposit(...)` without spelling out the
/// factory + apply two-step on every line. They return `AccountOperationError`
/// directly (the only possible non-input failure source on valid test data);
/// factory failures are `.expect()`-ed since test inputs are assumed valid.
#[cfg(test)]
impl Account {
    /// Test-only helper mirroring the legacy production wrapper. `pub(crate)`
    /// so non-test code in other crates can never accidentally call it.
    /// Tests passing `amount <= 0` will panic via the factory's
    /// `AmountNotPositive` (caught by `.expect()`); to assert that error type
    /// directly, call `Transaction::new_deposit` instead of this helper.
    pub(crate) fn record_deposit(
        &mut self,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> StdResult<Transaction, AccountOperationError> {
        let tx =
            Transaction::new_deposit(self.id.clone(), self.cash_asset_id(), date, amount, note)
                .expect("test transaction inputs must be valid");
        self.apply_deposit(tx)
    }

    /// Test-only helper mirroring the legacy production wrapper. See
    /// `record_deposit` for the panic-on-invalid-input contract.
    pub(crate) fn record_withdrawal(
        &mut self,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> StdResult<Transaction, AccountOperationError> {
        let tx =
            Transaction::new_withdrawal(self.id.clone(), self.cash_asset_id(), date, amount, note)
                .expect("test transaction inputs must be valid");
        self.apply_withdrawal(tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn micro(v: i64) -> i64 {
        v * 1_000_000
    }

    fn base_account() -> Account {
        Account::restore_with_positions(
            "acc-1".to_string(),
            "Test".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Returns a base account pre-seeded with a large cash balance so existing buy/sell
    /// tests don't trip CSH-041 (Insufficient cash on Purchase).
    fn cash_seeded_account() -> Account {
        let mut acc = base_account();
        acc.record_deposit("2020-01-01".to_string(), 1_000_000_000_000, None)
            .unwrap();
        // Drain pending_changes so tests that count emitted changes start clean.
        acc.pending_changes.clear();
        acc
    }

    // R1 — trim at creation
    #[test]
    fn new_trims_leading_trailing_spaces() {
        let account = Account::new(
            "  My Account  ".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
        )
        .unwrap();
        assert_eq!(account.name, "My Account");
    }

    // R1, R2 — spaces-only name is invalid after trim
    #[test]
    fn new_rejects_whitespace_only_name() {
        let result = Account::new(
            "   ".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
        );
        assert!(result.is_err());
    }

    // currency — invalid ISO code rejected
    #[test]
    fn new_rejects_invalid_currency() {
        let result = Account::new(
            "My Account".to_string(),
            "INVALID".to_string(),
            UpdateFrequency::ManualMonth,
        );
        assert!(result.is_err());
    }

    // R1, R2 — with_id trims and validates
    #[test]
    fn with_id_trims_name() {
        let account = Account::with_id(
            "some-id".to_string(),
            "  Trimmed  ".to_string(),
            "USD".to_string(),
            UpdateFrequency::ManualDay,
        )
        .unwrap();
        assert_eq!(account.name, "Trimmed");
    }

    // R1, R2 — with_id rejects empty name after trim
    #[test]
    fn with_id_rejects_empty_name_after_trim() {
        let result = Account::with_id(
            "some-id".to_string(),
            "  ".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
        );
        assert!(result.is_err());
    }

    // TRX-026 / TRX-030 — buy_holding updates VWAP correctly (2 purchases)
    #[test]
    fn buy_holding_updates_vwap() {
        let mut acc = cash_seeded_account();
        // Buy 2 units @ 100.00 → total = 200.00
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(2),
            micro(100),
            micro(1),
            0,
            None,
        )
        .unwrap();
        // Buy 2 units @ 200.00 → total = 400.00; VWAP = 600/4 = 150.00
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-02-01".to_string(),
            micro(2),
            micro(200),
            micro(1),
            0,
            None,
        )
        .unwrap();

        let h = acc
            .holdings
            .iter()
            .find(|h| h.asset_id == "asset-1")
            .unwrap();
        assert_eq!(h.quantity, micro(4));
        assert_eq!(h.average_price, micro(150));
    }

    // SEL-012 — sell_holding on a zero-qty position is rejected
    #[test]
    fn sell_holding_rejects_closed_position() {
        let mut acc = cash_seeded_account();
        let err = acc
            .sell_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap_err();
        assert!(
            err.downcast_ref::<AccountOperationError>()
                .map(|e| matches!(e, AccountOperationError::ClosedPosition))
                .unwrap_or(false),
            "expected ClosedPosition, got: {err}"
        );
    }

    // SEL-021 — sell_holding rejects quantity exceeding available
    #[test]
    fn sell_holding_rejects_oversell() {
        let mut acc = cash_seeded_account();
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(1),
            micro(100),
            micro(1),
            0,
            None,
        )
        .unwrap();
        let err = acc
            .sell_holding(
                "asset-1".to_string(),
                "2024-06-01".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap_err();
        assert!(
            err.downcast_ref::<AccountOperationError>()
                .map(|e| matches!(e, AccountOperationError::Oversell { .. }))
                .unwrap_or(false),
            "expected Oversell, got: {err}"
        );
    }

    // SEL-024 — sell_holding computes P&L: sell 1 unit @ 150 after buying @ 100 → P&L = +50
    #[test]
    fn sell_holding_computes_realized_pnl() {
        let mut acc = cash_seeded_account();
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(1),
            micro(100),
            micro(1),
            0,
            None,
        )
        .unwrap();
        let tx = acc
            .sell_holding(
                "asset-1".to_string(),
                "2024-06-01".to_string(),
                micro(1),
                micro(150),
                micro(1),
                0,
                None,
            )
            .unwrap();
        assert_eq!(tx.realized_pnl, Some(micro(50)));
    }

    // TRX-031 — correct_transaction recalculates holding
    #[test]
    fn correct_transaction_recalculates_holding() {
        let mut acc = cash_seeded_account();
        let tx = acc
            .buy_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap()
            .clone();

        // Correct: change unit_price to 200 → total = 400, VWAP = 200
        acc.correct_transaction(
            &tx.id,
            "2024-01-01".to_string(),
            micro(2),
            micro(200),
            micro(1),
            0,
            None,
        )
        .unwrap();

        let h = acc
            .holdings
            .iter()
            .find(|h| h.asset_id == "asset-1")
            .unwrap();
        assert_eq!(h.average_price, micro(200));
    }

    // TRX-034 — cancel_transaction removes holding when it was the last transaction
    #[test]
    fn cancel_transaction_removes_holding_when_last() {
        let mut acc = cash_seeded_account();
        let tx = acc
            .buy_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap()
            .clone();

        acc.cancel_transaction(&tx.id).unwrap();

        assert!(
            acc.holdings.iter().all(|h| h.asset_id != "asset-1"),
            "asset-1 holding should be removed"
        );
        assert!(
            acc.transactions.iter().all(|t| t.id != tx.id),
            "purchase transaction should be removed"
        );
    }

    // -------------------------------------------------------------------------
    // Opening balance tests (TRX-042 through TRX-051)
    // -------------------------------------------------------------------------

    // TRX-044 — open_holding rejects quantity ≤ 0
    // TransactionDomainError::QuantityNotPositive is checked via error message;
    // the exact variant will be confirmed by the downcast once the impl imports it.
    #[test]
    fn open_holding_rejects_zero_quantity() {
        let mut acc = cash_seeded_account();
        let err = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                0,
                micro(100),
            )
            .unwrap_err();
        // Check via error message — TransactionDomainError::QuantityNotPositive message:
        // "Quantity must be strictly positive"
        assert!(
            err.to_string().contains("positive"),
            "expected QuantityNotPositive error, got: {err}"
        );
    }

    // TRX-044 — open_holding rejects negative quantity
    #[test]
    fn open_holding_rejects_negative_quantity() {
        let mut acc = cash_seeded_account();
        let err = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                -micro(1),
                micro(100),
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("positive"),
            "expected QuantityNotPositive error, got: {err}"
        );
    }

    // TRX-045 — open_holding rejects total_cost ≤ 0
    #[test]
    fn open_holding_rejects_zero_total_cost() {
        let mut acc = cash_seeded_account();
        let err = acc
            .open_holding("asset-1".to_string(), "2024-01-01".to_string(), micro(1), 0)
            .unwrap_err();
        // OpeningBalanceDomainError is in scope via `use super::*` once implemented
        assert!(
            err.downcast_ref::<OpeningBalanceDomainError>()
                .map(|e| matches!(e, OpeningBalanceDomainError::InvalidTotalCost))
                .unwrap_or(false),
            "expected InvalidTotalCost, got: {err}"
        );
    }

    // TRX-045 — open_holding rejects negative total_cost
    #[test]
    fn open_holding_rejects_negative_total_cost() {
        let mut acc = cash_seeded_account();
        let err = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                -micro(1),
            )
            .unwrap_err();
        // OpeningBalanceDomainError is in scope via `use super::*` once implemented
        assert!(
            err.downcast_ref::<OpeningBalanceDomainError>()
                .map(|e| matches!(e, OpeningBalanceDomainError::InvalidTotalCost))
                .unwrap_or(false),
            "expected InvalidTotalCost, got: {err}"
        );
    }

    // TRX-046 — open_holding rejects future date
    #[test]
    fn open_holding_rejects_future_date() {
        let mut acc = cash_seeded_account();
        let err = acc
            .open_holding(
                "asset-1".to_string(),
                "2099-12-31".to_string(),
                micro(1),
                micro(100),
            )
            .unwrap_err();
        // TransactionDomainError::DateInFuture message: "Transaction date cannot be in the future"
        assert!(
            err.to_string().contains("future"),
            "expected DateInFuture error, got: {err}"
        );
    }

    // TRX-046 — open_holding rejects date before 1900-01-01
    #[test]
    fn open_holding_rejects_date_too_old() {
        let mut acc = cash_seeded_account();
        let err = acc
            .open_holding(
                "asset-1".to_string(),
                "1899-12-31".to_string(),
                micro(1),
                micro(100),
            )
            .unwrap_err();
        // TransactionDomainError::DateTooOld message: "Transaction date cannot be before 1900-01-01"
        assert!(
            err.to_string().contains("1900-01-01"),
            "expected DateTooOld error, got: {err}"
        );
    }

    // TRX-047 — open_holding stores total_amount = total_cost directly
    #[test]
    fn open_holding_sets_total_amount_equal_to_total_cost() {
        let mut acc = cash_seeded_account();
        let total_cost = micro(500); // 500.000000 in account currency
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(2),
                total_cost,
            )
            .unwrap();
        assert_eq!(
            tx.total_amount, total_cost,
            "total_amount must equal total_cost"
        );
    }

    // TRX-047 — open_holding sets fees = 0
    #[test]
    fn open_holding_sets_fees_to_zero() {
        let mut acc = cash_seeded_account();
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(2),
                micro(500),
            )
            .unwrap();
        assert_eq!(tx.fees, 0, "fees must be 0 for OpeningBalance");
    }

    // TRX-047 — open_holding sets exchange_rate = 1_000_000
    #[test]
    fn open_holding_sets_exchange_rate_to_one() {
        let mut acc = cash_seeded_account();
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(2),
                micro(500),
            )
            .unwrap();
        assert_eq!(
            tx.exchange_rate, 1_000_000,
            "exchange_rate must be 1.0 (1_000_000 micro)"
        );
    }

    // TRX-047 — open_holding computes unit_price = floor(total_cost * MICRO / quantity)
    #[test]
    fn open_holding_computes_unit_price_as_floor_of_cost_over_qty() {
        let mut acc = cash_seeded_account();
        // quantity = 3_000_000 (3.0), total_cost = 10_000_000 (10.0)
        // unit_price = floor(10_000_000 * 1_000_000 / 3_000_000) = floor(3_333_333.33) = 3_333_333
        let quantity = 3 * 1_000_000i64;
        let total_cost = 10 * 1_000_000i64;
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                quantity,
                total_cost,
            )
            .unwrap();
        let expected_unit_price = (total_cost as i128 * 1_000_000 / quantity as i128) as i64;
        assert_eq!(
            tx.unit_price, expected_unit_price,
            "unit_price must be floor(total_cost*MICRO/qty)"
        );
    }

    // TRX-047 — open_holding sets transaction_type = OpeningBalance
    #[test]
    fn open_holding_sets_transaction_type_to_opening_balance() {
        let mut acc = cash_seeded_account();
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .unwrap();
        assert_eq!(tx.transaction_type, TransactionType::OpeningBalance);
    }

    // TRX-048 — OpeningBalance participates in VWAP identically to Purchase
    // 1 OpeningBalance of 2 units @ total 200 + 1 Purchase of 2 units @ 200
    // VWAP = (200 + 200) / 4 = 100
    #[test]
    fn open_holding_participates_in_vwap_identically_to_purchase() {
        let mut acc = cash_seeded_account();
        // OpeningBalance: 2 units, total_cost = 200
        acc.open_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(2),
            micro(200),
        )
        .unwrap();
        // Purchase: 2 units @ 100 each → total = 200
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-02-01".to_string(),
            micro(2),
            micro(100),
            micro(1),
            0,
            None,
        )
        .unwrap();

        let h = acc
            .holdings
            .iter()
            .find(|h| h.asset_id == "asset-1")
            .unwrap();
        // VWAP = (200 + 200) / (2 + 2) = 100
        assert_eq!(h.quantity, micro(4), "total quantity must accumulate");
        assert_eq!(
            h.average_price,
            micro(100),
            "VWAP must include OpeningBalance"
        );
    }

    // TRX-049 — multiple OpeningBalance entries allowed for same (account, asset) pair
    #[test]
    fn open_holding_allows_multiple_for_same_pair() {
        let mut acc = cash_seeded_account();
        let r1 = acc
            .open_holding(
                "asset-1".to_string(),
                "2023-01-01".to_string(),
                micro(1),
                micro(100),
            )
            .cloned();
        let r2 = acc
            .open_holding(
                "asset-1".to_string(),
                "2023-06-01".to_string(),
                micro(2),
                micro(200),
            )
            .cloned();
        assert!(r1.is_ok(), "first open_holding must succeed");
        assert!(r2.is_ok(), "second open_holding must succeed for same pair");
        let h = acc
            .holdings
            .iter()
            .find(|h| h.asset_id == "asset-1")
            .unwrap();
        assert_eq!(
            h.quantity,
            micro(3),
            "quantities must accumulate across multiple openings"
        );
    }

    // TRX-051 (backend) — correct_transaction on an OpeningBalance row recomputes
    // total_amount = quantity * unit_price / MICRO (not TRX-026 purchase formula)
    #[test]
    fn correct_transaction_on_opening_balance_recomputes_total_from_qty_and_price() {
        let mut acc = cash_seeded_account();
        // Create an opening balance: 2 units, total_cost = 200 → unit_price = 100_000_000
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(2),
                micro(200),
            )
            .unwrap()
            .clone();

        // Correct it: change quantity to 3, keep unit_price from original (100_000_000 micro = 100)
        // For OpeningBalance correction: total_amount = floor(3_000_000 * 100_000_000 / 1_000_000)
        //   = 300_000_000 (300.0)
        // NOT the TRX-026 purchase formula with exchange_rate
        let corrected = acc
            .correct_transaction(
                &tx.id,
                "2024-01-01".to_string(),
                micro(3),      // new quantity
                tx.unit_price, // keep same unit_price
                1_000_000,     // exchange_rate (must be 1 for OpeningBalance)
                0,             // fees (must be 0 for OpeningBalance)
                None,
            )
            .unwrap();

        // total_amount should be floor(qty * unit_price / MICRO) — not using exchange_rate
        let expected = (micro(3) as i128 * tx.unit_price as i128 / 1_000_000) as i64;
        assert_eq!(
            corrected.total_amount, expected,
            "corrected OpeningBalance total_amount must use qty*unit_price/MICRO formula"
        );
    }

    // TRX-047 — open_holding does NOT apply TRX-026 formula (no exchange_rate factor)
    #[test]
    fn open_holding_total_amount_ignores_exchange_rate() {
        let mut acc = cash_seeded_account();
        // total_cost = 1_000_000 (1.0 unit), quantity = 1_000_000 (1.0)
        // TRX-026 would multiply by exchange_rate — but open_holding must not
        let tx = acc
            .open_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(1),
            )
            .unwrap();
        // total_amount must be exactly total_cost — regardless of any implied exchange_rate
        assert_eq!(tx.total_amount, micro(1));
        // exchange_rate is always 1_000_000 (1.0) per TRX-047
        assert_eq!(tx.exchange_rate, 1_000_000);
    }

    // SEL-026 — cancel_transaction retains holding at qty=0 when other transactions remain
    #[test]
    fn cancel_transaction_retains_holding_when_transactions_remain() {
        let mut acc = cash_seeded_account();
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(2),
            micro(100),
            micro(1),
            0,
            None,
        )
        .unwrap();
        let sell_tx = acc
            .sell_holding(
                "asset-1".to_string(),
                "2024-06-01".to_string(),
                micro(2),
                micro(150),
                micro(1),
                0,
                None,
            )
            .unwrap()
            .clone();

        // Cancel the sell → holding should remain at qty=2 with VWAP=100
        acc.cancel_transaction(&sell_tx.id).unwrap();

        let h = acc
            .holdings
            .iter()
            .find(|h| h.asset_id == "asset-1")
            .unwrap();
        assert_eq!(h.quantity, micro(2));
        assert_eq!(h.average_price, micro(100));
    }

    // -------------------------------------------------------------------------
    // CSH spec coverage — dedicated assertions for the rules listed in
    // docs/spec/cash-tracking.md. See docs/todo.md "(backend) Cash spec backend
    // test coverage gaps" for the spec-checker run that surfaced them.
    // -------------------------------------------------------------------------

    // CSH-012 — Cash Holding lazy creation: a fresh account has no Cash Holding;
    // the first Deposit creates it and sets quantity = deposited amount.
    #[test]
    fn csh_012_first_deposit_lazily_creates_cash_holding() {
        let mut acc = base_account();
        assert!(
            acc.holdings.is_empty(),
            "fresh account must have no holdings"
        );
        assert_eq!(acc.cash_holding_quantity(), 0);

        acc.record_deposit("2020-01-01".to_string(), 500_000_000, None)
            .unwrap();

        assert_eq!(acc.cash_holding_quantity(), 500_000_000);
        let cash = acc
            .holdings
            .iter()
            .find(|h| h.asset_id == acc.cash_asset_id())
            .expect("cash holding must exist after first deposit");
        assert_eq!(cash.quantity, 500_000_000);
        assert_eq!(cash.average_price, 1_000_000, "cash VWAP is constant 1.0");
    }

    // CSH-013 — Cash Holding lifecycle follows TRX-034: when no Deposit/Withdrawal
    // remain after a delete and the running balance is zero, the holding is removed.
    #[test]
    fn csh_013_cash_holding_removed_when_last_deposit_cancelled() {
        let mut acc = base_account();
        let dep = acc
            .record_deposit("2020-01-01".to_string(), 500_000_000, None)
            .unwrap()
            .clone();
        assert!(acc.cash_holding_quantity() > 0);

        acc.cancel_transaction(&dep.id).unwrap();

        assert!(
            !acc.holdings
                .iter()
                .any(|h| h.asset_id == acc.cash_asset_id()),
            "cash holding must be removed when no cash-pair tx remains"
        );
        assert!(
            acc.pending_changes.iter().any(|c| matches!(
                c,
                AccountChange::HoldingDeleted { asset_id, .. }
                    if asset_id == &acc.cash_asset_id()
            )),
            "HoldingDeleted change must be queued for the cash asset"
        );
    }

    // CSH-022 — Deposit creation: cash quantity rises by amount; AccountChanges
    // include TransactionInserted (the deposit) + HoldingUpserted (cash).
    #[test]
    fn csh_022_deposit_emits_transaction_and_holding_changes() {
        let mut acc = base_account();
        let tx = acc
            .record_deposit("2020-01-01".to_string(), 750_000_000, None)
            .unwrap()
            .clone();

        assert_eq!(acc.cash_holding_quantity(), 750_000_000);
        assert_eq!(tx.transaction_type, TransactionType::Deposit);
        assert_eq!(tx.total_amount, 750_000_000);
        assert!(
            acc.pending_changes.iter().any(|c| matches!(
                c,
                AccountChange::TransactionInserted(t) if t.id == tx.id
            )),
            "TransactionInserted must be queued for the deposit"
        );
        assert!(
            acc.pending_changes.iter().any(|c| matches!(
                c,
                AccountChange::HoldingUpserted(h)
                    if h.asset_id == acc.cash_asset_id() && h.quantity == 750_000_000
            )),
            "HoldingUpserted must reflect the new cash balance"
        );
    }

    // CSH-023 — Deposit edit re-applies chronological replay; the cash holding
    // reflects the new amount.
    #[test]
    fn csh_023_deposit_edit_replays_cash_holding() {
        let mut acc = base_account();
        let dep = acc
            .record_deposit("2020-01-01".to_string(), 500_000_000, None)
            .unwrap()
            .clone();

        acc.correct_transaction(
            &dep.id,
            "2020-01-01".to_string(),
            900_000_000,
            1_000_000,
            1_000_000,
            0,
            None,
        )
        .unwrap();

        assert_eq!(
            acc.cash_holding_quantity(),
            900_000_000,
            "edited deposit must drive the new cash balance"
        );
    }

    // CSH-024 — Deposit delete is rejected when the chronological replay would
    // leave a remaining Withdrawal in violation of CSH-080.
    #[test]
    fn csh_024_deposit_delete_rejected_when_replay_would_overdraw() {
        let mut acc = base_account();
        let dep = acc
            .record_deposit("2020-01-01".to_string(), 1_000_000_000, None)
            .unwrap()
            .clone();
        acc.record_withdrawal("2020-02-01".to_string(), 800_000_000, None)
            .unwrap();

        let err = acc.cancel_transaction(&dep.id).unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AccountOperationError>(),
                Some(AccountOperationError::InsufficientCash { .. })
            ),
            "expected InsufficientCash, got: {err}"
        );
    }

    // CSH-032 — Withdrawal creation: cash quantity decreases by amount; queues
    // both TransactionInserted and HoldingUpserted changes.
    #[test]
    fn csh_032_withdrawal_emits_transaction_and_holding_changes() {
        let mut acc = cash_seeded_account();
        let opening = acc.cash_holding_quantity();
        let wtx = acc
            .record_withdrawal("2020-02-01".to_string(), 250_000_000, None)
            .unwrap()
            .clone();

        assert_eq!(acc.cash_holding_quantity(), opening - 250_000_000);
        assert_eq!(wtx.transaction_type, TransactionType::Withdrawal);
        assert!(
            acc.pending_changes.iter().any(|c| matches!(
                c,
                AccountChange::TransactionInserted(t) if t.id == wtx.id
            )),
            "TransactionInserted must be queued for the withdrawal"
        );
        assert!(
            acc.pending_changes.iter().any(|c| matches!(
                c,
                AccountChange::HoldingUpserted(h) if h.asset_id == acc.cash_asset_id()
            )),
            "HoldingUpserted must be queued reflecting the new balance"
        );
    }

    // CSH-033 — Withdrawal edit re-applies replay; updated amount is reflected
    // in the cash balance.
    #[test]
    fn csh_033_withdrawal_edit_replays_cash_holding() {
        let mut acc = cash_seeded_account();
        let opening = acc.cash_holding_quantity();
        let wtx = acc
            .record_withdrawal("2020-02-01".to_string(), 200_000_000, None)
            .unwrap()
            .clone();
        acc.correct_transaction(
            &wtx.id,
            "2020-02-01".to_string(),
            500_000_000,
            1_000_000,
            1_000_000,
            0,
            None,
        )
        .unwrap();
        assert_eq!(acc.cash_holding_quantity(), opening - 500_000_000);
    }

    // CSH-034 — Withdrawal delete only ever raises the running balance, so it
    // never produces an InsufficientCash rejection.
    #[test]
    fn csh_034_withdrawal_delete_never_raises_insufficient_cash() {
        let mut acc = cash_seeded_account();
        let opening = acc.cash_holding_quantity();
        let wtx = acc
            .record_withdrawal("2020-02-01".to_string(), 400_000_000, None)
            .unwrap()
            .clone();
        acc.cancel_transaction(&wtx.id)
            .expect("deleting a withdrawal must succeed — it can only raise the cash balance");
        assert_eq!(acc.cash_holding_quantity(), opening);
    }

    // CSH-040 — Purchase debits cash by total_amount alongside its asset-side effect.
    #[test]
    fn csh_040_purchase_debits_cash_by_total_amount() {
        let mut acc = cash_seeded_account();
        let opening = acc.cash_holding_quantity();
        // 2 units × 100 = 200 base; × exchange_rate 1.0 + fees 0 → total = 200.
        acc.buy_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(2),
            micro(100),
            micro(1),
            0,
            None,
        )
        .unwrap();
        assert_eq!(
            acc.cash_holding_quantity(),
            opening - micro(200),
            "purchase must debit cash by total_amount (qty×price)"
        );
    }

    // CSH-041 — Purchase eligibility: rejected with InsufficientCash when no
    // Cash Holding exists or its balance < total_amount.
    #[test]
    fn csh_041_purchase_rejected_with_insufficient_cash() {
        let mut acc = base_account(); // no cash deposit at all
        let err = acc
            .buy_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AccountOperationError>(),
                Some(AccountOperationError::InsufficientCash { .. })
            ),
            "expected InsufficientCash, got: {err}"
        );
    }

    // CSH-042 — Purchase edit re-runs the chronological replay; an edit that
    // would leave a later cash-debit in violation is rejected.
    #[test]
    fn csh_042_purchase_edit_rejected_when_replay_would_overdraw() {
        // Start with a tight cash budget so the edit pushes us over.
        let mut acc = base_account();
        acc.record_deposit("2020-01-01".to_string(), micro(300), None)
            .unwrap();
        let buy_tx = acc
            .buy_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(1),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap()
            .clone();
        // Re-edit to require 500 EUR — only 300 is available.
        let err = acc
            .correct_transaction(
                &buy_tx.id,
                "2024-01-01".to_string(),
                micro(5),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AccountOperationError>(),
                Some(AccountOperationError::InsufficientCash { .. })
            ),
            "expected InsufficientCash on overspending edit, got: {err}"
        );
    }

    // CSH-043 — Purchase delete returns cash; never violates CSH-080.
    #[test]
    fn csh_043_purchase_delete_returns_cash() {
        let mut acc = cash_seeded_account();
        let pre = acc.cash_holding_quantity();
        let buy_tx = acc
            .buy_holding(
                "asset-1".to_string(),
                "2024-01-01".to_string(),
                micro(2),
                micro(100),
                micro(1),
                0,
                None,
            )
            .unwrap()
            .clone();
        assert_eq!(acc.cash_holding_quantity(), pre - micro(200));
        acc.cancel_transaction(&buy_tx.id).unwrap();
        assert_eq!(
            acc.cash_holding_quantity(),
            pre,
            "deleting the buy must restore cash to its pre-buy balance"
        );
    }

    // CSH-050 — Sell credits cash and lazy-creates the Cash Holding when this
    // is the first cash-affecting transaction (no prior Deposit).
    #[test]
    fn csh_050_sell_credits_cash_and_lazy_creates_holding() {
        // Seed a holding directly via open_holding so we can sell without the
        // CSH-041 cash-prerequisite of a Deposit.
        let mut acc = base_account();
        acc.open_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(10),
            micro(1_000),
        )
        .unwrap();
        assert_eq!(acc.cash_holding_quantity(), 0, "no cash before the sell");

        acc.sell_holding(
            "asset-1".to_string(),
            "2024-06-01".to_string(),
            micro(2),
            micro(150),
            micro(1),
            0,
            None,
        )
        .unwrap();

        // Sell of 2 × 150 = 300 credits cash by total_amount.
        assert_eq!(
            acc.cash_holding_quantity(),
            micro(300),
            "sell must credit cash by total_amount and lazy-create the holding"
        );
    }

    // CSH-080 — InsufficientCash payload's current_balance_micros equals the
    // cash holding's balance immediately before the rejected mutation would have
    // applied (so the FE can render it without a follow-up fetch).
    #[test]
    fn csh_080_insufficient_cash_payload_carries_pre_mutation_balance() {
        let mut acc = base_account();
        acc.record_deposit("2020-01-01".to_string(), 300_000_000, None)
            .unwrap();
        // Withdrawal of 500 against a balance of 300 → reject with current=300.
        let err = acc
            .record_withdrawal("2020-02-01".to_string(), 500_000_000, None)
            .unwrap_err();
        match err {
            AccountOperationError::InsufficientCash {
                current_balance_micros,
                currency,
            } => {
                assert_eq!(current_balance_micros, 300_000_000);
                assert_eq!(currency, "EUR");
            }
            other => panic!("expected InsufficientCash{{300_000_000, EUR}}, got: {other:?}"),
        }
    }

    // CSH-051 — Sell delete triggers replay across both the sold-asset holding
    // and the Cash Holding; cash returns to its pre-sell balance.
    #[test]
    fn csh_051_sell_delete_replays_cash_holding() {
        let mut acc = base_account();
        acc.open_holding(
            "asset-1".to_string(),
            "2024-01-01".to_string(),
            micro(10),
            micro(1_000),
        )
        .unwrap();
        let sell_tx = acc
            .sell_holding(
                "asset-1".to_string(),
                "2024-06-01".to_string(),
                micro(2),
                micro(150),
                micro(1),
                0,
                None,
            )
            .unwrap()
            .clone();
        assert_eq!(acc.cash_holding_quantity(), micro(300));

        acc.cancel_transaction(&sell_tx.id).unwrap();

        // After the sell is gone, no cash-affecting tx remains → cash holding
        // cleared per CSH-013.
        assert_eq!(
            acc.cash_holding_quantity(),
            0,
            "deleting the only cash-affecting tx must reset cash to 0"
        );
    }

    // --- apply_deposit / apply_withdrawal aggregate-method tests ---
    // These cover the new aggregate-level entry points directly. CSH-021/CSH-031
    // (AmountNotPositive) cases stay in the record_* wrapper tests since that
    // framing lives in the wrapper, not in apply_*.

    // CSH-022 — apply_deposit pushes to history, queues TransactionInserted,
    // and replays the cash holding (lazy-creates per CSH-012).
    #[test]
    fn apply_deposit_pushes_tx_and_replays_cash_holding() {
        let mut acc = base_account();
        let tx = Transaction::new_deposit(
            acc.id.clone(),
            acc.cash_asset_id(),
            "2020-01-01".to_string(),
            micro(500),
            None,
        )
        .unwrap();
        let returned = acc.apply_deposit(tx.clone()).unwrap();
        assert_eq!(returned.id, tx.id);
        assert_eq!(acc.transactions.len(), 1);
        assert_eq!(acc.cash_holding_quantity(), micro(500));
        assert!(acc
            .pending_changes
            .iter()
            .any(|c| matches!(c, AccountChange::TransactionInserted(_))));
    }

    // CSH-080 — apply_withdrawal rejects when current cash balance is below the
    // requested amount, and the rejected transaction is NOT left in
    // self.transactions (eligibility runs before any mutation).
    #[test]
    fn apply_withdrawal_rejects_when_insufficient_cash() {
        let mut acc = base_account();
        // No deposit → cash balance is 0.
        let tx = Transaction::new_withdrawal(
            acc.id.clone(),
            acc.cash_asset_id(),
            "2020-01-01".to_string(),
            micro(100),
            None,
        )
        .unwrap();
        let err = acc.apply_withdrawal(tx).unwrap_err();
        assert!(
            matches!(
                err,
                AccountOperationError::InsufficientCash {
                    current_balance_micros: 0,
                    ..
                }
            ),
            "expected InsufficientCash{{0,…}}, got: {err:?}"
        );
        assert!(acc.transactions.is_empty(), "rejected tx must not be kept");
    }

    // CSH-080 — apply_withdrawal succeeds when balance >= requested amount and
    // the new running balance is reflected by the cash holding.
    #[test]
    fn apply_withdrawal_succeeds_when_balance_sufficient() {
        let mut acc = cash_seeded_account();
        let before = acc.cash_holding_quantity();
        let tx = Transaction::new_withdrawal(
            acc.id.clone(),
            acc.cash_asset_id(),
            "2020-02-01".to_string(),
            micro(200),
            None,
        )
        .unwrap();
        acc.apply_withdrawal(tx).unwrap();
        assert_eq!(acc.cash_holding_quantity(), before - micro(200));
    }
}
