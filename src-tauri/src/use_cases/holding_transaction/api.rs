// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::HoldingTransactionUseCase;
use crate::context::account::{
    AccountApplicationError, HoldingTransactionError, OpeningBalanceDomainError, Transaction,
    TransactionDomainError,
};
use crate::core::logger::BACKEND;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

// =============================================================================
// Opening Balance — DTO + dedicated error
// =============================================================================

/// Parameters for recording an opening balance for an asset in an account (TRX-042).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OpenHoldingDTO {
    /// Account where the opening balance is recorded.
    pub account_id: String,
    /// Financial asset being seeded.
    pub asset_id: String,
    /// Date of the opening balance (YYYY-MM-DD).
    pub date: String,
    /// Quantity in micro-units; strictly positive (TRX-044).
    pub quantity: i64,
    /// Total cost paid in account currency (micro-units); strictly positive (TRX-045).
    pub total_cost: i64,
}

/// Typed error returned to the frontend for the open_holding command.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum OpenHoldingCommandError {
    /// No account exists with the requested ID. Wire shape mirrors
    /// `AccountApplicationError::AccountNotFound` for FE consistency across
    /// every cash and non-cash command (per PR 2b's tightening).
    #[error("Account not found: {account_id}")]
    AccountNotFound {
        /// The ID the caller asked for.
        account_id: String,
    },
    /// No asset exists with the requested ID.
    #[error("Asset not found")]
    AssetNotFound,
    /// Asset is archived — cannot open a holding (TRX-050).
    #[error("Cannot open a holding for an archived asset")]
    ArchivedAsset,
    /// Target asset is a system Cash Asset — record initial cash via `record_deposit` (CSH-061).
    #[error("Opening balance cannot be recorded against a cash asset; use record_deposit instead")]
    OpeningBalanceOnCashAsset,
    /// Total cost is zero or negative (TRX-045).
    #[error("Total cost must be strictly positive")]
    InvalidTotalCost,
    /// Quantity is zero or negative (TRX-044).
    #[error("Quantity must be strictly positive")]
    QuantityNotPositive,
    /// Date string could not be parsed as YYYY-MM-DD.
    #[error("Invalid date format — expected YYYY-MM-DD")]
    InvalidDate,
    /// Transaction date is in the future.
    #[error("Transaction date cannot be in the future")]
    DateInFuture,
    /// Transaction date is before 1900-01-01.
    #[error("Transaction date cannot be before 1900-01-01")]
    DateTooOld,
    /// An unexpected server-side error occurred. `hint` carries a developer-only
    /// diagnostic string mirroring the `tracing::error!` log so support reports
    /// can be triaged without correlating timestamps.
    #[error("An unexpected error occurred ({hint})")]
    Unknown {
        /// Developer-only diagnostic string. Not user-facing; the FE displays
        /// the i18n key `error.Unknown` and forwards `hint` to the JS console
        /// log via `logger.error`.
        hint: String,
    },
}

