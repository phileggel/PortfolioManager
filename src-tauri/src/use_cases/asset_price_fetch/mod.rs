//! Auto-fetch use case: retrieves prices from Stooq for all active, derivable
//! holdings on launch (MKT-122), on global refresh (MKT-130), and on
//! per-account refresh (MKT-132).

/// Per-account fetch use case and error composite (MKT-132).
pub mod account;
/// All-accounts fetch use case and error composite (MKT-122).
pub mod all;
/// Tauri command handlers for fetch tasks.
pub mod api;
/// Background task dispatcher — runs per-asset HTTP fetch + upsert.
pub mod dispatcher;
/// Use-case-specific failure codes shared by both composites.
pub mod error;
/// In-flight fetch guard (MKT-113) — RAII lease pattern.
pub mod guard;
#[cfg(test)]
mod serde_check;

pub use account::{FetchAccountAssetPricesError, FetchAccountAssetPricesUseCase};
pub use all::{FetchAllAssetPricesError, FetchAllAssetPricesUseCase};
pub use api::*;
pub use error::FetchPriceTask;
pub use guard::FetchGuard;
