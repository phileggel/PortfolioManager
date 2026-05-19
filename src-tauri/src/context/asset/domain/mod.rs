/// Asset aggregate and repository trait.
pub mod asset;
/// AssetPrice aggregate, repository trait, AssetPriceSource, and PriceProvider trait.
pub mod asset_price;
/// AssetCategory aggregate and repository trait.
pub mod category;
/// Typed error enums for the asset domain.
pub mod error;
/// Canonical trading venue value object (Exchange) and curated set (AST-021).
pub mod exchange;
/// OpenFIGI inbound exchange mapper — `micCode` / `exchCode` → `Exchange` (WEB-049).
pub mod openfigi_exchange_mapper;
/// Stooq outbound exchange mapper — `Exchange` → Stooq suffix (MKT-110).
pub mod stooq_exchange_mapper;
/// Stooq provider symbol derivation from asset reference (MKT-110, ADR-008).
pub mod stooq_symbol;

pub use asset::*;
pub use asset_price::{AssetPrice, AssetPriceRepository, AssetPriceSource, PriceProvider};
pub use category::*;
pub use error::{AssetDomainError, AssetPriceDomainError, CategoryDomainError};
pub use exchange::Exchange;
pub use stooq_symbol::{derive_stooq_symbol, derive_stooq_symbol_with_exchange};

#[cfg(test)]
pub use asset::MockAssetRepository;
#[cfg(test)]
pub use asset_price::{MockAssetPriceRepository, MockPriceProvider};
#[cfg(test)]
pub use category::MockAssetCategoryRepository;
