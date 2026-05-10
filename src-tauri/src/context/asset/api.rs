// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use crate::context::asset::domain::error::{AssetDomainError, AssetPriceDomainError};
use crate::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;

use tauri::State;

use super::domain::{Asset, AssetCategory, AssetClass, AssetPrice};

// --- DTOs ---

/// Parameters for creating a new asset.
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct CreateAssetDTO {
    /// Display name.
    pub name: String,
    /// Ticker, ISIN, or user-defined reference (mandatory — R1).
    pub reference: String,
    /// Classification type.
    pub class: AssetClass,
    /// ISO currency code.
    pub currency: String,
    /// 1-5 risk score.
    pub risk_level: u8,
    /// ID of the primary category.
    pub category_id: String,
}

/// Parameters for updating an existing asset.
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct UpdateAssetDTO {
    /// Target asset ID.
    pub asset_id: String,
    /// New display name.
    pub name: String,
    /// New reference (mandatory — R1).
    pub reference: String,
    /// New classification.
    pub class: AssetClass,
    /// New currency.
    pub currency: String,
    /// New risk level.
    pub risk_level: u8,
    /// New category link.
    pub category_id: String,
}

// --- Boundary errors ---

/// Typed error returned to the frontend for asset CRUD commands.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum AssetCommandError {
    /// Asset name is empty or whitespace-only.
    #[error("Asset name cannot be empty")]
    NameEmpty,
    /// Asset reference (ticker/ISIN) is empty.
    #[error("Asset reference cannot be empty")]
    ReferenceEmpty,
    /// Risk level is outside the 1–5 range.
    #[error("Risk level must be between 1 and 5")]
    InvalidRiskLevel,
    /// Currency string is not a valid ISO 4217 code.
    #[error("Invalid currency code")]
    InvalidCurrency,
    /// Asset is archived and cannot be edited.
    #[error("Cannot edit an archived asset")]
    Archived,
    /// Target asset is a system Cash Asset and cannot be edited or unarchived (CSH-016).
    #[error("Cannot edit a system Cash Asset")]
    CashAssetNotEditable,
    /// No asset exists with the requested ID.
    #[error("Asset not found")]
    NotFound,
    /// The category referenced in the DTO does not exist.
    #[error("Category not found")]
    CategoryNotFound,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_asset_error(e: anyhow::Error) -> AssetCommandError {
    if let Some(err) = e.downcast_ref::<AssetDomainError>() {
        return match err {
            AssetDomainError::NameEmpty => AssetCommandError::NameEmpty,
            AssetDomainError::ReferenceEmpty => AssetCommandError::ReferenceEmpty,
            AssetDomainError::InvalidRiskLevel(_) => AssetCommandError::InvalidRiskLevel,
            AssetDomainError::InvalidCurrency(_) => AssetCommandError::InvalidCurrency,
            AssetDomainError::Archived => AssetCommandError::Archived,
            AssetDomainError::CashAssetNotEditable => AssetCommandError::CashAssetNotEditable,
            AssetDomainError::NotFound(_) => AssetCommandError::NotFound,
        };
    }
    if let Some(err) = e.downcast_ref::<crate::context::asset::CategoryApplicationError>() {
        return match err {
            crate::context::asset::CategoryApplicationError::NotFound { .. } => {
                AssetCommandError::CategoryNotFound
            }
            other => {
                tracing::error!(err = ?other, "unexpected category error in asset command");
                AssetCommandError::Unknown
            }
        };
    }
    tracing::error!(err = ?e, "unexpected error in asset command");
    AssetCommandError::Unknown
}

/// Typed error returned to the frontend for the record_asset_price command.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum AssetPriceCommandError {
    /// The asset referenced in the command does not exist (MKT-043).
    #[error("Asset not found")]
    AssetNotFound,
    /// Price must be strictly positive.
    #[error("Price must be strictly positive")]
    NotPositive,
    /// Price value is not a finite number.
    #[error("Price must be a finite number")]
    NonFinite,
    /// Price date is in the future.
    #[error("Date cannot be in the future")]
    DateInFuture,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_asset_price_error(e: anyhow::Error) -> AssetPriceCommandError {
    if let Some(err) = e.downcast_ref::<AssetDomainError>() {
        if matches!(err, AssetDomainError::NotFound(_)) {
            return AssetPriceCommandError::AssetNotFound;
        }
    }
    if let Some(err) = e.downcast_ref::<AssetPriceDomainError>() {
        return match err {
            AssetPriceDomainError::NotPositive => AssetPriceCommandError::NotPositive,
            AssetPriceDomainError::NonFinite => AssetPriceCommandError::NonFinite,
            AssetPriceDomainError::DateInFuture => AssetPriceCommandError::DateInFuture,
            AssetPriceDomainError::NotFound => {
                tracing::warn!("AssetPriceDomainError::NotFound routed through to_asset_price_error — use to_update/delete_asset_price_error instead");
                AssetPriceCommandError::Unknown
            }
        };
    }
    tracing::error!(err = ?e, "unexpected error in asset price command");
    AssetPriceCommandError::Unknown
}

