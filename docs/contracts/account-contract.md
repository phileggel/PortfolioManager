# Contract — Account

> Domain: account
> Last updated by: account spec, financial-asset-transaction spec, sell-transaction spec, transaction-list spec, account-details spec, cash-tracking spec

> **Error model**: every command returns `Result<T, E>` where `E` is a typed Rust enum.
> Each leaf serializes as a flat `{ code: "VariantName", ...payload }` shape via `#[serde(tag = "code")]`.
> Composite enums use `#[serde(untagged)]` to flatten their leaves into a single FE-visible union.
> All infrastructure failures translate at the application layer to `AccountApplicationError::DatabaseError`
> (unit variant — no payload on the wire; the diagnostic chain is preserved server-side via `tracing::error!`).
>
> **Composites by command surface**:
>
> - **Account CRUD** (`add_account`, `update_account`) → `AccountCrudError` = `AccountApplicationError | AccountDomainError`
> - **Account read / delete** (`get_accounts`, `delete_account`, `get_asset_ids_for_account`, `get_transactions`, `get_account_deletion_summary`, `get_account_details`) → `AccountApplicationError` directly (single-leaf surface)
> - **Holding transactions** (`buy_holding`, `sell_holding`, `correct_transaction`, `cancel_transaction`, `record_deposit`, `record_withdrawal`) → `HoldingTransactionError` = `AccountApplicationError | AccountOperationError | TransactionDomainError`
> - **Open holding** (`open_holding`) → `OpenHoldingError` = `AccountApplicationError | OpenHoldingApplicationError | OpeningBalanceDomainError | TransactionDomainError`
>
> **Leaf variants** (full set; per-command reachable subsets are in the tables below):
>
> - `AccountApplicationError`: `AccountNotFound { account_id }`, `NameAlreadyExists`, `DatabaseError`
> - `AccountDomainError`: `NameEmpty`, `InvalidCurrency { currency }`
> - `AccountOperationError`: `ClosedPosition`, `Oversell { available, requested }`, `CascadingOversell`, `TransactionNotFound`, `InsufficientCash { current_balance_micros, currency }`
> - `TransactionDomainError`: `InvalidDate`, `DateInFuture`, `DateTooOld`, `QuantityNotPositive`, `AmountNotPositive`, `UnitPriceNegative`, `FeesNegative`, `ExchangeRateNotPositive`, `TotalAmountNotPositive`
> - `OpenHoldingApplicationError`: `AssetNotFound`, `ArchivedAsset`, `OpeningBalanceOnCashAsset`
> - `OpeningBalanceDomainError`: `InvalidTotalCost`

## Commands

### Account CRUD

| Command                        | Args                                                                                                 | Return                   | Error type                | Reachable codes                                                                                                                            |
| ------------------------------ | ---------------------------------------------------------------------------------------------------- | ------------------------ | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `get_accounts`                 | —                                                                                                    | `Vec<Account>`           | `AccountApplicationError` | `DatabaseError`                                                                                                                            |
| `add_account`                  | `CreateAccountDTO { name: String, currency: String, update_frequency: UpdateFrequency }`             | `Account`                | `AccountCrudError`        | `NameEmpty (ACC-002)`, `NameAlreadyExists (ACC-003)`, `InvalidCurrency { currency } (TRX-021)`, `DatabaseError`                            |
| `update_account`               | `UpdateAccountDTO { id: String, name: String, currency: String, update_frequency: UpdateFrequency }` | `Account`                | `AccountCrudError`        | `NameEmpty (ACC-002)`, `NameAlreadyExists (ACC-003)`, `InvalidCurrency { currency } (TRX-021)`, `DatabaseError`                            |
| `delete_account`               | `id: String`                                                                                         | `()`                     | `AccountApplicationError` | `DatabaseError (ACC-005, ACC-006)` _(no NotFound — plain DELETE, silent on missing row)_                                                   |
| `get_account_deletion_summary` | `account_id: String`                                                                                 | `AccountDeletionSummary` | `AccountApplicationError` | `DatabaseError` _(read-only; counts are 0 if account has no data — no NotFound raised)_                                                    |

