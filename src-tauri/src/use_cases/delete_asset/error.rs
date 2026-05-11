use crate::context::account::AccountApplicationError;
use crate::context::asset::AssetCrudError;

/// Application-layer rejection specific to the `delete_asset` use case —
/// the cross-BC transaction-history check performed by the orchestrator
/// before delegating to `AssetService::delete_asset`.
///
/// Per the rejection-layer rule (`docs/ddd-reference.md` § Errors): this
/// rejection is born at the orchestrator (it queries the account service and
/// decides whether to proceed), not by an aggregate method on its own loaded
/// state — application-class.
///
/// Tagged with `#[serde(tag = "code")]` so it serializes verbatim across the
/// Tauri boundary into a flat `{ code: "..." }` shape through the
/// `DeleteAssetError` untagged composite.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum DeleteAssetApplicationError {
    /// At least one transaction references this asset; deletion would break history.
    #[error("Cannot delete an asset with existing transactions")]
    ExistingTransactions,
}

/// Use-case composite for the **delete asset** failure surface — the single
/// command `delete_asset` and its full chain of rejections.
///
/// Replaces the anyhow-era `DeleteAssetCommandError` boundary type. This IS
/// the FE-facing contract for the `delete_asset` Tauri command — each leaf
/// already serializes with `#[serde(tag = "code")]`, and `#[serde(untagged)]`
/// here flattens them into a single FE-visible union.
///
/// Each leaf lives in its rightful layer:
/// - `AssetCrudError` — asset BC composite (`asset/application/`), carries
///   `AssetApplicationError::NotFound` and
///   `AssetDomainError::CashAssetNotEditable` propagated verbatim per the
///   composition-over-redefinition rule.
/// - `AccountApplicationError` — account BC (`account/application/`), surfaces
///   `DatabaseError` from the cross-BC transaction-history check.
/// - `DeleteAssetApplicationError` — use-case-owned (this file), raises
///   `ExistingTransactions` from the orchestrator.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum DeleteAssetError {
    /// Asset BC rejection (`NotFound`, `CashAssetNotEditable`, propagated
    /// `DatabaseError`).
    #[error(transparent)]
    Asset(#[from] AssetCrudError),
    /// Account BC rejection (`DatabaseError` from the cross-BC
    /// transaction-history check).
    #[error(transparent)]
    Account(#[from] AccountApplicationError),
    /// Use-case orchestration rejection (`ExistingTransactions`).
    #[error(transparent)]
    Application(#[from] DeleteAssetApplicationError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::asset::{AssetApplicationError, AssetDomainError};

    // CSH-016 — domain CashAssetNotEditable propagates through Asset leaf
    #[test]
    fn cash_asset_not_editable_propagates_through_asset_leaf() {
        let crud_err: AssetCrudError = AssetDomainError::CashAssetNotEditable.into();
        let composite: DeleteAssetError = crud_err.into();
        assert!(
            matches!(
                composite,
                DeleteAssetError::Asset(AssetCrudError::Validation(
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
        let composite: DeleteAssetError = crud_err.into();
        assert!(
            matches!(
                &composite,
                DeleteAssetError::Asset(AssetCrudError::Application(
                    AssetApplicationError::NotFound { id }
                )) if id == "missing-asset"
            ),
            "got: {composite:?}"
        );
    }

    // Account-side DatabaseError surfaces through the Account leaf
    // (cross-BC transaction-history check repo failure)
    #[test]
    fn account_database_error_surfaces_through_account_leaf() {
        let composite: DeleteAssetError = AccountApplicationError::DatabaseError.into();
        assert!(
            matches!(
                composite,
                DeleteAssetError::Account(AccountApplicationError::DatabaseError)
            ),
            "got: {composite:?}"
        );
    }

    // ExistingTransactions surfaces through the Application leaf
    #[test]
    fn existing_transactions_surfaces_through_application_leaf() {
        let composite: DeleteAssetError = DeleteAssetApplicationError::ExistingTransactions.into();
        assert!(
            matches!(
                composite,
                DeleteAssetError::Application(DeleteAssetApplicationError::ExistingTransactions)
            ),
            "got: {composite:?}"
        );
    }
}
