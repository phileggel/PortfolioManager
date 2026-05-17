# Contract — Asset

> Domain: `asset`
> Last updated by: `asset` spec, `market-price` spec, `asset-web-lookup` spec, `archive-asset` use case, `delete-asset` use case

> **Error model**: every command returns `Result<T, E>` where `E` is a typed Rust enum.
> Each leaf serializes as a flat `{ code: "VariantName", ...payload }` shape via `#[serde(tag = "code")]`.
> Composite enums use `#[serde(untagged)]` to flatten their leaves into a single FE-visible union.
> All infrastructure failures translate at the application layer to per-BC `*ApplicationError::DatabaseError`
> (unit variant — no payload on the wire; the diagnostic chain is preserved server-side via `tracing::error!`).
>
> **Composites by command surface**:
>
> - **Asset read** (`get_assets`, `get_assets_with_archived`) → `AssetApplicationError` directly (single-leaf surface)
> - **Asset CRUD writes** (`add_asset`, `update_asset`, `unarchive_asset`) → `AssetCrudError` = `AssetApplicationError | AssetDomainError | CategoryApplicationError`
> - **Category read** (`get_categories`) → `CategoryApplicationError` directly
> - **Category CRUD writes** (`add_category`, `update_category`, `delete_category`) → `CategoryCrudError` = `CategoryApplicationError | CategoryDomainError`
> - **Asset prices** (`record_asset_price`, `get_asset_prices`, `update_asset_price`, `delete_asset_price`) → `AssetPriceError` = `AssetApplicationError | AssetPriceApplicationError | AssetPriceDomainError`
> - **Archive use case** (`archive_asset`) → `ArchiveAssetError` = `AssetCrudError | AccountApplicationError | ArchiveAssetApplicationError`
> - **Delete use case** (`delete_asset`) → `DeleteAssetError` = `AssetCrudError | AccountApplicationError | DeleteAssetApplicationError`
> - **Web lookup** (`lookup_asset`) → `WebLookupApplicationError` (single variant: `NetworkError`)
>
> **Leaf variants** (full set; per-command reachable subsets are in the tables below):
>
> - `AssetApplicationError`: `NotFound { id }`, `DatabaseError`
> - `AssetDomainError`: `NameEmpty`, `ReferenceEmpty`, `InvalidRiskLevel { received }`, `InvalidCurrency { currency }`, `Archived`, `CashAssetNotEditable` (CSH-016)
> - `CategoryApplicationError`: `NotFound { id }`, `DuplicateName`, `DatabaseError`
> - `CategoryDomainError`: `LabelEmpty`, `SystemReadonly`, `SystemProtected`
> - `AssetPriceApplicationError`: `PriceNotFound { asset_id, date }`, `DatabaseError`
> - `AssetPriceDomainError`: `NotPositive`, `NonFinite`, `DateInFuture`, `InvalidDateFormat { date }`
> - `ArchiveAssetApplicationError`: `ActiveHoldings`
> - `DeleteAssetApplicationError`: `ExistingTransactions`
> - `WebLookupApplicationError`: `NetworkError`

---

## Commands

### Asset CRUD

