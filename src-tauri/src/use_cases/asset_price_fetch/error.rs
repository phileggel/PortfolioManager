use serde::Serialize;
use specta::Type;

/// Use-case-specific outcomes for the asset-price fetch tasks shared by
/// `FetchAllAssetPricesError` and `FetchAccountAssetPricesError`.
///
/// Carried inside each composite as a single `Failure(#[from] FetchPriceTask)` arm.
/// `#[serde(tag = "code")]` gives every variant a `{ "code": "..." }` payload so the
/// surrounding `#[serde(untagged)]` composite emits a flat, narrowable shape on the
/// wire.
#[derive(Debug, thiserror::Error, Serialize, Type, Clone)]
#[serde(tag = "code")]
pub enum FetchPriceTask {
    /// A fetch task is already in progress (MKT-113).
    #[error("A fetch task is already running")]
    FetchAlreadyRunning,
    /// No active holdings with a derivable provider symbol found in scope (MKT-111).
    #[error("No fetchable holdings in scope")]
    NoFetchableHoldings,
    /// Catch-all for unexpected runtime failures not attributable to a specific BC.
    #[error("Unexpected error")]
    UnknownError,
}
