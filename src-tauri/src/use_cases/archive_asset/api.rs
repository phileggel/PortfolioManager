// Allow unreachable lint as tauri::command and specta::specta macros generate false positives
#![allow(clippy::unreachable)]

use super::{ArchiveAssetError, ArchiveAssetUseCase};
use crate::context::asset::{AssetApplicationError, AssetCrudError, AssetDomainError};
use crate::core::logger::BACKEND;
use serde::Serialize;
use specta::Type;
use tauri::State;

/// Typed error returned to the frontend for the archive_asset command.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "code")]
pub enum ArchiveAssetCommandError {
    /// Asset still has non-zero holdings in at least one account.
    #[error("Cannot archive an asset with active holdings")]
    ActiveHoldings,
    /// Target asset is a system Cash Asset and cannot be archived (CSH-016).
    #[error("Cannot archive a system Cash Asset")]
    CashAssetNotEditable,
    /// No asset exists with the requested ID.
    #[error("Asset not found")]
    NotFound,
    /// An unexpected server-side error occurred.
    #[error("An unexpected error occurred")]
    Unknown,
}

fn to_archive_error(e: anyhow::Error) -> ArchiveAssetCommandError {
    if let Some(err) = e.downcast_ref::<ArchiveAssetError>() {
        return match err {
            ArchiveAssetError::ActiveHoldings => ArchiveAssetCommandError::ActiveHoldings,
        };
    }
    if let Some(err) = e.downcast_ref::<AssetCrudError>() {
        return match err {
            AssetCrudError::Application(AssetApplicationError::NotFound { .. }) => {
                ArchiveAssetCommandError::NotFound
            }
            AssetCrudError::Validation(AssetDomainError::CashAssetNotEditable) => {
                ArchiveAssetCommandError::CashAssetNotEditable
            }
            other => {
                tracing::error!(target: BACKEND, err = ?other, "unexpected asset error in archive_asset command");
                ArchiveAssetCommandError::Unknown
            }
        };
    }
    tracing::error!(target: BACKEND, err = ?e, "unexpected error in archive_asset command");
    ArchiveAssetCommandError::Unknown
}

/// Archives an asset, guarded against active holdings (OQ-6).
#[tauri::command]
#[specta::specta]
pub async fn archive_asset(
    uc: State<'_, ArchiveAssetUseCase>,
    id: String,
) -> Result<(), ArchiveAssetCommandError> {
    uc.archive_asset(&id).await.map_err(to_archive_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    // CSH-016 — to_archive_error maps AssetCrudError(Validation(CashAssetNotEditable))
    #[test]
    fn to_archive_error_maps_cash_asset_not_editable() {
        let crud_err: AssetCrudError = AssetDomainError::CashAssetNotEditable.into();
        let cmd_err = to_archive_error(anyhow::anyhow!(crud_err));
        assert!(
            matches!(cmd_err, ArchiveAssetCommandError::CashAssetNotEditable),
            "got: {cmd_err:?}"
        );
    }

    // to_archive_error maps AssetCrudError(Application(NotFound)) → NotFound
    #[test]
    fn to_archive_error_maps_not_found() {
        let crud_err: AssetCrudError = AssetApplicationError::NotFound {
            id: "missing".into(),
        }
        .into();
        let cmd_err = to_archive_error(anyhow::anyhow!(crud_err));
        assert!(
            matches!(cmd_err, ArchiveAssetCommandError::NotFound),
            "got: {cmd_err:?}"
        );
    }
}