/// Typed error returned to the frontend for the update_asset_price command.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum UpdateAssetPriceCommandError {
    /// No price record exists for the given (asset_id, original_date) (MKT-083).
    #[error("Asset price not found")]
    NotFound,
    /// Price must be strictly positive.
    #[error("Price must be strictly positive")]
    NotPositive,
    /// Price value is not a finite number.
    #[error("Price must be a finite number")]
    NonFinite,
    /// Price date is in the future.
    #[error("Date cannot be in the future")]
    DateInFuture,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_update_asset_price_error(e: anyhow::Error) -> UpdateAssetPriceCommandError {
    if let Some(err) = e.downcast_ref::<AssetPriceDomainError>() {
        return match err {
            AssetPriceDomainError::NotFound => UpdateAssetPriceCommandError::NotFound,
            AssetPriceDomainError::NotPositive => UpdateAssetPriceCommandError::NotPositive,
            AssetPriceDomainError::NonFinite => UpdateAssetPriceCommandError::NonFinite,
            AssetPriceDomainError::DateInFuture => UpdateAssetPriceCommandError::DateInFuture,
        };
    }
    tracing::error!(err = ?e, "unexpected error in update_asset_price command");
    UpdateAssetPriceCommandError::Unknown
}

/// Typed error returned to the frontend for the delete_asset_price command.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum DeleteAssetPriceCommandError {
    /// No price record exists for the given (asset_id, date) (MKT-090).
    #[error("Asset price not found")]
    NotFound,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_delete_asset_price_error(e: anyhow::Error) -> DeleteAssetPriceCommandError {
    if let Some(err) = e.downcast_ref::<AssetPriceDomainError>() {
        if matches!(err, AssetPriceDomainError::NotFound) {
            return DeleteAssetPriceCommandError::NotFound;
        }
    }
    tracing::error!(err = ?e, "unexpected error in delete_asset_price command");
    DeleteAssetPriceCommandError::Unknown
}

// --- Assets ---

/// Fetches all active (non-archived) assets.
#[tauri::command]
#[specta::specta]
pub async fn get_assets(state: State<'_, AppState>) -> Result<Vec<Asset>, AssetCommandError> {
    state
        .asset_service
        .get_all_assets()
        .await
        .map_err(to_asset_error)
}

/// Fetches all assets including archived ones.
#[tauri::command]
#[specta::specta]
pub async fn get_assets_with_archived(
    state: State<'_, AppState>,
) -> Result<Vec<Asset>, AssetCommandError> {
    state
        .asset_service
        .get_all_assets_with_archived()
        .await
        .map_err(to_asset_error)
}

/// Adds a new asset.
#[tauri::command]
#[specta::specta]
pub async fn add_asset(
    state: State<'_, AppState>,
    dto: CreateAssetDTO,
) -> Result<Asset, AssetCommandError> {
    state
        .asset_service
        .create_asset(dto)
        .await
        .map_err(to_asset_error)
}

/// Updates an existing asset.
#[tauri::command]
#[specta::specta]
pub async fn update_asset(
    state: State<'_, AppState>,
    dto: UpdateAssetDTO,
) -> Result<Asset, AssetCommandError> {
    state
        .asset_service
        .update_asset(dto)
        .await
        .map_err(to_asset_error)
}