### Account Details

> `get_account_details` is implemented in `use_cases/account_details/` — it reads from both the
> account and asset BCs but mutates neither; owned here as the account aggregate is the primary subject.

| Command               | Args                 | Return                   | Error type                | Reachable codes                                                                                                            |
| --------------------- | -------------------- | ------------------------ | ------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `get_account_details` | `account_id: String` | `AccountDetailsResponse` | `AccountApplicationError` | `AccountNotFound { account_id } (ACD-012)`, `DatabaseError (ACD-038)`; price lookup failures silently degrade to `None` (MKT-031) |

### Holdings & Transactions

> Commands below split between two locations:
>
> - `context/account/api.rs` — read paths only: `get_asset_ids_for_account`, `get_transactions`.
> - `use_cases/holding_transaction/api.rs` — every command that mutates a `Holding` through a `Transaction`: `buy_holding`, `sell_holding`, `correct_transaction`, `cancel_transaction`, `open_holding`. These live in a use case because the orchestrator coordinates across the account and asset BCs (cash-asset seeding, archived-asset guards, etc.).
>
> The `HoldingTransactionError` composite is owned in `context/account/application/error.rs` because every leaf is account-context-typed; the use-case commands re-export it. `OpenHoldingError` is owned in `use_cases/holding_transaction/error.rs` because its `OpenHoldingApplicationError` leaf is use-case-owned (cross-BC asset checks performed by the orchestrator).

