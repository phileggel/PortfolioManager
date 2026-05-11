use crate::context::account::domain::{
    AccountDomainError, AccountOperationError, TransactionDomainError,
};

/// Application-layer errors raised by the Account bounded context.
///
/// Tagged with `#[serde(tag = "code")]` so it serializes verbatim across the
/// Tauri boundary into a flat `{ code: "...", ... }` shape.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum AccountApplicationError {
    /// No account exists with the requested ID.
    #[error("Account not found: {account_id}")]
    AccountNotFound {
        /// The ID the caller asked for.
        account_id: String,
    },
    /// Account name (case-insensitive) collides with an existing one.
    #[error("Account name already exists")]
    NameAlreadyExists,
    /// Application-layer translation of any infrastructure failure from an
    /// account-side repository call. Unit variant — no `hint` payload on the
    /// wire; the full diagnostic chain is preserved server-side via
    /// `tracing::error!` at the translation site. FE shows the i18n key
    /// `error.DatabaseError`.
    #[error("An unexpected database error occurred")]
    DatabaseError,
}

/// Service-layer composite for the **holding-transaction** failure surface —
/// every operation that mutates an Account's holdings ledger:
/// `record_deposit`, `record_withdrawal`, `buy_holding`, `sell_holding`,
/// `correct_transaction`, `cancel_transaction`.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (this module)
/// - `AccountOperationError` — domain layer (`account/domain/`)
/// - `TransactionDomainError` — domain layer (`account/domain/`)
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum HoldingTransactionError {
    /// Application-layer rejection (`AccountNotFound`, `DatabaseError`).
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
}

/// Service-layer composite for the **Account CRUD** failure surface — the
/// write commands `add_account` and `update_account`.
///
/// Each leaf lives in its rightful layer:
/// - `AccountApplicationError` — application layer (this module) — raises
///   `NameAlreadyExists` and `DatabaseError`.
/// - `AccountDomainError` — domain layer (`account/domain/`) — raises
///   `NameEmpty` / `InvalidCurrency` from the `Account::new` /
///   `Account::with_id` constructors on their own input.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum AccountCrudError {
    /// Service-layer rejection (`NameAlreadyExists`, `DatabaseError`).
    #[error(transparent)]
    Application(#[from] AccountApplicationError),
    /// Aggregate-constructor rejection (`NameEmpty`, `InvalidCurrency`).
    #[error(transparent)]
    Validation(#[from] AccountDomainError),
}
