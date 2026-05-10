use crate::context::asset::domain::CategoryDomainError;

/// Application-layer rejections for the Category sub-aggregate of the Asset
/// bounded context — concerns raised at the service layer rather than by an
/// aggregate method on its own loaded state.
///
/// Per the rejection-layer rule (`docs/ddd-reference.md` § Errors):
/// - `NotFound` is born when `category_repo.get_by_id` returns `Ok(None)` —
///   a service-level translation, not an aggregate invariant.
/// - `DuplicateName` is born when the service-layer `find_by_name` uniqueness
///   pre-check matches an existing row — a cross-aggregate invariant.
/// - `DatabaseError` is the application-layer translation of any raw
///   infrastructure failure from a category-repo call. The diagnostic chain
///   is preserved via `tracing::error!` at the same translation site; the
///   variant carries no payload (per the project-specific infra-translation
///   rule in `docs/plan/error-model-refactor.md`).
///
/// Tagged with `#[serde(tag = "code")]` so each variant serializes as a flat
/// `{ code: "..." }` shape across the Tauri boundary. `NotFound` carries the
/// requested ID as a struct field so the FE can surface it diagnostically.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum CategoryApplicationError {
    /// No category exists with the requested ID. Born at the service layer
    /// when `category_repo.get_by_id` returns `None`.
    #[error("Category not found: {id}")]
    NotFound {
        /// The ID the caller asked for.
        id: String,
    },
    /// A category with the same name (case-insensitive) already exists. Born
    /// at the service layer from a `find_by_name` uniqueness pre-check before
    /// the repository write — a cross-aggregate invariant, not a single-
    /// aggregate state rule.
    #[error("A category with this name already exists")]
    DuplicateName,
    /// Application-layer translation of any infrastructure failure from a
    /// category-repo call. Unit variant — no `hint` payload on the wire; the
    /// full diagnostic chain is preserved server-side via `tracing::error!`
    /// at the translation site. FE shows the i18n key `error.DatabaseError`.
    #[error("An unexpected database error occurred")]
    DatabaseError,
}

/// Service-layer composite for the **Category CRUD** failure surface — the
/// write commands `add_category`, `update_category`, `delete_category`.
///
/// Replaces the anyhow-era `CategoryCommandError` boundary type. **First PR
/// (6) enforcing the gold infra-translation rule**: this composite has NO
/// shared `InfrastructureError` leaf — infra failures are translated at the
/// application layer into `CategoryApplicationError::DatabaseError` (typed,
/// payload-free) rather than passed through opaquely.
///
/// **This IS the FE-facing contract** for write commands. `get_categories`
/// (read-only) returns the narrower `CategoryApplicationError` directly
/// because it has no domain-rejection paths.
///
/// Each leaf lives in its rightful layer:
/// - `CategoryApplicationError` — application layer (this module) — raises
///   `NotFound`, `DuplicateName`, `DatabaseError`.
/// - `CategoryDomainError` — domain layer (`asset/domain/`) — raises
///   `LabelEmpty` (value-object validation), `SystemReadonly` /
///   `SystemProtected` (aggregate-method invariants on loaded state).
///
/// `CategoryCrudError` itself owns no variants; it only enumerates which
/// leaves the create/update/delete commands can produce.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum CategoryCrudError {
    /// Service-layer rejection (`NotFound`, `DuplicateName`, `DatabaseError`).
    #[error(transparent)]
    Application(#[from] CategoryApplicationError),
    /// Aggregate-level domain rejection (`LabelEmpty`, `SystemReadonly`,
    /// `SystemProtected`).
    #[error(transparent)]
    Validation(#[from] CategoryDomainError),
}