| Command                    | Args                                                                                                                                              | Return       | Error type              | Reachable codes                                                                                                                                                                                                                                                                                                           |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_assets`               | —                                                                                                                                                 | `Vec<Asset>` | `AssetApplicationError` | `DatabaseError` _(returns active assets only)_                                                                                                                                                                                                                                                                            |
| `get_assets_with_archived` | —                                                                                                                                                 | `Vec<Asset>` | `AssetApplicationError` | `DatabaseError` _(returns all assets including archived)_                                                                                                                                                                                                                                                                 |
| `add_asset`                | `CreateAssetDTO { name: String, class: AssetClass, category_id: String, currency: String, risk_level: i32, reference: String }`                   | `Asset`      | `AssetCrudError`        | `NameEmpty (R1)`, `ReferenceEmpty (R1)`, `InvalidRiskLevel { received } (AST-002)`, `InvalidCurrency { currency } (TRX-021)`, `NotFound { id } (CategoryApplicationError — when category_id missing)`, `DatabaseError`                                                                                                    |
| `update_asset`             | `UpdateAssetDTO { asset_id: String, name: String, reference: String, class: AssetClass, currency: String, risk_level: i32, category_id: String }` | `Asset`      | `AssetCrudError`        | `NotFound { id } (AssetApplicationError)`, `Archived (R18 — archived asset cannot be edited)`, `CashAssetNotEditable (CSH-016)`, `NameEmpty`, `ReferenceEmpty`, `InvalidRiskLevel { received }`, `InvalidCurrency { currency }`, `NotFound { id } (CategoryApplicationError — when category_id missing)`, `DatabaseError` |
| `unarchive_asset`          | `id: String`                                                                                                                                      | `()`         | `AssetCrudError`        | `NotFound { id } (AssetApplicationError)`, `CashAssetNotEditable (CSH-016)`, `DatabaseError`                                                                                                                                                                                                                              |

### Categories

> All category commands are owned by the asset BC. The system default category cannot be renamed (`SystemReadonly`) or deleted (`SystemProtected`).

| Command           | Args                        | Return               | Error type                 | Reachable codes                                                                               |
| ----------------- | --------------------------- | -------------------- | -------------------------- | --------------------------------------------------------------------------------------------- |
| `get_categories`  | —                           | `Vec<AssetCategory>` | `CategoryApplicationError` | `DatabaseError` _(read-only; `NotFound` and `DuplicateName` are unreachable on a list query)_ |
| `add_category`    | `label: String`             | `AssetCategory`      | `CategoryCrudError`        | `LabelEmpty`, `DuplicateName`, `DatabaseError`                                                |
| `update_category` | `id: String, label: String` | `AssetCategory`      | `CategoryCrudError`        | `NotFound { id }`, `LabelEmpty`, `DuplicateName`, `SystemReadonly`, `DatabaseError`           |
| `delete_category` | `id: String`                | `()`                 | `CategoryCrudError`        | `NotFound { id }`, `SystemProtected`, `DatabaseError`                                         |

### Archive / Delete (use cases)

> Both live in `use_cases/{archive_asset,delete_asset}/`. Each composes the asset BC's `AssetCrudError` (carrying the asset-existence check + the cash-asset guard), an `AccountApplicationError` leaf (the cross-BC check that no active holdings or transactions reference the asset), and a use-case-owned application error for the orchestrator's verdict.

| Command         | Args         | Return | Error type          | Reachable codes                                                                                                                                                                                                          |
| --------------- | ------------ | ------ | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `archive_asset` | `id: String` | `()`   | `ArchiveAssetError` | `NotFound { id } (AssetApplicationError)`, `CashAssetNotEditable (AssetDomainError, CSH-016)`, `DatabaseError (AssetApplicationError or AccountApplicationError)`, `ActiveHoldings (ArchiveAssetApplicationError, OQ-6)` |
| `delete_asset`  | `id: String` | `()`   | `DeleteAssetError`  | `NotFound { id } (AssetApplicationError)`, `CashAssetNotEditable (AssetDomainError, CSH-016)`, `DatabaseError (AssetApplicationError or AccountApplicationError)`, `ExistingTransactions (DeleteAssetApplicationError)`  |

### Asset Prices

| Command              | Args                                                                        | Return            | Error type        | Reachable codes                                                                                                                                                             |
| -------------------- | --------------------------------------------------------------------------- | ----------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `record_asset_price` | `asset_id: String, date: String, price: f64`                                | `()`              | `AssetPriceError` | `NotFound { id } (AssetApplicationError, MKT-043)`, `NotPositive (MKT-021)`, `NonFinite (MKT-021)`, `DateInFuture (MKT-022)`, `InvalidDateFormat { date }`, `DatabaseError` |
| `get_asset_prices`   | `asset_id: String`                                                          | `Vec<AssetPrice>` | `AssetPriceError` | `NotFound { id } (AssetApplicationError, MKT-072)`, `DatabaseError`                                                                                                         |
| `update_asset_price` | `asset_id: String, original_date: String, new_date: String, new_price: f64` | `()`              | `AssetPriceError` | `PriceNotFound { asset_id, date } (MKT-083)`, `NotPositive (MKT-082)`, `NonFinite (MKT-082)`, `DateInFuture (MKT-082)`, `InvalidDateFormat { date }`, `DatabaseError`       |
| `delete_asset_price` | `asset_id: String, date: String`                                            | `()`              | `AssetPriceError` | `PriceNotFound { asset_id, date } (MKT-090)`, `DatabaseError`                                                                                                               |

### Web Lookup

> `lookup_asset` is implemented in `use_cases/asset_web_lookup/` — it reads from an external web
> API and returns transient value objects; it does not persist anything. Owned here as the asset
> aggregate is the primary subject.

| Command        | Args            | Return                   | Error type                  | Reachable codes          |
| -------------- | --------------- | ------------------------ | --------------------------- | ------------------------ |
| `lookup_asset` | `query: String` | `Vec<AssetLookupResult>` | `WebLookupApplicationError` | `NetworkError (WEB-025)` |

---

## Shared Types

```rust
struct Asset {
    id: String,
    name: String,
    class: AssetClass,           // AST-003
    category: AssetCategory,     // nested category (id + name); read responses include the resolved category, not just the id
    currency: String,            // ISO 4217 (TRX-021)
    risk_level: u8,              // 1..=5 (AST-002)
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
    asset_id: String,  // asset this price belongs to
    date: String,      // ISO 8601 calendar date (e.g. "2026-04-29")
    price: i64,        // market price in asset's native currency, i64 micros (ADR-001)
}
```

---

## Events

| Event               | Payload | Direction                                                                                                                                                                                                                                                         |
| ------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AssetUpdated`      | none    | published — fired after any successful asset CRUD write or archive/unarchive/delete (R18, R23)                                                                                                                                                                    |
| `AssetPriceUpdated` | none    | published — fired after successful `record_asset_price` (MKT-026), `update_asset_price` (MKT-085), `delete_asset_price` (MKT-091), or auto-record on `buy_holding`/`sell_holding`/`correct_transaction` when `record_price = true` and `unit_price > 0` (MKT-057) |

