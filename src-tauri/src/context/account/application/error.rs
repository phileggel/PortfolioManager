use crate::context::account::domain::{AccountOperationError, TransactionDomainError};
use crate::core::InfrastructureError;

/// Application-layer errors raised by the Account bounded context — concerns
/// that belong to use-case orchestration rather than aggregate invariants.
///
/// Per Rule B' (`docs/plan/error-model-refactor.md`): an error is **domain**
/// only if raised by an aggregate method on its own loaded state. Anything
/// raised at the service/use-case layer — `NotFound` lookups, cross-aggregate
/// preconditions, infrastructure translations — is **application**.
///
/// Tagged with `#[serde(tag = "code")]` so it serializes verbatim across the
/// Tauri boundary into a flat `{ code: "...", ... }` shape, identical to
/// existing domain error enums.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum AccountApplicationError {
    /// No account exists with the requested ID. Born at the service layer
    /// when a repository lookup returns `None`; never raised by an aggregate.
    /// `account_id` mirrors the (deprecated) `AccountDomainError::AccountNotFound`
    /// payload so the diagnostic chain doesn't lose the requested ID.
    #[error("Account not found: {account_id}")]
    AccountNotFound {
        /// The ID the caller asked for.
        account_id: String,
    },
}

/// Service-layer composite for the cash-recording failure-surface-family
/// (`AccountService::record_deposit` and `record_withdrawal`). Replaces the
/// deleted domain-layer `CashOperationError` per Rule B' — composition belongs
/// in the application layer; each leaf retains its single, typed failure
/// source in its proper layer (no cash-prefixed leaf types).
///
/// **This IS the FE-facing contract** for cash-recording Tauri commands. No
/// separate boundary type / mapper is needed: the four leaf enums below each
/// derive `Serialize` + `specta::Type` with `#[serde(tag = "code")]`, and
/// `#[serde(untagged)]` here flattens them into a single FE-visible union of
/// `{ code: "...", ... }` discriminated variants.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (`account/application/`)
/// - `AccountOperationError` — domain layer (`account/domain/`)
/// - `TransactionDomainError` — domain layer (`account/domain/`)
/// - `InfrastructureError` — shared catch-all (`core/`)
///
/// `CashRecordingError` itself owns no variants; it only enumerates which
/// leaves cash recording can produce.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum CashRecordingError {
    /// Application-layer rejection (`AccountNotFound`).
    #[error(transparent)]
    Application(#[from] AccountApplicationError),
    /// Aggregate-level domain rejection (e.g. `InsufficientCash` from
    /// `Account::apply_withdrawal`). Cash input-validation rejections
    /// (`AmountNotPositive`) live in `Validation` below — raised by the cash
    /// factories, not the aggregate.
    #[error(transparent)]
    Operation(#[from] AccountOperationError),
    /// Transaction-factory validation rejection (invalid date variants, etc.).
    #[error(transparent)]
    Validation(#[from] TransactionDomainError),
    /// Opaque catch-all for repository / cross-BC infrastructure failures.
    /// Wire shape: `{ code: "Unknown", hint: "..." }`. The `hint` mirrors the
    /// corresponding `tracing::error!` log; FE shows `error.Unknown`.
    #[error(transparent)]
    Infrastructure(#[from] InfrastructureError),
}
