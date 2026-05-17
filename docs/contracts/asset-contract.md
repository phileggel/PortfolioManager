# Contract — Asset

> Domain: `asset`
> Last updated by: `asset` spec, `market-price` spec (auto-fetch amendment), `asset-web-lookup` spec, `archive-asset` use case, `delete-asset` use case

> **Error model on the wire**: each command's error serializes as a flat `{ code: "VariantName", ...payload }` object. The FE matches on `code`. Per-command reachable codes are listed in the "Errors" column of each table below. Infrastructure failures surface as `{ code: "DatabaseError" }` (no payload; diagnostic chain preserved server-side via `tracing::error!`).
>
> Rust-internal type organization (per-BC enums, use-case composites, serde tagging) is out of scope for this contract — it documents the BE↔FE frontier, not Rust internals.

---

## Commands

### Asset CRUD

| Command                    | Args                                                                                                                                              | Return       | Errors                                                                                                                                                                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_assets`               | —                                                                                                                                                 | `Vec<Asset>` | `DatabaseError` _(returns active assets only)_                                                                                                                                                                                                            |
| `get_assets_with_archived` | —                                                                                                                                                 | `Vec<Asset>` | `DatabaseError` _(returns all assets including archived)_                                                                                                                                                                                                 |
| `add_asset`                | `CreateAssetDTO { name: String, class: AssetClass, category_id: String, currency: String, risk_level: i32, reference: String }`                   | `Asset`      | `NameEmpty` (R1), `ReferenceEmpty` (R1), `InvalidRiskLevel { received: i32 }` (AST-002), `InvalidCurrency { currency }` (TRX-021), `NotFound { id }` (when `category_id` missing), `DatabaseError`                                                        |
| `update_asset`             | `UpdateAssetDTO { asset_id: String, name: String, reference: String, class: AssetClass, currency: String, risk_level: i32, category_id: String }` | `Asset`      | `NotFound { id }` (asset or category missing), `Archived` (R18 — archived asset cannot be edited), `CashAssetNotEditable` (CSH-016), `NameEmpty`, `ReferenceEmpty`, `InvalidRiskLevel { received: i32 }`, `InvalidCurrency { currency }`, `DatabaseError` |
| `unarchive_asset`          | `id: String`                                                                                                                                      | `()`         | `NotFound { id }`, `CashAssetNotEditable` (CSH-016), `DatabaseError`                                                                                                                                                                                      |

### Categories

> All category commands are owned by the asset BC. The system default category cannot be renamed (`SystemReadonly`) or deleted (`SystemProtected`).

| Command           | Args                        | Return               | Errors                                                                              |
| ----------------- | --------------------------- | -------------------- | ----------------------------------------------------------------------------------- |
| `get_categories`  | —                           | `Vec<AssetCategory>` | `DatabaseError` _(read-only)_                                                       |
| `add_category`    | `label: String`             | `AssetCategory`      | `LabelEmpty`, `DuplicateName`, `DatabaseError`                                      |
| `update_category` | `id: String, label: String` | `AssetCategory`      | `NotFound { id }`, `LabelEmpty`, `DuplicateName`, `SystemReadonly`, `DatabaseError` |
| `delete_category` | `id: String`                | `()`                 | `NotFound { id }`, `SystemProtected`, `DatabaseError`                               |

### Archive / Delete (use cases)

> Both are cross-BC use cases that check asset existence + cash-asset guard, then check the account BC for holdings/transactions referencing the asset.

| Command         | Args         | Return | Errors                                                                                        |
| --------------- | ------------ | ------ | --------------------------------------------------------------------------------------------- |
| `archive_asset` | `id: String` | `()`   | `NotFound { id }`, `CashAssetNotEditable` (CSH-016), `ActiveHoldings` (OQ-6), `DatabaseError` |
| `delete_asset`  | `id: String` | `()`   | `NotFound { id }`, `CashAssetNotEditable` (CSH-016), `ExistingTransactions`, `DatabaseError`  |

### Asset Prices

> All `date` / `original_date` / `new_date` arguments use ISO 8601 calendar format (e.g. `"2026-04-29"`), matching the `AssetPrice.date` shared-type convention.

| Command              | Args                                                                        | Return            | Errors                                                                                                                                                                |
| -------------------- | --------------------------------------------------------------------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `record_asset_price` | `asset_id: String, date: String, price: f64`                                | `()`              | `NotFound { id }` (MKT-043), `NotPositive` (MKT-021), `NonFinite` (MKT-021), `DateInFuture` (MKT-022), `InvalidDateFormat { date }`, `DatabaseError`                  |
| `get_asset_prices`   | `asset_id: String`                                                          | `Vec<AssetPrice>` | `NotFound { id }` (MKT-072), `DatabaseError`                                                                                                                          |
| `update_asset_price` | `asset_id: String, original_date: String, new_date: String, new_price: f64` | `()`              | `PriceNotFound { asset_id, date }` (MKT-083), `NotPositive` (MKT-082), `NonFinite` (MKT-082), `DateInFuture` (MKT-082), `InvalidDateFormat { date }`, `DatabaseError` |
| `delete_asset_price` | `asset_id: String, date: String`                                            | `()`              | `PriceNotFound { asset_id, date }` (MKT-090), `DatabaseError`                                                                                                         |

### Asset Price Fetch Tasks

> `fetch_all_asset_prices` is the single BE entry point shared by auto-fetch on launch (MKT-121, MKT-122) and global refresh on the dashboard (MKT-130). Both commands are acknowledged synchronously (return `()` once dispatched); per-asset results are signaled asynchronously via `AssetPriceUpdated` (MKT-112). Per-asset failures during the fetch degrade silently per MKT-114; the in-flight guard (MKT-113) rejects concurrent calls across both commands. System cash assets are excluded from scope per MKT-116.

| Command                      | Args                 | Return | Errors                                                                                                                                        |
| ---------------------------- | -------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `fetch_all_asset_prices`     | —                    | `()`   | `FetchAlreadyRunning` (MKT-113), `NoFetchableHoldings` (MKT-111), `DatabaseError`, `UnknownError`                                             |
| `fetch_account_asset_prices` | `account_id: String` | `()`   | `AccountNotFound { account_id }` (MKT-132), `FetchAlreadyRunning` (MKT-113), `NoFetchableHoldings` (MKT-111), `DatabaseError`, `UnknownError` |

### Web Lookup

> `lookup_asset` is implemented in `use_cases/asset_web_lookup/` — it reads from an external web
> API and returns transient value objects; it does not persist anything. Owned here as the asset
> aggregate is the primary subject.

| Command        | Args            | Return                   | Errors                   |
| -------------- | --------------- | ------------------------ | ------------------------ |
| `lookup_asset` | `query: String` | `Vec<AssetLookupResult>` | `NetworkError` (WEB-025) |

---

## Shared Types

```rust
struct Asset {
    id: String,
    name: String,
    class: AssetClass,           // AST-003
    category: AssetCategory,     // nested category (id + name); read responses include the resolved category, not just the id
    currency: String,            // ISO 4217 (TRX-021)
    risk_level: u8,              // 1..=5 (AST-002); DTOs accept `i32` input and the backend validates with `InvalidRiskLevel { received: i32 }`
    reference: String,           // ticker / ISIN / freeform reference (mandatory — R1)
    is_archived: bool,           // R18
}

