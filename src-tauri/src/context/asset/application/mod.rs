/// Asset application-layer types — errors raised at the service / use-case
/// layer per the rejection-layer rule (`docs/ddd-reference.md` § Errors).
///
/// First populated for PR 6 of the error-model refactor (Category CRUD).
/// Subsequent PRs will add `AssetApplicationError`, `AssetPriceApplicationError`,
/// etc. as each family migrates.
pub mod error;

pub use error::{
    AssetApplicationError, AssetCrudError, CategoryApplicationError, CategoryCrudError,
};
