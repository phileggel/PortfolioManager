# Ubiquitous Language

The authoritative dictionary of domain terms for this project.

> ⚠️ **Every addition or change to this document MUST be individually reviewed and validated
> by the user before it is considered confirmed.** Agents may propose additions (using status
> `confirmed`) but MUST NOT mark any term or entry as `confirmed` without explicit user approval.

**Rules:**

- All terms MUST be agreed with the user before use in code, tests, or docs.
- The agent MUST NOT invent or assume domain terms — propose and wait for confirmation.
- Terms marked `confirmed` are proposals awaiting user validation.
- Once confirmed, the term MUST be used consistently everywhere (code, comments, specs, UI).

---

## Account Context

### Account

The top-level aggregate root. Represents a financial account (e.g. brokerage, savings).
Owns all holdings and their transaction history.

> Status: confirmed

### Holding

An internal entity of `Account`. Represents the current position in a given asset within
an account — quantity held, average price (VWAP), and realized P&L.

> Status: confirmed

### Transaction

An internal entity of `Account`. A single financial event — a purchase or a sale — with
date, quantity, price, fees, and exchange rate. Owned directly by `Account` alongside
`Holding`; a transaction affects its corresponding holding but is not nested inside it.

> Status: confirmed

---

## Asset Context

### Asset

The aggregate root of the asset context. Represents a financial instrument
(stock, ETF, bond, etc.) with a currency, class, category, and risk level.

> Status: confirmed

### AssetPrice

An internal entity of `Asset`. A price observation for an asset on a given date, with a `source` field (see `AssetPriceSource`) qualifying its provenance.

> Status: confirmed

### AssetPriceSource

A value-object enum qualifying the provenance of an `AssetPrice` record. Variants in v1: `Manual` (user-entered via manual entry or transaction auto-record), `Stooq` (auto-fetched from the Stooq provider). `Finnhub` reserved for the KEY spec. Metadata for traceability per ADR-012 — does not influence read/write precedence (latest-write-wins).

> Status: confirmed

### Exchange

A canonical reference to a trading venue, independent of any market-data provider. Carries an ISO 10383 Market Identifier Code (MIC) as `code` (e.g. `XPAR`, `XNAS`) and a human-readable `label`. Optional field on `Asset`. Auto-filled by the OpenFIGI lookup path (WEB-049) or selected by the user via a curated picker on the Add/Edit Asset form (AST-021). Used by the auto-fetch task to resolve the Stooq provider symbol (MKT-110). Provider keys (Stooq venue suffixes, OpenFIGI exchange codes) are NOT stored on `Exchange` — they are resolved by per-provider mappers at the boundary.

> Status: confirmed

---

## Aggregate Root Methods (Account)

| Name                  | Intent                                                                                                  | Status    |
| --------------------- | ------------------------------------------------------------------------------------------------------- | --------- |
| `buy_holding`         | Record a purchase of an asset into the account                                                          | confirmed |
| `sell_holding`        | Record a sale of an asset from the account                                                              | confirmed |
| `correct_transaction` | Correct the fields of an existing transaction (cascades VWAP/P&L recalculation on the affected holding) | confirmed |
| `cancel_transaction`  | Delete an existing transaction (cascades VWAP/P&L recalculation or holding removal)                     | confirmed |
| `open_holding`        | Seed an existing position with a quantity and total cost at a given date, without full purchase history | confirmed |

## Transaction Types

| Name             | Intent                                                                               | Status    |
| ---------------- | ------------------------------------------------------------------------------------ | --------- |
| `Purchase`       | A regular buy transaction — quantity, unit price, exchange rate, fees                | confirmed |
| `Sell`           | A regular sell transaction — quantity, unit price, exchange rate, fees, realized P&L | confirmed |
| `OpeningBalance` | A position seed entry — quantity and total cost paid directly, no fee breakdown      | confirmed |
| `Deposit`        | A cash inflow from outside the application's tracked world (CSH-022)                 | confirmed |
| `Withdrawal`     | A cash outflow to outside the application's tracked world (CSH-032)                  | confirmed |

## Cash Domain Concepts (introduced by CSH spec)

### Cash Asset

