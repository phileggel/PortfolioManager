use super::transaction_error::TransactionDomainError;
use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Type of financial transaction.
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
pub enum TransactionType {
    /// A purchase (acquisition) of an asset.
    #[default]
    Purchase,
    /// A sale of a previously purchased asset.
    Sell,
    /// Seeds a holding directly from a known quantity and total cost, without full transaction history (TRX-042).
    OpeningBalance,
    /// A cash inflow from outside the application's tracked world (CSH-022).
    Deposit,
    /// A cash outflow to outside the application's tracked world (CSH-032).
    Withdrawal,
}

/// A single financial event affecting an asset's quantity and cost basis within an account.
/// All financial fields are stored as i64 micro-units (ADR-001, TRX-024).
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Transaction {
    /// Unique identifier.
    pub id: String,
    /// The account where the transaction occurred.
    pub account_id: String,
    /// The financial asset involved.
    pub asset_id: String,
    /// Type of transaction: Purchase, Sell, or OpeningBalance.
    pub transaction_type: TransactionType,
    /// Date when the transaction was executed (ISO 8601, "YYYY-MM-DD").
    pub date: String,
    /// Number of units traded (micro-units: value × 10^6). Must be > 0.
    pub quantity: i64,
    /// Price per unit in asset's native currency (micro-units). Can be 0 (gifted assets).
    pub unit_price: i64,
    /// Conversion rate from asset currency to account currency (micro-units).
    pub exchange_rate: i64,
    /// Transaction fees in account currency (micro-units).
    pub fees: i64,
    /// Total cost (Purchase) or proceeds (Sell) in account currency (micro-units). Must be > 0.
    pub total_amount: i64,
    /// Optional user comment.
    pub note: Option<String>,
    /// Realized P&L for Sell transactions (micro-units, SEL-024). NULL for Purchase.
    pub realized_pnl: Option<i64>,
    /// ISO 8601 timestamp of record creation — used for same-date tie-breaking (SEL-024).
    pub created_at: String,
}

