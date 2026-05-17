//! Application use cases layer.
//!
//! Cross-cutting application use cases that orchestrate multiple bounded
//! contexts or platform capabilities.

/// Account Deletion: pre-deletion summary of holdings and transactions (ACC-020).
pub mod account_deletion;
/// Account Details: cross-context read of holdings + asset metadata (ACD feature).
pub mod account_details;
/// Archive asset: guards archiving against active holdings across bounded contexts (OQ-6).
pub mod archive_asset;
/// Asset price auto-fetch: retrieves prices from Stooq on launch and user demand (MKT-100+).
pub mod asset_price_fetch;
/// Asset Web Lookup: OpenFIGI search to pre-fill the Add Asset form (WEB).
pub mod asset_web_lookup;
/// Delete asset: guards hard-deletion against existing transactions.
pub mod delete_asset;
/// Holding transaction: unified cross-BC orchestrators for buy/sell/correct/cancel/open (TRX, SEL, CSH).
pub mod holding_transaction;
/// Application auto-update: detection, download, and installation (R1–R27).
pub mod update_checker;
