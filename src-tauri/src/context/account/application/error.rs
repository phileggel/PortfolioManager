use crate::context::account::domain::{
    AccountDomainError, AccountOperationError, TransactionDomainError,
};
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
    /// Account name (case-insensitive) collides with an existing one. Born at
    /// the service layer from a `find_by_name` uniqueness pre-check before the
    /// repository write — a cross-aggregate invariant, not a single-aggregate
    /// state rule, so application-class per Rule B'.
    #[error("Account name already exists")]
    NameAlreadyExists,
}

/// Service-layer composite for the **holding-transaction** failure surface —
/// every operation that mutates an Account's holdings ledger:
/// `record_deposit`, `record_withdrawal`, `buy_holding`, `sell_holding`,
/// `correct_transaction`, `cancel_transaction`.
///
/// Cash deposit / withdrawal are the special case where the holding IS the
/// System Cash Asset (CSH-014); mechanically they share the same aggregate,
/// the same replay invariants, and therefore the same failure surface. So
/// they share the composite — one FE-facing type covers all six commands.
///
/// Replaces the deleted domain-layer `CashOperationError` and the anyhow-era
/// `TransactionCommandError` boundary type per Rule B' — composition belongs
/// in the application layer; each leaf retains its single, typed failure
/// source in its proper layer.
///
/// **This IS the FE-facing contract** for holding-transaction Tauri commands.
/// No separate boundary type / mapper is needed: the four leaf enums below
/// each derive `Serialize` + `specta::Type` with `#[serde(tag = "code")]`, and
/// `#[serde(untagged)]` here flattens them into a single FE-visible union of
/// `{ code: "...", ... }` discriminated variants.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (`account/application/`)
/// - `AccountOperationError` — domain layer (`account/domain/`)
/// - `TransactionDomainError` — domain layer (`account/domain/`)
/// - `InfrastructureError` — shared catch-all (`core/`)
///
/// `HoldingTransactionError` itself owns no variants; it only enumerates which
/// leaves any holding-transaction command can produce.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum HoldingTransactionError {
    /// Application-layer rejection (`AccountNotFound`).
    #[error(transparent)]
    Application(#[from] AccountApplicationError),
    /// Aggregate-level domain rejection (e.g. `InsufficientCash` from
    /// `Account::apply_withdrawal`, `Oversell` from `sell_holding`).
    #[error(transparent)]
    Operation(#[from] AccountOperationError),
    /// Transaction-factory validation rejection (invalid date, negative
    /// quantity, `AmountNotPositive` from cash factories, etc.).
    #[error(transparent)]
    Validation(#[from] TransactionDomainError),
    /// Opaque catch-all for repository / cross-BC infrastructure failures.
    /// Wire shape: `{ code: "Unknown", hint: "..." }`. The `hint` mirrors the
    /// corresponding `tracing::error!` log; FE shows `error.Unknown`.
    #[error(transparent)]
    Infrastructure(#[from] InfrastructureError),
}

/// Service-layer composite for the **Account CRUD** failure surface — the
/// write commands `add_account` and `update_account`. Replaces the anyhow-era
/// `AccountCommandError` boundary type per the rejection-layer rule
/// (`docs/ddd-reference.md` § Errors): each leaf retains its single, typed
/// failure source in its proper layer; `#[serde(untagged)]` flattens them
/// into a single FE-visible union of `{ code: "..." }` discriminated variants.
///
/// **This IS the FE-facing contract** for `add_account` / `update_account`.
/// No separate boundary type / mapper is needed. `delete_account`,
/// `get_accounts`, and `get_asset_ids_for_account` use the narrower shared
/// `InfrastructureError` directly because they have no domain-rejection paths.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (this module) — raises
///   `NameAlreadyExists` from the service-layer uniqueness pre-check.
/// - `AccountDomainError` — domain layer (`account/domain/`) — raises
///   `NameEmpty` / `InvalidCurrency` from the `Account::new` /
///   `Account::with_id` constructors on their own input.
/// - `InfrastructureError` — shared catch-all (`core/`) — opaques repository
///   failures.
///
/// `AccountCrudError` itself owns no variants; it only enumerates which
/// leaves the create/update commands can produce.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum AccountCrudError {
    /// Service-layer rejection (`NameAlreadyExists`).
    #[error(transparent)]
    Application(#[from] AccountApplicationError),
    /// Aggregate-constructor rejection (`NameEmpty`, `InvalidCurrency`).
    #[error(transparent)]
    Validation(#[from] AccountDomainError),
    /// Opaque catch-all for repository failures.
    #[error(transparent)]
    Infrastructure(#[from] InfrastructureError),
}
