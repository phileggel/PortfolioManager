/// Asset persistence logic.
mod asset;
/// Asset price persistence logic.
mod asset_price;
/// Asset category persistence logic.
mod category;
/// Stooq HTTP price provider (ADR-008).
mod stooq_client;

pub use asset::SqliteAssetRepository;
pub use asset_price::SqliteAssetPriceRepository;
pub use category::SqliteAssetCategoryRepository;
pub use stooq_client::ReqwestStooqClient;
