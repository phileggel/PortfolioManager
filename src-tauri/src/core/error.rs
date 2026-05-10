/// Shared application-wide infrastructure-error type.
///
/// Composed into every typed service composite (e.g. `HoldingTransactionError`)
/// via `#[from]` so any layer can translate an opaque infrastructure failure
/// (repository crash, file-system error, network failure, deserialization
/// error, cross-BC infra step) into a typed leaf without re-defining the same
/// `Unknown { hint }` shape per bounded context.
///
/// Per `docs/ddd-reference.md` § Errors travel rule: raw infrastructure
/// failures must be translated at the application boundary into either a
/// meaningful application error OR an opaque variant. This is the latter —
/// the `hint` is developer-only diagnostic mirroring the corresponding
/// `tracing::error!` log; the FE displays the i18n key `error.Unknown` and
/// forwards `hint` to the JS console via `logger.error`.
///
/// Tagged with `#[serde(tag = "code")]` so the wire shape is
/// `{ code: "Unknown", hint: "..." }` — identical across every command that
/// surfaces it through any composite.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(tag = "code")]
pub enum InfrastructureError {
    /// Opaque catch-all for any infrastructure failure with no domain meaning.
    /// Construct via `InfrastructureError::Unknown { hint: format!("...") }`
    /// or via `?` from any composite that has `#[from] InfrastructureError`.
    #[error("Unexpected infrastructure error: {hint}")]
    Unknown {
        /// Developer-only diagnostic string. Not user-facing.
        hint: String,
    },
}
