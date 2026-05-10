/// Typed errors for asset domain validation.
#[derive(Debug, thiserror::Error)]
pub enum AssetDomainError {
    /// Asset name is empty or whitespace-only.
    #[error("Asset name cannot be empty")]
    NameEmpty,
    /// Asset reference (ticker/ISIN) is empty or whitespace-only.
    #[error("Asset reference cannot be empty")]
    ReferenceEmpty,
    /// Risk level is outside the 1–5 range.
    #[error("Risk level must be between 1 and 5 (received: {0})")]
    InvalidRiskLevel(u8),
    /// The currency string is not a valid ISO 4217 code.
    #[error("Invalid currency code: {0}")]
    InvalidCurrency(String),
    /// The asset is archived and cannot be edited.
    #[error("Cannot edit an archived asset")]
    Archived,
    /// The asset is a system Cash Asset and cannot be edited, archived, unarchived, or deleted (CSH-016).
    #[error("Cannot edit a system Cash Asset")]
    CashAssetNotEditable,
    /// No asset with the given ID exists.
    #[error("Asset not found: {0}")]
    NotFound(String),
}

/// Typed errors for asset price domain validation.
#[derive(Debug, thiserror::Error)]
pub enum AssetPriceDomainError {
    /// Price must be strictly positive.
    #[error("Price must be strictly positive")]
    NotPositive,
    /// Price value is not a finite floating-point number.
    /// Emitted by the service boundary before micro conversion; `AssetPrice::new()` never produces this.
    #[error("Price must be a finite number")]
    NonFinite,
    /// Price date is in the future.
    #[error("Date cannot be in the future")]
    DateInFuture,
    /// No price record exists for the given (asset_id, date) pair.
    #[error("Asset price not found")]
    NotFound,
}

/// Typed errors for category domain validation.
///
/// Only genuine aggregate-method or value-object rejections live here per the
/// rejection-layer rule (`docs/ddd-reference.md` § Errors):
/// - `LabelEmpty` — value-object validation in `AssetCategory::new` /
///   `update_from`.
/// - `SystemReadonly` / `SystemProtected` — aggregate methods
///   `ensure_renameable` / `ensure_deletable` enforced on loaded state.
///
/// Tagged with `#[serde(tag = "code")]` so it can be exposed verbatim at the
/// Tauri boundary through the `CategoryCrudError` untagged composite.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum CategoryDomainError {
    /// Category label is empty or whitespace-only.
    #[error("Category label cannot be empty")]
    LabelEmpty,
    /// Attempt to rename the system default category.
    #[error("The system category cannot be renamed")]
    SystemReadonly,
    /// Attempt to delete the system default category.
    #[error("The system category cannot be deleted")]
    SystemProtected,
}

// `CategoryDomainError::NotFound(String)` and `DuplicateName` were migrated to
// `CategoryApplicationError` in PR 6 (`refactor/category-crud-typed-error`).
// Both are service-layer rejections (repository `Ok(None)` translation /
// uniqueness pre-check), not single-aggregate state rules — application-class
// per the rejection-layer rule (`docs/ddd-reference.md` § Errors).