| Command                     | Args                                   | Return             | Error type                  | Reachable codes                                                                                                                                                                                                                                                                                                                                                |
| --------------------------- | -------------------------------------- | ------------------ | --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_asset_ids_for_account` | `account_id: String`                   | `Vec<String>`      | `AccountApplicationError`   | `DatabaseError (TXL-054)` — returns empty list for unknown or empty account, never NotFound (TXL-013)                                                                                                                                                                                                                                                          |
| `get_transactions`          | `account_id: String, asset_id: String` | `Vec<Transaction>` | `AccountApplicationError`   | `DatabaseError (TXL-020)`                                                                                                                                                                                                                                                                                                                                      |
| `buy_holding`               | `BuyHoldingDTO`                        | `Transaction`      | `HoldingTransactionError`   | `AccountNotFound { account_id } (TRX-020)`, `InvalidDate (TRX-020)`, `DateInFuture (TRX-020)`, `DateTooOld (TRX-020)`, `QuantityNotPositive (TRX-020)`, `ExchangeRateNotPositive (TRX-020)`, `FeesNegative (TRX-020)`, `TotalAmountNotPositive (TRX-020)`, `InsufficientCash { current_balance_micros, currency } (CSH-041)`, `DatabaseError`                                                                    |
| `sell_holding`              | `SellHoldingDTO`                       | `Transaction`      | `HoldingTransactionError`   | `AccountNotFound { account_id } (TRX-020)`, `InvalidDate (TRX-020)`, `DateInFuture (TRX-020)`, `DateTooOld (TRX-020)`, `QuantityNotPositive (TRX-020)`, `ExchangeRateNotPositive (TRX-020)`, `FeesNegative (SEL-020)`, `TotalAmountNotPositive (TRX-020)`, `ClosedPosition (SEL-012)`, `Oversell { available, requested } (SEL-021)`, `DatabaseError`                                                            |
| `correct_transaction`       | `id: String, account_id: String, CorrectTransactionDTO` | `Transaction` | `HoldingTransactionError` | `TransactionNotFound (TRX-031)`, `InvalidDate (TRX-033)`, `DateInFuture (TRX-033)`, `DateTooOld (TRX-033)`, `QuantityNotPositive (TRX-033)`, `ExchangeRateNotPositive (TRX-033)`, `FeesNegative (TRX-033)`, `TotalAmountNotPositive (TRX-033)`, `CascadingOversell (SEL-032)`, `InsufficientCash { current_balance_micros, currency } (CSH-042 / CSH-051)`, `DatabaseError`                                       |
| `cancel_transaction`        | `id: String, account_id: String`       | `()`               | `HoldingTransactionError`   | `TransactionNotFound (TRX-034)`, `CascadingOversell (SEL-033 — replay after cancel can leave a later sell oversold)`, `InsufficientCash { current_balance_micros, currency } (CSH-024 / CSH-051)`, `DatabaseError`                                                                                                                                                                                                                                  |
| `open_holding`              | `OpenHoldingDTO`                       | `Transaction`      | `OpenHoldingError`          | `AccountNotFound { account_id } (TRX-056)`, `AssetNotFound (TRX-056)`, `ArchivedAsset (TRX-050)`, `OpeningBalanceOnCashAsset (CSH-061)`, `QuantityNotPositive (TRX-044)`, `InvalidTotalCost (TRX-045)`, `DateInFuture (TRX-046)`, `DateTooOld (TRX-046)`, `DatabaseError`                                                                                       |

### Cash Transactions

> Implemented in `use_cases/holding_transaction/api.rs`. Both commands record cash-only movements (no asset selector, no unit price, no exchange rate). They return the persisted `Transaction` so the frontend can mirror the buy/sell flow's success path.
>
> **Edit / delete of Deposit and Withdrawal reuse `correct_transaction` and `cancel_transaction`** (CSH-023 / CSH-033 / CSH-024 / CSH-034) — those commands accept any `transaction_type` and run the chronological replay across all cash-affecting transactions for the account.

| Command             | Args                                                                                           | Return        | Error type                | Reachable codes                                                                                                                                                                                                |
| ------------------- | ---------------------------------------------------------------------------------------------- | ------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `record_deposit`    | `DepositDTO { account_id: String, date: String, amount_micros: i64, note: Option<String> }`    | `Transaction` | `HoldingTransactionError` | `AccountNotFound { account_id } (CSH-021)`, `AmountNotPositive (CSH-021)`, `DateInFuture (CSH-021)`, `DateTooOld (CSH-021)`, `DatabaseError`                                                                   |
| `record_withdrawal` | `WithdrawalDTO { account_id: String, date: String, amount_micros: i64, note: Option<String> }` | `Transaction` | `HoldingTransactionError` | `AccountNotFound { account_id } (CSH-031)`, `AmountNotPositive (CSH-031)`, `DateInFuture (CSH-031)`, `DateTooOld (CSH-031)`, `InsufficientCash { current_balance_micros, currency } (CSH-080)`, `DatabaseError` |

## Shared Types

```rust
struct Account {
    id: String,                          // unique identifier
    name: String,                        // user-defined display name (normalised, unique)
    currency: String,                    // ISO 4217 currency code (TRX-021)
    update_frequency: UpdateFrequency,   // how often the user plans to update data
}

enum UpdateFrequency {
    Automatic,
    ManualDay,
    ManualWeek,
    ManualMonth,
    ManualYear,
}

struct AccountDeletionSummary {
    holding_count: u32,       // active holdings in the account
    transaction_count: u32,   // transactions associated with the account
}
```

```rust
// Purchase: type is implicit in the command — no transaction_type field
struct BuyHoldingDTO {
    account_id: String,
    asset_id: String,
    date: String,           // ISO date YYYY-MM-DD
    quantity: i64,          // micro-units; strictly positive (TRX-020)
    unit_price: i64,        // micro-units, asset currency; zero or positive (TRX-020)
    exchange_rate: i64,     // micro-units; strictly positive (TRX-020)
    fees: i64,              // micro-units, account currency; zero or positive (TRX-020)
    note: Option<String>,
}

// Sell: identical fields, separate type — may diverge as sell-specific rules grow
struct SellHoldingDTO {
    account_id: String,
    asset_id: String,
    date: String,
    quantity: i64,
    unit_price: i64,
    exchange_rate: i64,
    fees: i64,              // micro-units, account currency; zero or positive (SEL-020)
    note: Option<String>,
}

