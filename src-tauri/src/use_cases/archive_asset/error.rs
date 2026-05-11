use crate::context::account::AccountApplicationError;
use crate::context::asset::AssetCrudError;

/// Application-layer rejection specific to the `archive_asset` use case —
/// the cross-BC active-holdings check performed by the orchestrator before
/// delegating to `AssetService::archive_asset`.
///
/// Per the rejection-layer rule (`docs/ddd-reference.md` § Errors): this
/// rejection is born at the orchestrator (it queries the account service and
/// decides whether to proceed), not by an aggregate method on its own loaded
/// state — application-class.
///
/// Tagged with `#[serde(tag = "code")]` so it serializes verbatim across the
/// Tauri boundary into a flat `{ code: "..." }` shape through the
/// `ArchiveAssetError` untagged composite.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum ArchiveAssetApplicationError {
    /// Asset still has non-zero holdings in at least one account (OQ-6).
    #[error("Cannot archive an asset with active holdings")]
    ActiveHoldings,
}

/// Use-case composite for the **archive asset** failure surface — the single
/// command `archive_asset` (OQ-6) and its full chain of rejections.
///
/// Replaces the anyhow-era `ArchiveAssetCommandError` boundary type. This IS
/// the FE-facing contract for the `archive_asset` Tauri command — each leaf
/// already serializes with `#[serde(tag = "code")]`, and `#[serde(untagged)]`
/// here flattens them into a single FE-visible union.
///
/// Each leaf lives in its rightful layer:
/// - `AssetCrudError` — asset BC composite (`asset/application/`), carries
///   `AssetApplicationError::NotFound` and
///   `AssetDomainError::CashAssetNotEditable` propagated verbatim per the
///   composition-over-redefinition rule.
/// - `AccountApplicationError` — account BC (`account/application/`), surfaces
///   `DatabaseError` from the cross-BC active-holdings check.
/// - `ArchiveAssetApplicationError` — use-case-owned (this file), raises
///   `ActiveHoldings` from the orchestrator.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum ArchiveAssetError {
    /// Asset BC rejection (`NotFound`, `CashAssetNotEditable`, propagated
    /// `DatabaseError`).
    #[error(transparent)]
    Asset(#[from] AssetCrudError),
    /// Account BC rejection (`DatabaseError` from the cross-BC
    /// active-holdings check).
    #[error(transparent)]
    Account(#[from] AccountApplicationError),
    /// Use-case orchestration rejection (`ActiveHoldings`).
    #[error(transparent)]
    Application(#[from] ArchiveAssetApplicationError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::asset::{AssetApplicationError, AssetDomainError};

    // CSH-016 — domain CashAssetNotEditable propagates through Asset leaf
    #[test]
    fn cash_asset_not_editable_propagates_through_asset_leaf() {
        let crud_err: AssetCrudError = AssetDomainError::CashAssetNotEditable.into();
        let composite: ArchiveAssetError = crud_err.into();
        assert!(
            matches!(
                composite,
                ArchiveAssetError::Asset(AssetCrudError::Validation(
                    AssetDomainError::CashAssetNotEditable
                ))
            ),
            "got: {composite:?}"
        );
    }

    // Asset-side NotFound propagates verbatim with id payload preserved
    #[test]
    fn asset_not_found_propagates_with_id_payload() {
        let crud_err: AssetCrudError = AssetApplicationError::NotFound {
            id: "missing-asset".into(),
        }
        .into();
        let composite: ArchiveAssetError = crud_err.into();
        assert!(
            matches!(
                &composite,
                ArchiveAssetError::Asset(AssetCrudError::Application(
                    AssetApplicationError::NotFound { id }
                )) if id == "missing-asset"
            ),
            "got: {composite:?}"
        );
    }

    // Account-side DatabaseError surfaces through the Account leaf
    // (cross-BC active-holdings check repo failure)
    #[test]
    fn account_database_error_surfaces_through_account_leaf() {
        let composite: ArchiveAssetError = AccountApplicationError::DatabaseError.into();
        assert!(
            matches!(
                composite,
                ArchiveAssetError::Account(AccountApplicationError::DatabaseError)
            ),
            "got: {composite:?}"
        );
    }

    // OQ-6 — ActiveHoldings surfaces through the Application leaf
    #[test]
    fn active_holdings_surfaces_through_application_leaf() {
        let composite: ArchiveAssetError = ArchiveAssetApplicationError::ActiveHoldings.into();
        assert!(
            matches!(
                composite,
                ArchiveAssetError::Application(ArchiveAssetApplicationError::ActiveHoldings)
            ),
            "got: {composite:?}"
        );
    }
}
