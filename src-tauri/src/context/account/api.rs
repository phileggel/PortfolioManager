// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::domain::{Account, UpdateFrequency};
use crate::context::account::{AccountApplicationError, AccountDomainError, Transaction};
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
    if let Some(err) = e.downcast_ref::<AccountApplicationError>() {
        return match err {
            AccountApplicationError::NameAlreadyExists => AccountCommandError::NameAlreadyExists,
            // AccountNotFound cannot fire here: create/update never load an aggregate via
            // get_with_holdings_and_transactions — the ID is only used for name-uniqueness checks.
            AccountApplicationError::AccountNotFound { .. } => {
                tracing::error!(target: BACKEND, err = ?err, "BUG: AccountNotFound in account command");
                AccountCommandError::Unknown
            }
        };
    }
    if let Some(err) = e.downcast_ref::<AccountDomainError>() {
        return match err {
            AccountDomainError::NameEmpty => AccountCommandError::NameEmpty,
            AccountDomainError::InvalidCurrency(_) => AccountCommandError::InvalidCurrency,
        };
    }
    tracing::error!(target: BACKEND, err = ?e, "unexpected error in account command");
    AccountCommandError::Unknown
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

/// Retrieves all transactions for an account/asset pair (TRX-036).
///
/// Read-only — only infrastructure failures (DB / repository) can fire here,
/// so the surface is a single `InfrastructureError`. The wider
/// `HoldingTransactionError` composite is reserved for write commands.
#[tauri::command]
#[specta::specta]
pub async fn get_transactions(
    state: State<'_, AppState>,
    account_id: String,
    asset_id: String,
) -> Result<Vec<Transaction>, crate::core::InfrastructureError> {
    state
        .account_service
        .get_transactions(&account_id, &asset_id)
        .await
        .map_err(|e| {
            tracing::error!(target: BACKEND, err = ?e, "unexpected error in get_transactions");
            crate::core::InfrastructureError::Unknown {
                hint: format!("get_transactions: {e:#}"),
            }
        })
}