---

## Changelog

- 2026-04-26 — Added by `market-price` spec: `record_asset_price`
- 2026-04-26 — Typed errors: all commands now return discriminated-union enums instead of `String`
- 2026-04-26 — Added `CategoryNotFound` (now reachable via `CategoryApplicationError::NotFound { id }` propagated through `AssetCrudError`)
- 2026-04-27 — Updated by `market-price` spec (MKT-050+): `AssetPriceUpdated` event now also fires from the auto-record path on `add_transaction` / `update_transaction`; no new commands
- 2026-04-29 — Added by `market-price` spec (MKT-070+): `get_asset_prices`, `update_asset_price`, `delete_asset_price`; `AssetPrice` shared type
- 2026-05-03 — Merged from `asset_web_lookup-contract.md`: `lookup_asset`; added `AssetLookupResult` shared type
- 2026-05-03 — WEB-048/049: added `exchange` field to `AssetLookupResult`; added `Derivatives` AssetClass variant (AST-003); WEB-023 extended to map Warrant/Option/Future/Rights → Derivatives
- 2026-05-06 — CSH-016: added `CashAssetNotEditable` to `AssetDomainError`; `archive_asset` and `delete_asset` now also surface `NotFound` (commands now load the asset to enforce the cash guard)
- 2026-05-11 — Refreshed contract after the 13-PR error-model arc: replaced legacy `*CommandError` boundary types with the current composite shape (`AssetCrudError`, `CategoryCrudError`, `AssetPriceError`, `ArchiveAssetError`, `DeleteAssetError`) and per-leaf typed enums; renamed wire variants `Unknown` → `DatabaseError`; added previously-undocumented commands `get_assets`, `get_assets_with_archived`, `add_asset`, `update_asset`, `unarchive_asset`, `get_categories`, `add_category`, `update_category`, `delete_category`, `archive_asset`, `delete_asset`; per-command tables now show both the error type and the reachable code subset; corrected `Asset` shared type — read shape carries nested `category: AssetCategory`, not `category_id: String` (the FK form is on the write DTOs only); fixed `risk_level` type `i32` → `u8` to match Rust + bindings.