impl Transaction {
    /// Creates a new Transaction with a generated ID.
    /// Validates TRX-020 and TRX-026. Returns a typed `TransactionDomainError`
    /// so callers can propagate it through typed unions (e.g. the application-
    /// layer `CashRecordingError` composed by `AccountService::record_deposit`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account_id: String,
        asset_id: String,
        transaction_type: TransactionType,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        total_amount: i64,
        note: Option<String>,
        realized_pnl: Option<i64>,
    ) -> std::result::Result<Self, TransactionDomainError> {
        Self::validate(
            &date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
        )?;
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            account_id,
            asset_id,
            transaction_type,
            date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            note,
            realized_pnl,
            // Microsecond precision keeps the ORDER BY tiebreaker deterministic even when
            // multiple Transactions land within the same second — important for the
            // chronological cash replay (CSH-080).
            created_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                .to_string(),
        })
    }

    /// Creates a Transaction with a provided ID (used for updates, TRX-033).
    /// Applies the same validation as new().
    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        id: String,
        account_id: String,
        asset_id: String,
        transaction_type: TransactionType,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        total_amount: i64,
        note: Option<String>,
        realized_pnl: Option<i64>,
        created_at: String,
    ) -> std::result::Result<Self, TransactionDomainError> {
        Self::validate(
            &date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
        )?;
        Ok(Self {
            id,
            account_id,
            asset_id,
            transaction_type,
            date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            note,
            realized_pnl,
            created_at,
        })
    }

    /// Factory: builds a Deposit transaction with cash-specific defaults
    /// (`unit_price = 1.0` micros, `exchange_rate = 1.0` micros, `fees = 0`,
    /// `total_amount = amount`).
    ///
    /// CSH-021 — `amount <= 0` is rejected here as `AmountNotPositive` so the
    /// FE sees the cash-specific error code. The check fires BEFORE the
    /// generic `Transaction::new` validator (which would otherwise raise the
    /// less-specific `QuantityNotPositive`).
    pub fn new_deposit(
        account_id: String,
        cash_asset_id: String,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> std::result::Result<Self, TransactionDomainError> {
        if amount <= 0 {
            return Err(TransactionDomainError::AmountNotPositive);
        }
        Self::new(
            account_id,
            cash_asset_id,
            TransactionType::Deposit,
            date,
            amount,
            1_000_000,
            1_000_000,
            0,
            amount,
            note,
            None,
        )
    }

    /// Factory: builds a Withdrawal transaction with cash-specific defaults.
    /// Same shape as `new_deposit` but with `TransactionType::Withdrawal`.
    ///
    /// CSH-031 — `amount <= 0` is rejected here as `AmountNotPositive`
    /// (mirrors `new_deposit`). CSH-080 (insufficient cash) is enforced by
    /// `Account::apply_withdrawal`, not here — this factory only validates
    /// the transaction itself.
    pub fn new_withdrawal(
        account_id: String,
        cash_asset_id: String,
        date: String,
        amount: i64,
        note: Option<String>,
    ) -> std::result::Result<Self, TransactionDomainError> {
        if amount <= 0 {
            return Err(TransactionDomainError::AmountNotPositive);
        }
        Self::new(
            account_id,
            cash_asset_id,
            TransactionType::Withdrawal,
            date,
            amount,
            1_000_000,
            1_000_000,
            0,
            amount,
            note,
            None,
        )
    }

    /// Reconstructs a Transaction from storage without validation.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: String,
        account_id: String,
        asset_id: String,
        transaction_type: TransactionType,
        date: String,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        total_amount: i64,
        note: Option<String>,
        realized_pnl: Option<i64>,
        created_at: String,
    ) -> Self {
        Self {
            id,
            account_id,
            asset_id,
            transaction_type,
            date,
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            note,
            realized_pnl,
            created_at,
        }
    }

    /// Validates business rules (TRX-020).
    /// total_amount is computed by the orchestrator (TRX-026) before this is called —
    /// no formula check here.
    fn validate(
        date: &str,
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        total_amount: i64,
    ) -> std::result::Result<(), TransactionDomainError> {
        // TRX-020 — date must be parseable, not in the future, not before 1900-01-01
        let parsed_date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| TransactionDomainError::InvalidDate)?;
        let today = chrono::Local::now().date_naive();
        if parsed_date > today {
            return Err(TransactionDomainError::DateInFuture);
        }
        let min_date = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(chrono::NaiveDate::MIN);
        if parsed_date < min_date {
            return Err(TransactionDomainError::DateTooOld);
        }

        // TRX-020 — quantity must be strictly positive
        if quantity <= 0 {
            return Err(TransactionDomainError::QuantityNotPositive);
        }

        // TRX-020 — unit_price must be >= 0
        if unit_price < 0 {
            return Err(TransactionDomainError::UnitPriceNegative);
        }

        // SEL-020 — fees must be zero or positive
        if fees < 0 {
            return Err(TransactionDomainError::FeesNegative);
        }

        // TRX-020 — exchange_rate must be strictly positive
        if exchange_rate <= 0 {
            return Err(TransactionDomainError::ExchangeRateNotPositive);
        }

        // TRX-020 — total_amount must be > 0
        if total_amount <= 0 {
            return Err(TransactionDomainError::TotalAmountNotPositive);
        }

        Ok(())
    }
}

