// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::domain::{Account, UpdateFrequency};
use crate::context::account::{AccountCrudError, Transaction};
use crate::core::logger::BACKEND;
use crate::core::InfrastructureError;
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

// --- Commands ---

/// Retrieves all accounts.
///
/// Read-only — only infrastructure failures (DB / repository) can fire here,
/// so the surface is the narrow shared `InfrastructureError`.
#[tauri::command]
#[specta::specta]
pub async fn get_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, InfrastructureError> {
    state.account_service.get_all().await
}

/// Adds a new account.
///
/// Returns the typed `AccountCrudError` directly — no boundary type or mapper
/// is needed because every leaf in the composite (`AccountApplicationError`,
/// `AccountDomainError`, shared `InfrastructureError`) already serializes with
/// `#[serde(tag = "code")]`, and `AccountCrudError`'s `#[serde(untagged)]`
/// flattens them into a single FE-visible union.
#[tauri::command]
#[specta::specta]
pub async fn add_account(
    state: State<'_, AppState>,
    dto: CreateAccountDTO,
) -> Result<Account, AccountCrudError> {
    state
        .account_service
        .create(dto.name, dto.currency, dto.update_frequency)
        .await
}

/// Updates an existing account.
#[tauri::command]
#[specta::specta]
pub async fn update_account(
    state: State<'_, AppState>,
    dto: UpdateAccountDTO,
) -> Result<Account, AccountCrudError> {
    state
        .account_service
        .update(dto.id, dto.name, dto.currency, dto.update_frequency)
        .await
}

/// Deletes an account.
///
/// Pure infrastructure surface — no domain rejections (cascade is repo-level).
#[tauri::command]
#[specta::specta]
pub async fn delete_account(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), InfrastructureError> {
    state.account_service.delete(&id).await
}

/// Returns the distinct asset IDs that have transactions for the given account (TXL-013).
///
/// Read-only — only infrastructure failures can fire here.
#[tauri::command]
#[specta::specta]
pub async fn get_asset_ids_for_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<String>, InfrastructureError> {
    state
        .account_service
        .get_asset_ids_for_account(&account_id)
        .await
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