/// Unarchives an asset (R18).
#[tauri::command]
#[specta::specta]
pub async fn unarchive_asset(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AssetCommandError> {
    state
        .asset_service
        .unarchive_asset(&id)
        .await
        .map_err(to_asset_error)
}

// --- Categories ---

/// Fetches all active categories.
///
/// Read-only — only infrastructure failures can fire here, so the surface is
/// the narrow `CategoryApplicationError` (only `DatabaseError` is reachable).
#[tauri::command]
#[specta::specta]
pub async fn get_categories(
    state: State<'_, AppState>,
) -> Result<Vec<AssetCategory>, crate::context::asset::CategoryApplicationError> {
    state.asset_service.get_all_categories().await
}

/// Creates a new category.
///
/// Returns the typed `CategoryCrudError` directly — no boundary type or mapper
/// is needed because every leaf in the composite (`CategoryApplicationError`,
/// `CategoryDomainError`) already serializes with `#[serde(tag = "code")]`,
/// and `CategoryCrudError`'s `#[serde(untagged)]` flattens them into a single
/// FE-visible union.
#[tauri::command]
#[specta::specta]
pub async fn add_category(
    label: String,
    state: State<'_, AppState>,
) -> Result<AssetCategory, crate::context::asset::CategoryCrudError> {
    state.asset_service.create_category(&label).await
}

/// Updates an existing category.
#[tauri::command]
#[specta::specta]
pub async fn update_category(
    id: String,
    label: String,
    state: State<'_, AppState>,
) -> Result<AssetCategory, crate::context::asset::CategoryCrudError> {
    state.asset_service.update_category(&id, &label).await
}

/// Deletes a category.
#[tauri::command]
#[specta::specta]
pub async fn delete_category(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), crate::context::asset::CategoryCrudError> {
    state.asset_service.delete_category(&id).await
}

// --- AssetPrice ---

/// Records (or overwrites) a market price for an asset on a given date (MKT-024/025).
/// price is a human-readable decimal; the backend converts to i64 micros at this boundary (MKT-024).
#[tauri::command]
#[specta::specta]
pub async fn record_asset_price(
    state: State<'_, AppState>,
    asset_id: String,
    date: String,
    price: f64,
) -> Result<(), AssetPriceCommandError> {
    state
        .asset_service
        .record_asset_price(&asset_id, &date, price)
        .await
        .map_err(to_asset_price_error)
}

/// Returns all recorded prices for the given asset, sorted date descending (MKT-072).
#[tauri::command]
#[specta::specta]
pub async fn get_asset_prices(
    state: State<'_, AppState>,
    asset_id: String,
) -> Result<Vec<AssetPrice>, AssetPriceCommandError> {
    state
        .asset_service
        .get_asset_prices(&asset_id)
        .await
        .map_err(to_asset_price_error)
}

/// Updates the date and/or price of an existing price record (MKT-083/084).
#[tauri::command]
#[specta::specta]
pub async fn update_asset_price(
    state: State<'_, AppState>,
    asset_id: String,
    original_date: String,
    new_date: String,
    new_price: f64,
) -> Result<(), UpdateAssetPriceCommandError> {
    state
        .asset_service
        .update_asset_price(&asset_id, &original_date, &new_date, new_price)
        .await
        .map_err(to_update_asset_price_error)
}

/// Deletes a specific price record by (asset_id, date) (MKT-090).
#[tauri::command]
#[specta::specta]
pub async fn delete_asset_price(
    state: State<'_, AppState>,
    asset_id: String,
    date: String,
) -> Result<(), DeleteAssetPriceCommandError> {
    state
        .asset_service
        .delete_asset_price(&asset_id, &date)
        .await
        .map_err(to_delete_asset_price_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::asset::domain::error::{AssetDomainError, AssetPriceDomainError};

    // CSH-016 — to_asset_error maps AssetDomainError::CashAssetNotEditable to AssetCommandError::CashAssetNotEditable
    #[test]
    fn to_asset_error_maps_cash_asset_not_editable() {
        let domain_err = AssetDomainError::CashAssetNotEditable;
        let cmd_err = to_asset_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, AssetCommandError::CashAssetNotEditable),
            "got: {cmd_err:?}"
        );
    }

    // MKT-043 — to_asset_price_error maps AssetDomainError::NotFound to AssetPriceCommandError::AssetNotFound
    #[test]
    fn to_asset_price_error_maps_asset_not_found() {
        let domain_err = AssetDomainError::NotFound("asset-xyz".to_string());
        let cmd_err = to_asset_price_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, AssetPriceCommandError::AssetNotFound),
            "got: {cmd_err:?}"
        );
    }

    // update_asset_price command error — NotFound maps correctly
    #[test]
    fn to_update_asset_price_error_maps_not_found() {
        let domain_err = AssetPriceDomainError::NotFound;
        let cmd_err = to_update_asset_price_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, UpdateAssetPriceCommandError::NotFound),
            "got: {cmd_err:?}"
        );
    }

    // update_asset_price command error — NotPositive maps correctly
    #[test]
    fn to_update_asset_price_error_maps_not_positive() {
        let domain_err = AssetPriceDomainError::NotPositive;
        let cmd_err = to_update_asset_price_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, UpdateAssetPriceCommandError::NotPositive),
            "got: {cmd_err:?}"
        );
    }

    // update_asset_price command error — NonFinite maps correctly
    #[test]
    fn to_update_asset_price_error_maps_non_finite() {
        let domain_err = AssetPriceDomainError::NonFinite;
        let cmd_err = to_update_asset_price_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, UpdateAssetPriceCommandError::NonFinite),
            "got: {cmd_err:?}"
        );
    }

    // update_asset_price command error — DateInFuture maps correctly
    #[test]
    fn to_update_asset_price_error_maps_date_in_future() {
        let domain_err = AssetPriceDomainError::DateInFuture;
        let cmd_err = to_update_asset_price_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, UpdateAssetPriceCommandError::DateInFuture),
            "got: {cmd_err:?}"
        );
    }

    // delete_asset_price command error — NotFound maps correctly
    #[test]
    fn to_delete_asset_price_error_maps_not_found() {
        let domain_err = AssetPriceDomainError::NotFound;
        let cmd_err = to_delete_asset_price_error(anyhow::anyhow!(domain_err));
        assert!(
            matches!(cmd_err, DeleteAssetPriceCommandError::NotFound),
            "got: {cmd_err:?}"
        );
    }
}
