// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::domain::{Account, UpdateFrequency};
use crate::context::account::{
    AccountDomainError, AccountOperationError, Transaction, TransactionDomainError,
};
use crate::core::logger::BACKEND;
use crate::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

// --- DTOs ---

/// Parameters for creating a new account.
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct CreateAccountDTO {
    /// Display name.
    pub name: String,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Update frequency.
    pub update_frequency: UpdateFrequency,
}

/// Parameters for updating an existing account.
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct UpdateAccountDTO {
    /// Target account ID.
    pub id: String,
    /// New display name.
    pub name: String,
    /// ISO 4217 currency code.
    pub currency: String,
    /// New update frequency.
    pub update_frequency: UpdateFrequency,
}

// --- Boundary error ---

/// Typed error returned to the frontend for account commands.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum AccountCommandError {
    /// Account name is empty or whitespace-only.
    #[error("Account name cannot be empty")]
    NameEmpty,
    /// An account with the same name already exists.
    #[error("An account with this name already exists")]
    NameAlreadyExists,
    /// The currency string is not a valid ISO 4217 code.
    #[error("Invalid currency code")]
    InvalidCurrency,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_account_error(e: anyhow::Error) -> AccountCommandError {
    if let Some(err) = e.downcast_ref::<AccountDomainError>() {
        match err {
            AccountDomainError::NameEmpty => AccountCommandError::NameEmpty,
            AccountDomainError::NameAlreadyExists => AccountCommandError::NameAlreadyExists,
            AccountDomainError::InvalidCurrency(_) => AccountCommandError::InvalidCurrency,
            // AccountNotFound cannot fire here: create/update never loads an aggregate via
            // get_with_holdings_and_transactions — the ID is only used for name-uniqueness checks.
            AccountDomainError::AccountNotFound(_) => {
                tracing::error!(target: BACKEND, err = ?err, "BUG: AccountNotFound in account command");
                AccountCommandError::Unknown
            }
        }
    } else {
        tracing::error!(target: BACKEND, err = ?e, "unexpected error in account command");
        AccountCommandError::Unknown
    }
}

// --- Commands ---

/// Retrieves all accounts.
#[tauri::command]
#[specta::specta]
pub async fn get_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, AccountCommandError> {
    state
        .account_service
        .get_all()
        .await
        .map_err(to_account_error)
}

/// Adds a new account.
#[tauri::command]
#[specta::specta]
pub async fn add_account(
    state: State<'_, AppState>,
    dto: CreateAccountDTO,
) -> Result<Account, AccountCommandError> {
    state
        .account_service
        .create(dto.name, dto.currency, dto.update_frequency)
        .await
        .map_err(to_account_error)
}

/// Updates an existing account.
#[tauri::command]
#[specta::specta]
pub async fn update_account(
    state: State<'_, AppState>,
    dto: UpdateAccountDTO,
) -> Result<Account, AccountCommandError> {
    state
        .account_service
        .update(dto.id, dto.name, dto.currency, dto.update_frequency)
        .await
        .map_err(to_account_error)
}

