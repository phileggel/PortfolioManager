/// External API and Tauri commands.
mod api;
/// Application-layer types (errors raised by service / use cases).
mod application;
/// Core business entities and repository traits.
mod domain;
/// Data persistence implementations.
mod repository;
/// Coordination layer for business operations.
mod service;

pub use api::*;
pub use application::{
    AssetApplicationError, AssetCrudError, AssetPriceApplicationError, AssetPriceError,
    CategoryApplicationError, CategoryCrudError,
};
pub use domain::*;
pub use repository::*;
pub use service::*;