/// Interface for transaction persistence.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TransactionRepository: Send + Sync {
    /// Fetches a transaction by ID.
    async fn get_by_id(&self, id: &str) -> Result<Option<Transaction>>;
    /// Fetches all transactions for a given account and asset, ordered chronologically (TRX-036).
    async fn get_by_account_asset(
        &self,
        account_id: &str,
        asset_id: &str,
    ) -> Result<Vec<Transaction>>;
    /// Returns distinct asset IDs that have transactions for the given account (TXL-013).
    async fn get_asset_ids_for_account(&self, account_id: &str) -> Result<Vec<String>>;
    /// Returns sum of realized_pnl grouped by asset_id for Sell transactions in the account (SEL-038).
    async fn get_realized_pnl_by_account(&self, account_id: &str) -> Result<Vec<(String, i64)>>;
    /// Persists a new transaction.
    async fn create(&self, tx: Transaction) -> Result<Transaction>;
    /// Updates an existing transaction.
    async fn update(&self, tx: Transaction) -> Result<Transaction>;
    /// Deletes a transaction by ID.
    async fn delete(&self, id: &str) -> Result<()>;
    /// Returns true if any transaction references this asset (across all accounts).
    async fn has_transactions_for_asset(&self, asset_id: &str) -> Result<bool>;
    /// Counts all transactions for a given account (ACC-020).
    async fn count_by_account(&self, account_id: &str) -> Result<u32>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transaction(
        quantity: i64,
        unit_price: i64,
        exchange_rate: i64,
        fees: i64,
        total_amount: i64,
    ) -> std::result::Result<Transaction, TransactionDomainError> {
        Transaction::new(
            "account-1".to_string(),
            "asset-1".to_string(),
            TransactionType::Purchase,
            "2020-01-01".to_string(),
            quantity,
            unit_price,
            exchange_rate,
            fees,
            total_amount,
            None,
            None,
        )
    }

    // TRX-020 — quantity must be > 0
    #[test]
    fn rejects_zero_quantity() {
        let micro = 1_000_000i64;
        let result = make_transaction(0, micro, micro, 0, 0);
        assert!(result.is_err());
    }

    // TRX-020 — unit_price can be 0 (gifted assets, OQ-1)
    #[test]
    fn accepts_zero_unit_price() {
        // qty=1_000_000 (1.0), price=0, rate=1_000_000, fees=1_000_000 (1.0), total=1_000_000
        // expected = (1_000_000 * 0 / 1_000_000) * 1_000_000 / 1_000_000 + 1_000_000 = 1_000_000 ✓
        let micro = 1_000_000i64;
        let result = make_transaction(micro, 0, micro, micro, micro);
        assert!(result.is_ok(), "got: {:?}", result.err());
    }

    // TRX-020 — date before 1900-01-01 is rejected
    #[test]
    fn rejects_date_before_1900() {
        let micro = 1_000_000i64;
        let result = Transaction::new(
            "a".to_string(),
            "b".to_string(),
            TransactionType::Purchase,
            "1899-12-31".to_string(),
            micro,
            micro,
            micro,
            0,
            micro,
            None,
            None,
        );
        assert!(result.is_err());
    }

    // TRX-020 — future date is rejected
    #[test]
    fn rejects_future_date() {
        let micro = 1_000_000i64;
        let result = Transaction::new(
            "a".to_string(),
            "b".to_string(),
            TransactionType::Purchase,
            "2099-01-01".to_string(),
            micro,
            micro,
            micro,
            0,
            micro,
            None,
            None,
        );
        assert!(result.is_err());
    }

    // TRX-020 — exchange_rate must be strictly positive
    #[test]
    fn rejects_zero_exchange_rate() {
        let micro = 1_000_000i64;
        // total_amount=0 also fails (TRX-020) but exchange_rate=0 is caught first
        let result = make_transaction(micro, micro, 0, 0, 0);
        assert!(result.is_err());
    }

    // SEL-020 — fees cannot be negative
    #[test]
    fn rejects_negative_fees() {
        let micro = 1_000_000i64;
        let result = make_transaction(micro, micro, micro, -1, micro);
        assert!(result.is_err());
    }

    // TRX-042 — OpeningBalance variant exists in the enum
    #[test]
    fn opening_balance_variant_exists() {
        let tx_type = TransactionType::OpeningBalance;
        assert_eq!(tx_type, TransactionType::OpeningBalance);
    }

    // TRX-042 — OpeningBalance round-trips through strum Display → from_str
    #[test]
    fn opening_balance_round_trips_through_strum() {
        use std::str::FromStr;
        let original = TransactionType::OpeningBalance;
        let as_str = original.to_string();
        let parsed = TransactionType::from_str(&as_str).expect("strum parse");
        assert_eq!(parsed, TransactionType::OpeningBalance);
    }

    // TRX-042 — all three variants are distinct
    #[test]
    fn transaction_type_variants_are_distinct() {
        assert_ne!(TransactionType::Purchase, TransactionType::Sell);
        assert_ne!(TransactionType::Purchase, TransactionType::OpeningBalance);
        assert_ne!(TransactionType::Sell, TransactionType::OpeningBalance);
    }

    // CSH-022 — Transaction::new_deposit sets the cash-specific defaults
    // (price=1.0 micros, rate=1.0 micros, fees=0, total=amount, type=Deposit).
    #[test]
    fn new_deposit_sets_cash_defaults() {
        let tx = Transaction::new_deposit(
            "acc-1".to_string(),
            "asset-cash-USD".to_string(),
            "2020-01-01".to_string(),
            500_000_000,
            None,
        )
        .unwrap();
        assert_eq!(tx.transaction_type, TransactionType::Deposit);
        assert_eq!(tx.account_id, "acc-1");
        assert_eq!(tx.asset_id, "asset-cash-USD");
        assert_eq!(tx.quantity, 500_000_000);
        assert_eq!(tx.unit_price, 1_000_000);
        assert_eq!(tx.exchange_rate, 1_000_000);
        assert_eq!(tx.fees, 0);
        assert_eq!(tx.total_amount, 500_000_000);
        assert!(tx.realized_pnl.is_none());
    }

    // CSH-021 — zero amount surfaces the cash-specific `AmountNotPositive`
    // (raised by `new_deposit`'s own check) before the generic
    // `QuantityNotPositive` from `Transaction::new` could fire.
    #[test]
    fn new_deposit_rejects_zero_amount_as_amount_not_positive() {
        let err = Transaction::new_deposit(
            "acc-1".to_string(),
            "asset-cash-USD".to_string(),
            "2020-01-01".to_string(),
            0,
            None,
        )
        .unwrap_err();
        assert!(matches!(err, TransactionDomainError::AmountNotPositive));
    }

    // CSH-032 — Transaction::new_withdrawal sets the cash-specific defaults
    // with TransactionType::Withdrawal. CSH-080 is enforced by the aggregate
    // (Account::apply_withdrawal), not by this factory.
    #[test]
    fn new_withdrawal_sets_cash_defaults() {
        let tx = Transaction::new_withdrawal(
            "acc-1".to_string(),
            "asset-cash-USD".to_string(),
            "2020-01-01".to_string(),
            250_000_000,
            Some("ATM".to_string()),
        )
        .unwrap();
        assert_eq!(tx.transaction_type, TransactionType::Withdrawal);
        assert_eq!(tx.quantity, 250_000_000);
        assert_eq!(tx.unit_price, 1_000_000);
        assert_eq!(tx.exchange_rate, 1_000_000);
        assert_eq!(tx.fees, 0);
        assert_eq!(tx.total_amount, 250_000_000);
        assert_eq!(tx.note.as_deref(), Some("ATM"));
    }

    // TRX-020 propagates through new_withdrawal unchanged.
    #[test]
    fn new_withdrawal_propagates_date_validation() {
        let err = Transaction::new_withdrawal(
            "acc-1".to_string(),
            "asset-cash-USD".to_string(),
            "1899-12-31".to_string(),
            100,
            None,
        )
        .unwrap_err();
        assert!(matches!(err, TransactionDomainError::DateTooOld));
    }
}