/// Deletes an account.
#[tauri::command]
#[specta::specta]
pub async fn delete_account(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AccountCommandError> {
    state
        .account_service
        .delete(&id)
        .await
        .map_err(to_account_error)
}

/// Returns the distinct asset IDs that have transactions for the given account (TXL-013).
#[tauri::command]
#[specta::specta]
pub async fn get_asset_ids_for_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<String>, AccountCommandError> {
    state
        .account_service
        .get_asset_ids_for_account(&account_id)
        .await
        .map_err(|e| {
            tracing::error!(err = ?e, "unexpected error in get_asset_ids_for_account");
            AccountCommandError::Unknown
        })
}

// Shared with `use_cases/holding_transaction/api.rs` (its commands map to this enum too).

/// Typed error returned to the frontend for holding operation commands.
#[derive(Debug, serde::Serialize, specta::Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum TransactionCommandError {
    /// No transaction exists with the requested ID.
    #[error("Transaction not found")]
    TransactionNotFound,
    /// No account exists with the requested ID. Wire shape mirrors
    /// `AccountApplicationError::AccountNotFound` and
    /// `OpenHoldingCommandError::AccountNotFound` for FE consistency across
    /// every cash and non-cash command (per PR 2b's tightening).
    #[error("Account not found: {account_id}")]
    AccountNotFound {
        /// The ID the caller asked for.
        account_id: String,
    },
    /// Sell requested but holding has zero available units.
    #[error("No units available to sell")]
    ClosedPosition,
    /// Sell quantity exceeds currently held units.
    #[error("Oversell: requested exceeds available")]
    Oversell {
        /// Units currently held before the sale.
        available: i64,
        /// Units the transaction attempts to sell.
        requested: i64,
    },
    /// Editing would leave a later transaction with insufficient units.
    #[error("Editing this transaction would create a cascading oversell")]
    CascadingOversell,
    /// Cash debit (or replay step) would drive the cash holding strictly negative (CSH-080).
    #[error("Insufficient cash: current balance {current_balance_micros} {currency}")]
    InsufficientCash {
        /// Cash holding's running balance at the point of rejection (micro-units, account currency).
        current_balance_micros: i64,
        /// ISO 4217 currency code of the offending account's cash holding.
        currency: String,
    },
    /// Date string could not be parsed as YYYY-MM-DD.
    #[error("Invalid date format — expected YYYY-MM-DD")]
    InvalidDate,
    /// Transaction date is in the future.
    #[error("Transaction date cannot be in the future")]
    DateInFuture,
    /// Transaction date is before 1900-01-01.
    #[error("Transaction date cannot be before 1900-01-01")]
    DateTooOld,
    /// Quantity is zero or negative.
    #[error("Quantity must be strictly positive")]
    QuantityNotPositive,
    /// Unit price is negative.
    #[error("Unit price cannot be negative")]
    UnitPriceNegative,
    /// Fees amount is negative.
    #[error("Fees cannot be negative")]
    FeesNegative,
    /// Exchange rate is zero or negative.
    #[error("Exchange rate must be strictly positive")]
    ExchangeRateNotPositive,
    /// Total amount is zero or negative.
    #[error("Total amount must be strictly positive")]
    TotalAmountNotPositive,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

/// Maps an `anyhow::Error` raised by `AccountService` holding/transaction operations
/// to the `TransactionCommandError` variant exposed at the Tauri boundary.
///
/// Visible to `use_cases/holding_transaction/api.rs` so its command handlers can share
/// the same translation logic; unrecognised errors are logged and reported as `Unknown`.
pub(crate) fn to_transaction_error(e: anyhow::Error) -> TransactionCommandError {
    if let Some(err) = e.downcast_ref::<AccountOperationError>() {
        return match err {
            AccountOperationError::ClosedPosition => TransactionCommandError::ClosedPosition,
            AccountOperationError::Oversell {
                available,
                requested,
            } => TransactionCommandError::Oversell {
                available: *available,
                requested: *requested,
            },
            AccountOperationError::CascadingOversell => TransactionCommandError::CascadingOversell,
            AccountOperationError::TransactionNotFound => {
                TransactionCommandError::TransactionNotFound
            }
            AccountOperationError::InsufficientCash {
                current_balance_micros,
                currency,
            } => TransactionCommandError::InsufficientCash {
                current_balance_micros: *current_balance_micros,
                currency: currency.clone(),
            },
        };
    }
    if let Some(err) = e.downcast_ref::<TransactionDomainError>() {
        return match err {
            TransactionDomainError::InvalidDate => TransactionCommandError::InvalidDate,
            TransactionDomainError::DateInFuture => TransactionCommandError::DateInFuture,
            TransactionDomainError::DateTooOld => TransactionCommandError::DateTooOld,
            TransactionDomainError::QuantityNotPositive => {
                TransactionCommandError::QuantityNotPositive
            }
            TransactionDomainError::UnitPriceNegative => TransactionCommandError::UnitPriceNegative,
            TransactionDomainError::FeesNegative => TransactionCommandError::FeesNegative,
            TransactionDomainError::ExchangeRateNotPositive => {
                TransactionCommandError::ExchangeRateNotPositive
            }
            TransactionDomainError::TotalAmountNotPositive => {
                TransactionCommandError::TotalAmountNotPositive
            }
            TransactionDomainError::AmountNotPositive => {
                // CSH-021/CSH-031 — AmountNotPositive is raised by the cash
                // factories (`Transaction::new_deposit` / `new_withdrawal`),
                // not by `Transaction::new` used by buy/sell/correct/cancel.
                // Should never fire from non-cash command paths; log and
                // surface as Unknown if it ever does.
                tracing::error!(target: BACKEND, "BUG: AmountNotPositive raised from non-cash command");
                TransactionCommandError::Unknown
            }
        };
    }
    if let Some(err) = e.downcast_ref::<AccountDomainError>() {
        return match err {
            AccountDomainError::AccountNotFound(account_id) => {
                TransactionCommandError::AccountNotFound {
                    account_id: account_id.clone(),
                }
            }
            // NameEmpty / NameAlreadyExists / InvalidCurrency cannot fire from aggregate-write
            // operations — they belong to account create/update paths only.
            _ => {
                tracing::error!(target: BACKEND, err = ?err, "BUG: unexpected AccountDomainError in transaction command");
                TransactionCommandError::Unknown
            }
        };
    }
    tracing::error!(target: BACKEND, err = ?e, "unexpected error in transaction command");
    TransactionCommandError::Unknown
}

/// Retrieves all transactions for an account/asset pair (TRX-036).
#[tauri::command]
#[specta::specta]
pub async fn get_transactions(
    state: State<'_, AppState>,
    account_id: String,
    asset_id: String,
) -> Result<Vec<Transaction>, TransactionCommandError> {
    state
        .account_service
        .get_transactions(&account_id, &asset_id)
        .await
        .map_err(to_transaction_error)
}