fn to_open_holding_error(e: anyhow::Error) -> OpenHoldingCommandError {
    if let Some(err) = e.downcast_ref::<OpeningBalanceDomainError>() {
        return match err {
            OpeningBalanceDomainError::InvalidTotalCost => {
                OpenHoldingCommandError::InvalidTotalCost
            }
            OpeningBalanceDomainError::AssetNotFound => OpenHoldingCommandError::AssetNotFound,
            OpeningBalanceDomainError::ArchivedAsset => OpenHoldingCommandError::ArchivedAsset,
            OpeningBalanceDomainError::OpeningBalanceOnCashAsset => {
                OpenHoldingCommandError::OpeningBalanceOnCashAsset
            }
        };
    }
    if let Some(err) = e.downcast_ref::<TransactionDomainError>() {
        return match err {
            TransactionDomainError::InvalidDate => OpenHoldingCommandError::InvalidDate,
            TransactionDomainError::DateInFuture => OpenHoldingCommandError::DateInFuture,
            TransactionDomainError::DateTooOld => OpenHoldingCommandError::DateTooOld,
            TransactionDomainError::QuantityNotPositive => {
                OpenHoldingCommandError::QuantityNotPositive
            }
            // These five variants require a user-supplied unit_price, fees,
            // exchange_rate, or are cash-factory-only (`AmountNotPositive`).
            // `open_holding` computes price/fees/rate as constants and uses
            // `Transaction::new` (not the cash factories), so none can fire
            // in practice. Enumerated explicitly (not wildcarded) so a future
            // regression triggers a compile error.
            TransactionDomainError::UnitPriceNegative
            | TransactionDomainError::FeesNegative
            | TransactionDomainError::ExchangeRateNotPositive
            | TransactionDomainError::TotalAmountNotPositive
            | TransactionDomainError::AmountNotPositive => {
                tracing::error!(target: BACKEND, err = ?err, "BUG: impossible TransactionDomainError in open_holding");
                OpenHoldingCommandError::Unknown {
                    hint: format!(
                        "BUG: impossible TransactionDomainError::{err:?} in open_holding"
                    ),
                }
            }
        };
    }
    if let Some(err) = e.downcast_ref::<AccountApplicationError>() {
        return match err {
            AccountApplicationError::AccountNotFound { account_id } => {
                OpenHoldingCommandError::AccountNotFound {
                    account_id: account_id.clone(),
                }
            }
            // NameAlreadyExists cannot fire from open_holding — it's raised
            // only by the create/update name-uniqueness pre-check.
            AccountApplicationError::NameAlreadyExists => {
                tracing::error!(target: BACKEND, err = ?err, "BUG: NameAlreadyExists in open_holding command");
                OpenHoldingCommandError::Unknown {
                    hint: "BUG: NameAlreadyExists in open_holding".to_string(),
                }
            }
        };
    }
    tracing::error!(target: BACKEND, err = ?e, "unexpected error in open_holding command");
    // {e:#} pretty-prints the full anyhow context chain on one line —
    // {e} (Display) would drop any upstream `.context(...)` wrappers.
    OpenHoldingCommandError::Unknown {
        hint: format!("unexpected error in open_holding: {e:#}"),
    }
}

// =============================================================================
// Buy / Sell / Correct — DTOs (shared HoldingTransactionError composite)
// =============================================================================

/// Parameters for recording a purchase of an asset into an account.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct BuyHoldingDTO {
    /// Account where the purchase is recorded.
    pub account_id: String,
    /// Financial asset being purchased.
    pub asset_id: String,
    /// Transaction date (YYYY-MM-DD).
    pub date: String,
    /// Quantity in micro-units.
    pub quantity: i64,
    /// Unit price in asset currency (micro-units).
    pub unit_price: i64,
    /// Exchange rate asset→account currency (micro-units).
    pub exchange_rate: i64,
    /// Fees in account currency (micro-units).
    pub fees: i64,
    /// Optional user note.
    pub note: Option<String>,
}

/// Parameters for recording a sale of an asset from an account.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SellHoldingDTO {
    /// Account where the sale is recorded.
    pub account_id: String,
    /// Financial asset being sold.
    pub asset_id: String,
    /// Transaction date (YYYY-MM-DD).
    pub date: String,
    /// Quantity in micro-units.
    pub quantity: i64,
    /// Unit price in asset currency (micro-units).
    pub unit_price: i64,
    /// Exchange rate asset→account currency (micro-units).
    pub exchange_rate: i64,
    /// Fees in account currency (micro-units).
    pub fees: i64,
    /// Optional user note.
    pub note: Option<String>,
}

