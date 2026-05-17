use serde::Serialize;
use specta::Type;

/// Flat error enum for the asset bounded context (per error-model.md).
///
/// Holds every variant the asset BC can raise on the new fetch surface. The
/// existing `AssetApplicationError`, `AssetPriceApplicationError`, and the
/// legacy composites remain untouched on the existing CRUD / price-history
/// surfaces (see docs/techdebt.md for the planned retrofit).
#[derive(Debug, thiserror::Error, Serialize, Type, Clone)]
#[serde(tag = "code")]
pub enum AssetError {
    /// Application-layer translation of any infrastructure failure from an
    /// asset-repo call on the fetch surface.
    #[error("An unexpected database error occurred")]
    DatabaseError,
}