// Correction: no account_id / asset_id / type — those are immutable on an existing transaction
struct CorrectTransactionDTO {
    date: String,
    quantity: i64,
    unit_price: i64,
    exchange_rate: i64,
    fees: i64,
    note: Option<String>,
}
```

> `total_amount` intentionally absent from input DTOs — computed by backend (TRX-026, SEL-023).
> `realized_pnl` intentionally absent — computed by backend (SEL-024).

```rust
// Opening balance: total_cost set directly by user; no fees, no exchange_rate (TRX-047); no note (TRX-043)
struct OpenHoldingDTO {
    account_id: String,
    asset_id: String,
    date: String,       // ISO date YYYY-MM-DD; must not be future or before 1900-01-01 (TRX-046)
    quantity: i64,      // micro-units; strictly positive (TRX-044)
    total_cost: i64,    // micro-units, account currency; strictly positive (TRX-045)
}

// Cash inflow from outside the application (CSH-020/022). Backend resolves the Cash Asset
// for `account.currency`; user does not pick an asset.
struct DepositDTO {
    account_id: String,
    date: String,           // ISO date YYYY-MM-DD; same TRX-020 / CSH-021 bounds as buy/sell
    amount_micros: i64,     // micro-units, account currency; strictly positive (CSH-021)
    note: Option<String>,
}

// Cash outflow to outside the application (CSH-030/032). Same shape as Deposit; eligibility
// (CSH-080) checked against current Cash Holding balance.
struct WithdrawalDTO {
    account_id: String,
    date: String,
    amount_micros: i64,
    note: Option<String>,
}
```

```rust
enum TransactionType {
    Purchase,
    Sell,
    OpeningBalance,  // TRX-042
    Deposit,         // CSH-022 — cash inflow
    Withdrawal,      // CSH-032 — cash outflow
}

// Returned by buy_holding, sell_holding, correct_transaction, open_holding, record_deposit,
// record_withdrawal, and get_transactions.
//
// For Deposit/Withdrawal: asset_id is always the Cash Asset for account.currency;
// quantity == total_amount; unit_price == 1_000_000 (cash is its own unit); exchange_rate ==
// 1_000_000; fees == 0 (v1); realized_pnl is None.
struct Transaction {
    id: String,
    account_id: String,
    asset_id: String,
    transaction_type: TransactionType,
    date: String,                   // ISO date YYYY-MM-DD
    quantity: i64,                  // micro-units (TRX-024)
    unit_price: i64,                // micro-units, asset currency (TRX-021)
    exchange_rate: i64,             // micro-units, asset→account rate (TRX-021)
    fees: i64,                      // micro-units, account currency
    total_amount: i64,              // micro-units, account currency — computed by backend (TRX-026, SEL-023)
    realized_pnl: Option<i64>,      // micros; Some only for Sell (SEL-024); None for Purchase/OpeningBalance
    note: Option<String>,           // optional user comment; None when absent
    created_at: String,             // ISO 8601 timestamp; chronological tie-breaking (TRX-036, SEL-024)
}
```

```rust
// Active position — quantity > 0 (ACD-020)
struct HoldingDetail {
    asset_id: String,
    asset_name: String,
    asset_reference: String,
    quantity: i64,                      // micros, always > 0
    average_price: i64,                 // micros, VWAP
    cost_basis: i64,                    // micros, quantity × average_price (ACD-023)
    realized_pnl: i64,                  // micros, cumulative from partial sells; 0 if none (SEL-042)
    asset_currency: String,             // ISO 4217 code of the asset's native currency (MKT-023)
    current_price: Option<i64>,         // micros in asset currency; None when no price ever recorded (MKT-031)
    current_price_date: Option<String>, // ISO date of the price observation; None when current_price is None (MKT-031)
    unrealized_pnl: Option<i64>,        // micros in account currency; None on currency mismatch or no price; 0 (not None) when price == avg_price (MKT-033/034)
    performance_pct: Option<i64>,       // micros (5.25% = 5_250_000); None when unrealized_pnl is None or cost_basis = 0; 0 (not None) when unrealized_pnl is 0 (MKT-035)
}

// Closed position — quantity = 0 (ACD-044)
struct ClosedHoldingDetail {
    asset_id: String,
    asset_name: String,
    asset_reference: String,
    realized_pnl: i64,      // micros, total gain/loss for this position (ACD-045)
    last_sold_date: String, // ISO date "YYYY-MM-DD"; non-optional in this DTO (ACD-043)
}