/// Parameters for correcting an existing transaction.
/// `account_id` and `asset_id` are immutable — taken from the existing transaction.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CorrectTransactionDTO {
    /// Corrected transaction date (YYYY-MM-DD).
    pub date: String,
    /// Corrected quantity in micro-units.
    pub quantity: i64,
    /// Corrected unit price in asset currency (micro-units).
    pub unit_price: i64,
    /// Corrected exchange rate asset→account currency (micro-units).
    pub exchange_rate: i64,
    /// Corrected fees in account currency (micro-units).
    pub fees: i64,
    /// Optional user note.
    pub note: Option<String>,
}

// =============================================================================
// Commands
// =============================================================================

/// Seeds a holding directly from a known quantity and total cost (TRX-042, TRX-047).
#[tauri::command]
#[specta::specta]
pub async fn open_holding(
    uc: State<'_, HoldingTransactionUseCase>,
    dto: OpenHoldingDTO,
) -> Result<Transaction, OpenHoldingCommandError> {
    uc.open_holding(
        &dto.account_id,
        dto.asset_id,
        dto.date,
        dto.quantity,
        dto.total_cost,
    )
    .await
    .map_err(to_open_holding_error)
}

/// Records a purchase of an asset into an account (TRX-027).
#[tauri::command]
#[specta::specta]
pub async fn buy_holding(
    uc: State<'_, HoldingTransactionUseCase>,
    dto: BuyHoldingDTO,
) -> Result<Transaction, HoldingTransactionError> {
    uc.buy_holding(
        &dto.account_id,
        dto.asset_id,
        dto.date,
        dto.quantity,
        dto.unit_price,
        dto.exchange_rate,
        dto.fees,
        dto.note,
    )
    .await
}

/// Records a sale of an asset from an account (SEL-012, SEL-021, SEL-023, SEL-024).
#[tauri::command]
#[specta::specta]
pub async fn sell_holding(
    uc: State<'_, HoldingTransactionUseCase>,
    dto: SellHoldingDTO,
) -> Result<Transaction, HoldingTransactionError> {
    uc.sell_holding(
        &dto.account_id,
        dto.asset_id,
        dto.date,
        dto.quantity,
        dto.unit_price,
        dto.exchange_rate,
        dto.fees,
        dto.note,
    )
    .await
}

/// Corrects an existing transaction and recalculates the affected holding (TRX-031).
#[tauri::command]
#[specta::specta]
pub async fn correct_transaction(
    uc: State<'_, HoldingTransactionUseCase>,
    id: String,
    account_id: String,
    dto: CorrectTransactionDTO,
) -> Result<Transaction, HoldingTransactionError> {
    uc.correct_transaction(
        &account_id,
        &id,
        dto.date,
        dto.quantity,
        dto.unit_price,
        dto.exchange_rate,
        dto.fees,
        dto.note,
    )
    .await
}

/// Cancels a transaction and recalculates (or removes) the associated holding (TRX-034).
#[tauri::command]
#[specta::specta]
pub async fn cancel_transaction(
    uc: State<'_, HoldingTransactionUseCase>,
    id: String,
    account_id: String,
) -> Result<(), HoldingTransactionError> {
    uc.cancel_transaction(&account_id, &id).await
}

// =============================================================================
// Cash Transactions — DTOs + dedicated errors (CSH-022 / CSH-032)
// =============================================================================

/// Parameters for recording a cash deposit (CSH-020).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DepositDTO {
    /// Account receiving the cash.
    pub account_id: String,
    /// Transaction date (YYYY-MM-DD).
    pub date: String,
    /// Deposited amount in account currency (micro-units); strictly positive (CSH-021).
    pub amount_micros: i64,
    /// Optional user note.
    pub note: Option<String>,
}

/// Parameters for recording a cash withdrawal (CSH-030).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WithdrawalDTO {
    /// Account from which to withdraw cash.
    pub account_id: String,
    /// Transaction date (YYYY-MM-DD).
    pub date: String,
    /// Withdrawn amount in account currency (micro-units); strictly positive (CSH-031).
    pub amount_micros: i64,
    /// Optional user note.
    pub note: Option<String>,
}