struct AssetCategory {
    id: String,
    name: String,
}

// Note on write DTOs: CreateAssetDTO and UpdateAssetDTO carry `category_id: String`
// (the FK), not the nested `AssetCategory`. The service resolves the id to the
// aggregate at write time and the read shape returns the resolved category.
```

```rust
// AssetClass variants (AST-003) — Derivatives added by WEB-023
enum AssetClass { Cash, Bonds, RealEstate, MutualFunds, ETF, Stocks, DigitalAsset, Derivatives }
// Derivatives maps from securityType: "Warrant" | "Option" | "Future" | "Rights" (WEB-023)
// default_risk for Derivatives = 5 (AST-003)
```

```rust
// Transient value object — not persisted (WEB-020)
// Fields marked "optional" may be absent per spec rules
struct AssetLookupResult {
    name: String,
    reference: Option<String>,       // absent for keyword results with no ticker (WEB-046)
    currency: Option<String>,        // absent when OpenFIGI returns no currency (WEB-024)
    asset_class: Option<AssetClass>, // absent when securityType unrecognised (WEB-023)
    exchange: Option<String>,        // human-readable market name from exchCode; absent when OpenFIGI returns none (WEB-049)
}
```

```rust
// Input prices are transmitted as f64 decimal; backend converts to i64 micros at the IPC boundary (MKT-024).
// ADR-001 (i64 micros) applies to storage and read responses — f64 on write input is the intentional
// transport-layer exception; the f64 → i64 conversion inside the command handler is the ADR-001 compliance point.

struct AssetPrice {
    asset_id: String,            // asset this price belongs to
    date: String,                // ISO 8601 calendar date (e.g. "2026-04-29")
    price: i64,                  // market price in asset's native currency, i64 micros (ADR-001)
    source: AssetPriceSource,    // MKT-100 — provenance qualifier
}
```

```rust
// AssetPriceSource variants (MKT-100) — Finnhub reserved for the KEY spec
enum AssetPriceSource { Manual, Stooq }
// Manual: every user-driven write — manual entry (MKT-020+), transaction
//         auto-record follow-up (MKT-050+), price-history edit (MKT-083+);
//         set by record_asset_price / update_asset_price per MKT-101.
// Stooq:  fetch-task write (fetch_all_asset_prices, fetch_account_asset_prices)
//         per MKT-102.
```

---

## Events

| Event               | Payload | Direction                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AssetUpdated`      | none    | published — fired after any successful asset CRUD write or archive/unarchive/delete (R18, R23)                                                                                                                                                                                                                                                                                                                         |
| `AssetPriceUpdated` | none    | published — fired after successful `record_asset_price` (MKT-026), `update_asset_price` (MKT-085), `delete_asset_price` (MKT-091), or per-asset write success during a fetch task — `fetch_all_asset_prices` / `fetch_account_asset_prices` (MKT-112). The transaction auto-record path (MKT-055/057) emits via the same `record_asset_price` call the FE issues after the transaction commits — no separate producer. |
