//! Tauri command handler for asset web lookup (WEB-020).
// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::error::WebLookupApplicationError;
use super::orchestrator::AssetWebLookupUseCase;
use super::primary_listing_processor::AssetLookupResult;

/// Searches OpenFIGI for instruments matching the query and returns up to 10
/// results (WEB-020, WEB-022).
///
/// Routing is transparent to the caller: 12-char alphanumeric queries are sent
/// to the ISIN mapping endpoint; all others to the keyword search endpoint
/// (WEB-014). HTTP 429 surfaces as `WebLookupApplicationError::RateLimited`;
/// every other failure surfaces as `WebLookupApplicationError::NetworkError`
/// (WEB-025).
#[tauri::command]
#[specta::specta]
pub async fn lookup_asset(
    uc: tauri::State<'_, AssetWebLookupUseCase>,
    query: String,
) -> Result<Vec<AssetLookupResult>, WebLookupApplicationError> {
    uc.search(query).await
}