/// Records a cash deposit into an account (CSH-022).
///
/// The Tauri command returns the typed `HoldingTransactionError` directly — no
/// boundary type or mapper is needed because every leaf in the composite
/// (`AccountApplicationError`, `AccountOperationError`, `TransactionDomainError`,
/// shared `InfrastructureError`) already serializes with `#[serde(tag = "code")]`,
/// and `HoldingTransactionError`'s `#[serde(untagged)]` flattens them into a
/// single FE-visible union.
#[tauri::command]
#[specta::specta]
pub async fn record_deposit(
    uc: State<'_, HoldingTransactionUseCase>,
    dto: DepositDTO,
) -> Result<Transaction, HoldingTransactionError> {
    uc.record_deposit(&dto.account_id, dto.date, dto.amount_micros, dto.note)
        .await
}

/// Records a cash withdrawal from an account (CSH-032).
#[tauri::command]
#[specta::specta]
pub async fn record_withdrawal(
    uc: State<'_, HoldingTransactionUseCase>,
    dto: WithdrawalDTO,
) -> Result<Transaction, HoldingTransactionError> {
    uc.record_withdrawal(&dto.account_id, dto.date, dto.amount_micros, dto.note)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    // PR 3 — to_open_holding_error covers every classified branch in one test:
    // the OpeningBalanceDomainError leaves, a TransactionDomainError leaf
    // (both possible-from-open_holding and impossible-from-open_holding cases),
    // the AccountApplicationError arms (AccountNotFound preserves payload;
    // NameAlreadyExists is the BUG-guard added in PR 3), and the unclassified
    // fallback. Mapper IS the FE wire contract.
    #[test]
    fn to_open_holding_error_maps_every_branch() {
        // OpeningBalanceDomainError leaves
        assert!(matches!(
            to_open_holding_error(OpeningBalanceDomainError::AssetNotFound.into()),
            OpenHoldingCommandError::AssetNotFound
        ));
        assert!(matches!(
            to_open_holding_error(OpeningBalanceDomainError::ArchivedAsset.into()),
            OpenHoldingCommandError::ArchivedAsset
        ));
        assert!(matches!(
            to_open_holding_error(OpeningBalanceDomainError::OpeningBalanceOnCashAsset.into()),
            OpenHoldingCommandError::OpeningBalanceOnCashAsset
        ));
        assert!(matches!(
            to_open_holding_error(OpeningBalanceDomainError::InvalidTotalCost.into()),
            OpenHoldingCommandError::InvalidTotalCost
        ));

        // TransactionDomainError — possible variant (DateInFuture)
        assert!(matches!(
            to_open_holding_error(TransactionDomainError::DateInFuture.into()),
            OpenHoldingCommandError::DateInFuture
        ));
        // TransactionDomainError — impossible-from-open_holding variant
        // (cash-factory-only) hits the BUG arm and surfaces as Unknown.
        assert!(matches!(
            to_open_holding_error(TransactionDomainError::AmountNotPositive.into()),
            OpenHoldingCommandError::Unknown { .. }
        ));

        // AccountApplicationError — AccountNotFound preserves the account_id payload
        match to_open_holding_error(
            AccountApplicationError::AccountNotFound {
                account_id: "acc-42".into(),
            }
            .into(),
        ) {
            OpenHoldingCommandError::AccountNotFound { account_id } => {
                assert_eq!(account_id, "acc-42");
            }
            other => panic!("expected AccountNotFound, got: {other:?}"),
        }
        // NameAlreadyExists is the BUG-guard added in PR 3 (impossible from open_holding).
        assert!(matches!(
            to_open_holding_error(AccountApplicationError::NameAlreadyExists.into()),
            OpenHoldingCommandError::Unknown { .. }
        ));

        // Unclassified fallback
        assert!(matches!(
            to_open_holding_error(anyhow!("synthetic infra failure")),
            OpenHoldingCommandError::Unknown { .. }
        ));
    }
}
