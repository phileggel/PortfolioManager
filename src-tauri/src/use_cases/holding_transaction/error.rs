use crate::context::account::{
    AccountApplicationError, OpeningBalanceDomainError, TransactionDomainError,
};
use crate::core::InfrastructureError;

/// Application-layer rejections specific to the `open_holding` use case —
/// cross-BC asset checks performed by the orchestrator before delegating to
/// `AccountService::open_holding`.
///
/// Per Rule B' (`docs/plan/error-model-refactor.md`): an error is **domain**
/// only if raised by an aggregate method on its own loaded state. These three
/// rejections are use-case orchestration concerns (the orchestrator queries
/// the asset service and decides whether to proceed) — application-class.
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
/// command `open_holding` (TRX-042) and its full chain of rejections. Replaces
/// the anyhow-era `OpenHoldingCommandError` boundary type per Rule B' —
/// composition belongs in the use-case layer; each leaf retains its single,
/// typed failure source in its proper layer.
///
/// **This IS the FE-facing contract** for the `open_holding` Tauri command.
/// No separate boundary type / mapper is needed: each leaf below derives
/// `Serialize` + `specta::Type` with `#[serde(tag = "code")]`, and
/// `#[serde(untagged)]` here flattens them into a single FE-visible union of
/// `{ code: "...", ... }` discriminated variants.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (`account/application/`),
///   raises `AccountNotFound { account_id }` from the service-layer load.
/// - `OpenHoldingApplicationError` — use-case-owned (this file), raises the
///   3 cross-BC rejections (`AssetNotFound`, `ArchivedAsset`,
///   `OpeningBalanceOnCashAsset`).
/// - `OpeningBalanceDomainError` — domain layer (`account/domain/`), raises
///   `InvalidTotalCost` from `Account::open_holding` on its own input.
/// - `TransactionDomainError` — domain layer (`account/domain/`), raises
///   the date / quantity invariants enforced by `Transaction::new`.
/// - `InfrastructureError` — shared catch-all (`core/`), opaques repository /
///   cross-BC asset-service infrastructure failures.
///
/// `OpenHoldingError` itself owns no variants; it only enumerates which
/// leaves the open_holding command can produce.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum OpenHoldingError {
    /// Service-layer rejection (`AccountNotFound`).
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
    /// Opaque catch-all for repository / cross-BC asset-service failures.
    /// Wire shape: `{ code: "Unknown", hint: "..." }`. The `hint` mirrors the
    /// corresponding `tracing::error!` log; FE shows `error.Unknown`.
    #[error(transparent)]
    Infrastructure(#[from] InfrastructureError),
}
