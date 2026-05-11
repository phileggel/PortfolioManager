use crate::context::account::{
    AccountApplicationError, OpeningBalanceDomainError, TransactionDomainError,
};

/// Application-layer rejections specific to the `open_holding` use case —
/// cross-BC asset checks performed by the orchestrator before delegating to
/// `AccountService::open_holding`.
///
/// Tagged with `#[serde(tag = "code")]` so it serializes verbatim across the
/// Tauri boundary into a flat `{ code: "..." }` shape.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum OpenHoldingApplicationError {
    /// No asset exists with the requested ID (TRX-056).
    #[error("Asset not found")]
    AssetNotFound,
    /// Target asset is archived — cannot open a holding (TRX-050).
    /// The orchestrator does not auto-unarchive; the caller must unarchive
    /// explicitly through the asset BC first.
    #[error("Cannot open a holding for an archived asset")]
    ArchivedAsset,
    /// Target asset is a system Cash Asset (CSH-061). Initial cash should be
    /// recorded via `record_deposit`, which goes through the cash-recording
    /// path and lazy-creates the Cash Holding.
    #[error("Opening balance cannot be recorded against a cash asset; use record_deposit instead")]
    OpeningBalanceOnCashAsset,
}

/// Use-case composite for the **open holding** failure surface — the single
/// command `open_holding` (TRX-042) and its full chain of rejections.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (`account/application/`),
///   raises `AccountNotFound { account_id }` and `DatabaseError` from
///   account-side service operations. Asset-side `DatabaseError` from the
///   cross-BC `get_asset_by_id` lookup is also tunnelled through this leaf
///   (the orchestrator translates at the call site) so the FE wire surface
///   carries a single `{ code: "DatabaseError" }` shape.
/// - `OpenHoldingApplicationError` — use-case-owned (this file), raises the
///   3 cross-BC rejections (`AssetNotFound`, `ArchivedAsset`,
///   `OpeningBalanceOnCashAsset`).
/// - `OpeningBalanceDomainError` — domain layer (`account/domain/`), raises
///   `InvalidTotalCost` from `Account::open_holding` on its own input.
/// - `TransactionDomainError` — domain layer (`account/domain/`), raises
///   the date / quantity invariants enforced by `Transaction::new`.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum OpenHoldingError {
    /// Account-side rejection (`AccountNotFound`, `DatabaseError`).
    #[error(transparent)]
    Application(#[from] AccountApplicationError),
    /// Use-case-layer rejection (cross-BC asset checks).
    #[error(transparent)]
    UseCase(#[from] OpenHoldingApplicationError),
    /// Aggregate-level domain rejection (`InvalidTotalCost`).
    #[error(transparent)]
    Validation(#[from] OpeningBalanceDomainError),
    /// Transaction-factory validation rejection (invalid date, negative
    /// quantity, etc. — subset of variants reachable from `open_holding`).
    #[error(transparent)]
    TxValidation(#[from] TransactionDomainError),
}
