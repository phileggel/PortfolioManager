// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::domain::{Account, UpdateFrequency};
use crate::context::account::{AccountApplicationError, AccountCrudError, Transaction};
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
#[tauri::command]
#[specta::specta]
pub async fn get_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<Account>, AccountApplicationError> {
    state.account_service.get_all().await
}

/// Adds a new account.
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

/// Deletes an account (R5 — cascades to its holdings at the repo level).
#[tauri::command]
#[specta::specta]
pub async fn delete_account(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AccountApplicationError> {
    state.account_service.delete(&id).await
}

/// Returns the distinct asset IDs that have transactions for the given account (TXL-013).
#[tauri::command]
#[specta::specta]
pub async fn get_asset_ids_for_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<String>, AccountApplicationError> {
    state
        .account_service
        .get_asset_ids_for_account(&account_id)
        .await
}

/// Retrieves all transactions for an account/asset pair (TRX-036).
#[tauri::command]
#[specta::specta]
pub async fn get_transactions(
    state: State<'_, AppState>,
    account_id: String,
    asset_id: String,
) -> Result<Vec<Transaction>, AccountApplicationError> {
    state
        .account_service
        .get_transactions(&account_id, &asset_id)
        .await
}