> Status: confirmed

A system-seeded `Asset` of `class = AssetClass::Cash`, one per ISO currency, with deterministic id `system-cash-{ccy}`. Acts as the asset reference for cash positions. Not user-editable, not user-creatable, not displayed in the asset catalog.

### Cash Holding

> Status: confirmed

A `Holding` whose asset is a Cash Asset. Represents the cash balance held in the account in the account's reference currency. At most one Cash Holding per `(account_id, account.currency)`. Lazy-created on first Deposit or Sell; cleaned up by TRX-034 when no cash-affecting transactions remain.

### Global Value

> Status: confirmed

The full economic value of an account: cash balance + Σ (market value of non-cash active holdings). Surfaced as `AccountDetailsResponse.total_global_value` (CSH-094). Used as the canonical "what is this account worth right now?" metric across the Account Details header and (later) the portfolio dashboard.

---

## Domain Events

| Name                 | Raised by  | Intent                                                      | Status    |
| -------------------- | ---------- | ----------------------------------------------------------- | --------- |
| `AccountUpdated`     | Account BC | Any state change in the account or its holdings             | confirmed |
| `TransactionUpdated` | Account BC | A holding or transaction was created, updated, or cancelled | confirmed |
| `AssetUpdated`       | Asset BC   | Any state change in an asset or category                    | confirmed |
| `CategoryUpdated`    | Asset BC   | Any state change in a category                              | confirmed |
| `AssetPriceUpdated`  | Asset BC   | An AssetPrice record was created, updated, or deleted       | confirmed |

---

## Asset Web Lookup

### OpenFIGI Lookup

The outbound HTTP search that, given a name, ticker, or ISIN, queries the OpenFIGI API and
returns up to 10 candidate `AssetLookupResult` values. 12-character alphanumeric inputs route
to the ISIN mapping endpoint; all others route to the keyword search endpoint.

> Status: confirmed

### AssetLookupResult

A transient value object returned by the OpenFIGI lookup. Never persisted. Carries the name,
reference (ISIN or ticker), currency, and asset class of a candidate instrument — used solely
to pre-fill the Add Asset form.

> Status: confirmed

## Asset Web Lookup Command

| Name           | Intent                                                                                                             | Status    |
| -------------- | ------------------------------------------------------------------------------------------------------------------ | --------- |
| `lookup_asset` | Query OpenFIGI with a name, ticker, or ISIN and return up to 10 `AssetLookupResult` values. Errors: `NetworkError` | confirmed |

---

## Asset Price Fetch Tasks

### Fetch task

A backend job that retrieves current prices from an external provider and upserts `AssetPrice` records. Umbrella term for the three named instances below.

> Status: confirmed

### Auto-fetch

A fetch task triggered automatically at application launch when the user has enabled the auto-fetch setting. Scope: all active holdings across all accounts.

> Status: confirmed

### Global refresh

A fetch task triggered manually by the user from the global dashboard. Scope: all active holdings across all accounts. Shares the same backend entry point as auto-fetch.

> Status: confirmed

### Account refresh

A fetch task triggered manually by the user from an account detail page. Scope: active holdings of the specified account.

> Status: confirmed

### External provider

A third-party HTTP service that returns current asset prices, configured per ADR-008. v1: Stooq.

> Status: confirmed

---

## Asset Price Service Operations

| Name                 | Intent                                                                                                                                                                                | Status    |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| `record_asset_price` | Create or overwrite the price for an asset on a given date (upsert by `(asset_id, date)`). Errors: `AssetNotFound`, `NotPositive`, `NonFinite`, `DateInFuture`, `Unknown`             | confirmed |
| `get_asset_prices`   | Return all recorded prices for an asset, ordered by date descending. Errors: `AssetNotFound`, `Unknown`                                                                               | confirmed |
| `update_asset_price` | Change the date and/or price of an existing price record; atomic delete-old + upsert-new when date changes. Errors: `NotFound`, `NotPositive`, `NonFinite`, `DateInFuture`, `Unknown` | confirmed |
| `delete_asset_price` | Remove a specific price record by `(asset_id, date)`. Errors: `NotFound`, `Unknown`                                                                                                   | confirmed |