// Top-level response for get_account_details
struct AccountDetailsResponse {
    account_name: String,
    holdings: Vec<HoldingDetail>,              // active (quantity > 0), includes Cash Holding when present and qty > 0 (CSH-090, CSH-097); includes archived assets (ACD-020, ACD-021), sorted by asset_name asc (ACD-033)
    closed_holdings: Vec<ClosedHoldingDetail>, // closed, sorted by asset_name asc (ACD-046); empty list when none
    total_holding_count: i64,                  // all holdings regardless of quantity (ACD-034)
    total_cost_basis: i64,                     // micros, sum of cost_basis across active non-cash holdings (ACD-031, CSH-093)
    total_realized_pnl: i64,                   // micros, sum of total_realized_pnl across all holdings (ACD-045)
    total_unrealized_pnl: Option<i64>,         // micros; sum across same-currency priced active holdings; None when none qualify (MKT-040)
    total_global_value: i64,                   // micros, account currency: cash_holding.quantity + Σ_h (h.quantity × latest_price(h)) over non-cash active holdings; unpriced non-cash holdings contribute 0 (CSH-094)
}
```

## Events

### Published

| Event            | Payload | Rule    |
| ---------------- | ------- | ------- |
| `AccountUpdated` | —       | TRX-037 |

### Subscribed (frontend re-fetch triggers)

| Event               | Payload | Rule    |
| ------------------- | ------- | ------- |
| `AccountUpdated`    | —       | ACD-039 |
| `AssetUpdated`      | —       | ACD-040 |
| `AssetPriceUpdated` | —       | MKT-036 |

## Changelog

- 2026-04-26 — Added by `account` spec: get_accounts, add_account, update_account, delete_account, get_account_deletion_summary
- 2026-04-26 — Fixed: added InvalidCurrency error (TRX-021); removed phantom NotFound from delete_account and update_account; clarified error typing note
- 2026-04-26 — Typed errors: commands now return discriminated-union enums instead of `String`
- 2026-04-28 — Added `AccountUpdated` event (previously undeclared; owned by Account BC per migration plan)
- 2026-05-03 — Merged from `record_transaction-contract.md` and `transaction-contract.md`: get_asset_ids_for_account, buy_holding, sell_holding, correct_transaction, cancel_transaction, get_transactions, open_holding; Transaction struct reconciled (added created_at, added OpeningBalance variant)
- 2026-05-03 — Merged from `account_details-contract.md`: get_account_details; HoldingDetail, ClosedHoldingDetail, AccountDetailsResponse shared types, subscribed events section; updated stale TransactionUpdated → AccountUpdated
- 2026-05-06 — Added by `cash-tracking` spec: record_deposit, record_withdrawal; extended buy_holding / correct_transaction / cancel_transaction error sets with `InsufficientCash { current_balance_micros, currency }` (CSH-080); extended open_holding error set with `OpeningBalanceOnCashAsset` (CSH-061); `TransactionType` gained `Deposit` and `Withdrawal` variants; `AccountDetailsResponse` gained `total_global_value: i64` (CSH-094); added `DepositDTO`, `WithdrawalDTO` types.
- 2026-05-11 — Refreshed error model after the 13-PR error-model arc: replaced legacy `*CommandError` boundary types with the current composite shape (`AccountCrudError`, `HoldingTransactionError`, `OpenHoldingError`) and per-leaf typed enums; renamed wire variants `DbError` / `Unknown` → `DatabaseError`; per-command tables now show both the error type and the reachable code subset; removed `ArchivedAssetSell` (no such variant in code) and `UnitPriceNegative` from buy/sell tables (factory accepts zero — TRX-020 only rejects strictly-negative); added missing `DateInFuture` / `DateTooOld` to buy/sell/correct (raised by `Transaction::validate`); added `CascadingOversell` to cancel_transaction (replay after cancel can leave a later sell oversold — SEL-033); clarified that `OpenHoldingError` is owned in `use_cases/holding_transaction/error.rs`.
