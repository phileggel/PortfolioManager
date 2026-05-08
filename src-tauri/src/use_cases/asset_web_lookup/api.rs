//! Tauri command handler for asset web lookup (WEB-020).
// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use serde::Serialize;
use specta::Type;

use super::orchestrator::AssetWebLookupUseCase;
use super::primary_listing_processor::AssetLookupResult;
use crate::core::logger::BACKEND;

/// Typed error for `lookup_asset` (WEB-025).
///
/// Single variant — covers all failure modes: network unreachable, connection
/// timeout, and any non-2xx HTTP status (including rate-limiting responses).
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum WebLookupCommandError {
    /// All network or HTTP-level failures.
    #[error("Network error while contacting the lookup service")]
    NetworkError,
}

/// Searches OpenFIGI for instruments matching the query and returns up to 10
/// results (WEB-020, WEB-022).
///
/// Routing is transparent to the caller: 12-char alphanumeric queries are sent
/// to the ISIN mapping endpoint; all others to the keyword search endpoint
/// (WEB-014).  Any network or HTTP failure is returned as
/// `WebLookupCommandError::NetworkError` (WEB-025).
#[tauri::command]
#[specta::specta]
pub async fn lookup_asset(
    uc: tauri::State<'_, AssetWebLookupUseCase>,
    query: String,
) -> Result<Vec<AssetLookupResult>, WebLookupCommandError> {
    uc.search(query).await.map_err(|e| {
        tracing::warn!(target: BACKEND, error = %e, "lookup_asset failed (WEB-025)");
        WebLookupCommandError::NetworkError
    })
}
