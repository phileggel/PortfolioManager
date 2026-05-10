// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::error::AccountDetailsError;
use super::orchestrator::{AccountDetailsResponse, AccountDetailsUseCase};
use tauri::State;

// --- Boundary error ---

/// Typed error returned to the frontend for the get_account_details command.
#[derive(Debug, serde::Serialize, specta::Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum AccountDetailsCommandError {
    /// No account exists with the requested ID.
    ///
    /// Intentionally a unit variant — unlike the write-path counterpart
    /// `AccountApplicationError::AccountNotFound` which carries
    /// `{ account_id: String }`, this is a read command: the caller already
    /// supplied `account_id` as the query parameter, so echoing it back adds
    /// no diagnostic value. Do not cargo-cult-add the field.
    #[error("Account not found")]
    AccountNotFound,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_account_details_error(e: anyhow::Error) -> AccountDetailsCommandError {
    if let Some(err) = e.downcast_ref::<AccountDetailsError>() {
        match err {
            AccountDetailsError::AccountNotFound => AccountDetailsCommandError::AccountNotFound,
        }
    } else {
        tracing::error!(err = ?e, "unexpected error in get_account_details command");
        AccountDetailsCommandError::Unknown
    }
}

// --- Command ---

/// Returns the full account details view for the given account (ACD-012 to ACD-041).
#[tauri::command]
#[specta::specta]
pub async fn get_account_details(
    state: State<'_, AccountDetailsUseCase>,
    account_id: String,
) -> Result<AccountDetailsResponse, AccountDetailsCommandError> {
    state
        .get_account_details(&account_id)
        .await
        .map_err(to_account_details_error)
}
