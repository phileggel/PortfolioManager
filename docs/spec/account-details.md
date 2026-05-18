# Business Rules — Account Details (ACD)

## Context

The `Account Details` feature provides a view of a specific account's current positions and their cost basis. It allows users to drill down into an account to see their active holdings, the quantity held, the volume-weighted average purchase price, and the total cost basis for each position.

This feature consumes data from two bounded contexts: `account` (for Holding data including pre-computed realized P&L per ACD-043) and `asset` (for asset metadata: name, ticker, currency). Because these contexts must not import each other directly (B2), all cross-context reads are orchestrated by a dedicated `use_cases/account_details/` use case that injects `AccountService` and `AssetService` (per ADR-003, ADR-004).

> **ACD-045 change:** `TransactionService` was previously injected to aggregate realized P&L via `get_realized_pnl_by_account` (SEL-038, ADR-005). This is superseded by ACD-043/ACD-045 — P&L is now pre-computed on the `Holding` entity during the transaction replay. `TransactionService` is no longer a dependency of `AccountDetailsUseCase`. SEL-038 is superseded for the account details aggregation path.

> Cross-spec dependency: entry-point navigation behavior is owned by [Account Management](account.md) rule ACC-010.

> Market price tracking (current price, unrealized gain/loss, performance percentage) is not implemented in this version. It will be introduced as a dedicated feature once a market price data source is designed.

---

## Entity Definition

### Holding (Position)

Represents the current state of a financial position within the account. Persisted in the `holdings` table; all financial fields are computed by `RecordTransactionUseCase` via a full chronological replay of transactions on every create/edit/delete.

| Field                | Business meaning                                                                                                 |
| -------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `asset_id`           | The financial asset held.                                                                                        |
| `quantity`           | Current number of units held (i64 micros). Zero when the position is closed.                                     |
| `average_price`      | Volume-weighted average purchase price in account currency (i64 micros). Preserved at closure (TRX-040).         |
| `last_sold_date`     | ISO date of the most recent Sell transaction for this holding. NULL if no sell has ever occurred.                |
| `total_realized_pnl` | Cumulative realized profit or loss across all Sell transactions for this holding (i64 micros). Zero if no sells. |

### HoldingDetail (Backend DTO)

Read projection for **open** positions (`quantity > 0`). Enriched by `AccountDetailsUseCase` with asset metadata. Defined as a Rust struct with `#[derive(Type, Serialize)]`.

| Field             | Source                       | Business meaning                                                                       |
| ----------------- | ---------------------------- | -------------------------------------------------------------------------------------- |
| `asset_id`        | `Holding`                    | ID of the held asset.                                                                  |
| `asset_name`      | `AssetService`               | Display name of the asset.                                                             |
| `asset_reference` | `AssetService`               | Ticker or user-defined reference.                                                      |
| `quantity`        | `Holding`                    | Current number of units held (i64 micros). Always > 0.                                 |
| `average_price`   | `Holding`                    | VWAP purchase price in account currency (i64 micros).                                  |
| `cost_basis`      | computed                     | `quantity × average_price` (i128 intermediate, per ACD-023/ACD-024). Not stored in DB. |
| `realized_pnl`    | `Holding.total_realized_pnl` | Cumulative realized P&L from partial sells (i64 micros). Zero if none. See SEL-042.    |

### ClosedHoldingDetail (Backend DTO)

Read projection for **closed** positions (`quantity = 0`). New DTO introduced by this spec. Defined as a Rust struct with `#[derive(Type, Serialize)]`.

| Field             | Source                       | Business meaning                                                                                                                                                                                                               |
| ----------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `asset_id`        | `Holding`                    | ID of the asset.                                                                                                                                                                                                               |
| `asset_name`      | `AssetService`               | Display name of the asset.                                                                                                                                                                                                     |
| `asset_reference` | `AssetService`               | Ticker or user-defined reference.                                                                                                                                                                                              |
| `realized_pnl`    | `Holding.total_realized_pnl` | Total realized profit or loss for this position (i64 micros).                                                                                                                                                                  |
| `last_sold_date`  | `Holding.last_sold_date`     | ISO date when the position was fully closed. Rust type: `String` (non-optional) — only holdings where `last_sold_date IS NOT NULL` are included in `closed_holdings`, enforced in application code in `AccountDetailsUseCase`. |

### AccountDetailsResponse (Backend DTO)

The top-level response returned by the `get_account_details(account_id)` Tauri command. Defined as a Rust struct with `#[derive(Type, Serialize)]`.

| Field                 | Business meaning                                                                                                                                |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `account_name`        | Display name of the account (per ACD-032).                                                                                                      |
| `holdings`            | Active holdings (`quantity > 0`), sorted per ACD-033.                                                                                           |
| `closed_holdings`     | Closed holdings (`quantity = 0`), sorted per ACD-044. Empty list when none exist.                                                               |
| `total_holding_count` | Count of all holdings for the account regardless of quantity (used by ACD-034).                                                                 |
| `total_cost_basis`    | Sum of `cost_basis` across all active holdings (per ACD-031).                                                                                   |
| `total_realized_pnl`  | Sum of `total_realized_pnl` across **all** holdings (open + closed) in the account (per SEL-042/ACD-045).                                       |
| `total_global_value`  | Cash + Σ(market value of non-cash active holdings); see CSH-094 for the formula and missing-price treatment. Account-currency micros (ADR-001). |

> **MKT extension**: `docs/spec/market-price.md` adds `total_unrealized_pnl: Option<i64>` to this response and five new fields to `HoldingDetail` (`asset_currency`, `current_price`, `current_price_date`, `unrealized_pnl`, `performance_pct`). See the MKT spec for definitions.

> **CSH extension**: `docs/spec/cash-tracking.md` adds `total_global_value: i64` to this response (CSH-094) and amends ACD-034 with a cash-specific exception (CSH-098). The Cash Holding (when present and `quantity > 0`) is included in `holdings` like any other holding (CSH-090); ACD-020's `quantity > 0` filter applies to it without override (CSH-097). Asset metadata is enriched via `AssetService` like any other asset (ACD-022).

---

## Business Rules

### Navigation

**ACD-010 — View entry point (frontend)**: The Account Details view is accessed by clicking on an account row in the Account Table, excluding action buttons. This is the canonical navigation gesture defined in ACC-010; ACD-010 records the resulting destination.

**ACD-011 — Account selection persistence (frontend)**: The selected account is identified by its ID in the route `/accounts/:id`, enabling direct linking and browser "Back" navigation.

**ACD-012 — Invalid account guard (backend + frontend)**: If the `account_id` supplied to the backend does not correspond to an existing account, the backend returns an explicit not-found error. The frontend transitions to the error state (ACD-038).

### Holding List and Cost Basis

**ACD-020 — Active holding filter (backend)**: Only holdings with `quantity > 0` are included in the **active holdings section** of the Account Details view. Holdings with `quantity = 0` are excluded from the active section but may appear in the closed positions section (ACD-044, ACD-047). The Cash Holding follows this rule without exception (CSH-097): when `quantity = 0`, it is hidden from the active section and the frontend shows the "No cash recorded yet" banner (CSH-095) in its place.

**ACD-021 — Archived asset inclusion (backend)**: Holdings for archived assets are included in the display as long as their `quantity > 0`. Archiving an asset does not close its position.

**ACD-022 — Asset metadata enrichment (backend)**: For each holding, the use case fetches the corresponding asset's name and reference (ticker) from `AssetService`. These fields populate `HoldingDetail.asset_name` and `HoldingDetail.asset_reference`.

**ACD-023 — Cost basis calculation (backend)**: The cost basis for a holding is computed as `Holding.quantity × Holding.average_price`. This is the only financial computation performed by this feature.

**ACD-024 — Calculation precision (backend)**: Cost basis calculations use `i128` intermediates to prevent overflow before scaling back to `i64` micro-units, per ADR-001.

### Account Summary

**ACD-031 — Total account cost basis (backend)**: The total cost basis of the account is the sum of `cost_basis` across all active holdings (those included per ACD-020). When the account has no active holdings, the total cost basis is `0`.

**ACD-032 — Account name in response (backend)**: The use case fetches the account record via `AccountService.get_by_id` as its first step. The account name is included in `AccountDetailsResponse` so the frontend can display it in the header without a separate request.

**ACD-033 — Holdings sort order (backend)**: Holdings in the response are sorted alphabetically by `asset_name` ascending. This is the default display order; no user-controlled sort is required for the initial implementation.

### States

**ACD-034 — Empty account state (frontend)**: If no holdings remain after applying the `quantity > 0` filter (ACD-020), the view displays one of two messages depending on the backend response: "No positions yet" when the account has no holdings at all, or "All positions are closed" when holdings exist but all have `quantity = 0`. The Tauri command response includes `total_holding_count` so the frontend can distinguish these two cases without a second request. **Cash exception (per CSH-098)**: the Cash Holding is excluded from this asset-positions count — an account whose only non-zero holding is cash still renders the asset-positions area as empty (with the cash row visible above per CSH-091/092).

**ACD-035 — Empty state CTA (frontend)**: In the empty state, the view displays an "Add Transaction" button that opens the Add Transaction modal pre-filled with the `account_id` from the current route (ACD-011), per the pre-fill contract defined in TRX-011.

**ACD-036 — Non-empty state CTA (frontend)**: When the account has at least one active holding, the view displays a FAB or "Add Transaction" button that opens the Add Transaction modal pre-filled with the `account_id` from the current route (ACD-011), per TRX-011. This is the entry point referenced by TRX-010.

**ACD-037 — Loading state (frontend)**: While the backend call is in progress, the view displays skeleton screens for the header (account name, total cost basis) and for the holdings table. The loading state is entered on initial mount and on each re-fetch triggered by ACD-039 or ACD-040.

**ACD-038 — Error state (frontend)**: If the backend call fails (network error, not-found per ACD-012, or unexpected error), the view displays a generic error message and a "Retry" button. Pressing "Retry" re-triggers the backend call.

### Data Integrity and Reactivity

**ACD-039 — Reactivity to transactions (frontend)**: When a `TransactionUpdated` event is received (published by TRX-037), the Account Details view re-fetches the full `AccountDetailsResponse` for the current account, as holdings data (quantity, average_price) is derived from transactions. ACD-039 is the authoritative re-fetch mechanism for the Account Details view; TRX-038's store action is scoped to the transaction list feature and does not affect account details data.

**ACD-040 — Reactivity to asset updates (frontend)**: When an `AssetUpdated` event is received, the Account Details view re-fetches the full `AccountDetailsResponse` to reflect any changes to asset metadata (name or reference). The event is published by the `asset` context; ACD-040 only governs the frontend reaction.

**ACD-041 — Precision handling (backend)**: All financial values in `HoldingDetail` and `AccountDetailsResponse` are serialised as `i64` micro-unit values per ADR-001. The frontend presenter is responsible for converting them to display-ready strings.

**ACD-042 — Holding row inspect action (frontend)**: Each holding row in the Account Details view exposes an inspect action (e.g. a magnifier icon). Clicking it navigates to the Transaction List view for that holding (`/accounts/:accountId/transactions/:assetId`), per TXL-010. Navigation goes through the router; the Account Details view does not import from the `transactions` feature.

### Closed Position History

**ACD-043 — Holding persistence fields (backend)**: The `holdings` table is extended with two new columns computed during the chronological replay in `RecordTransactionUseCase`: `last_sold_date TEXT` (nullable — NULL when no Sell transaction has ever occurred for this holding) and `total_realized_pnl INTEGER NOT NULL DEFAULT 0` (cumulative sum of `realized_pnl` across all Sell transactions for this holding). Both are updated on every transaction create, edit, and delete via the full replay — including the delete case, where removing a Sell transaction recalculates `last_sold_date` from the remaining Sells (or sets it to NULL if none remain).

The `Holding` domain entity gains both fields: `last_sold_date: Option<String>` and `total_realized_pnl: i64`. All three factory methods (`new()`, `with_id()`, `restore()`) must be updated to accept and carry these fields. `SqliteHoldingRepository.upsert()` must write both columns; its row mapping must read them.

The migration that adds these columns also backfills existing rows using SQL aggregation against the `transactions` table, so no replay-on-startup is required:

```sql
UPDATE holdings SET
  total_realized_pnl = COALESCE((
    SELECT SUM(realized_pnl) FROM transactions
    WHERE account_id = holdings.account_id AND asset_id = holdings.asset_id
    AND transaction_type = 'Sell'), 0),
  last_sold_date = (
    SELECT MAX(date) FROM transactions
    WHERE account_id = holdings.account_id AND asset_id = holdings.asset_id
    AND transaction_type = 'Sell');
```

**ACD-044 — Closed holdings query (backend)**: `AccountDetailsUseCase` splits the full holdings list for the account into active (`quantity > 0`) and closed (`quantity = 0`). No new DB query is needed — the existing holdings fetch returns all rows regardless of quantity; the split happens in application code. Closed holdings are enriched with asset name and reference via `AssetService`, identical to the active holdings enrichment path (ACD-022).

**ACD-045 — AccountDetailsResponse total_realized_pnl source (backend)**: `AccountDetailsResponse.total_realized_pnl` is computed as the sum of `Holding.total_realized_pnl` across **all** holdings for the account (both active and closed). This supersedes SEL-038's `get_realized_pnl_by_account` aggregation query for this use case — the value is semantically identical as long as ACD-043's backfill migration has run. `TransactionService` is no longer injected into `AccountDetailsUseCase`; the P&L data is available directly from the holdings fetch (ACD-044). No new error variant is introduced: the only failure path remains the holdings fetch itself, already covered by ACD-038. `TransactionService.get_realized_pnl_by_account` is retained in the service and repository layers as it may serve future use cases, but is no longer called from `AccountDetailsUseCase`.

**ACD-046 — Closed holdings sort order (backend)**: Closed holdings in `AccountDetailsResponse.closed_holdings` are sorted alphabetically by `asset_name` ascending, consistent with ACD-033.

**ACD-047 — Closed positions section visibility (frontend)**: A "Closed positions" section is rendered below the active holdings table only when `closed_holdings` is non-empty. When the list is empty, no section header or table is rendered.

**ACD-048 — Closed positions table columns (frontend)**: The closed positions table displays three columns: Asset (name + ticker), Realized P&L, Last sold date. Quantity, average price, and cost basis columns are omitted — they carry no meaning for a position with `quantity = 0`.

**ACD-049 — Closed positions row actions (frontend)**: Each closed position row exposes only the inspect action (magnifier icon, per ACD-042) to navigate to the transaction history for that asset. Buy (+) and Sell (−) action buttons are not shown for closed positions.

**ACD-050 — "All positions closed" state scope (frontend)**: The "All positions closed" empty-state message (ACD-034) applies to the active holdings area only. When `closed_holdings` is non-empty, the closed positions section is still rendered below — the page is not considered empty.

---

## Workflow

```
[User clicks account row (ACC-010)]
  → Route: /accounts/:id
          │
          ├─ [use_cases/account_details/: Fetch Account metadata from account context]
          ├─ [use_cases/account_details/: Fetch ALL Holdings for account (no qty filter)]
          ├─ [use_cases/account_details/: Split → active (qty > 0) + closed (qty = 0)]
          ├─ [use_cases/account_details/: Fetch asset metadata for all holdings from asset context]
          ├─ [use_cases/account_details/: Build HoldingDetail list (active) — cost_basis computed]
          ├─ [use_cases/account_details/: Build ClosedHoldingDetail list (closed)]
          ├─ [use_cases/account_details/: Compute total_cost_basis (ACD-031)]
          ├─ [use_cases/account_details/: Compute total_realized_pnl = Σ holding.total_realized_pnl (ACD-045)]
          │
          └─ [Frontend: Loading state → Header (Account name + Total Cost Basis + Total Realized P&L)]
             [Frontend: Active holdings table (one row per HoldingDetail)]
             [Frontend: Closed positions section (one row per ClosedHoldingDetail, ACD-047–ACD-049)]
             [Frontend: Empty / Error state as applicable]
```

---

## UX Draft

### Entry Point

- Clicking a row in the `AccountTable` (excluding action buttons), per ACC-010.

### Main Component

**ManagerLayout** containing:

- **Header**: Account name + Total Cost Basis + Total Realized P&L (large).
- **Active Positions Table**:
  - Asset (Name + Ticker)
  - Quantity
  - Avg. Price
  - Cost Basis
  - Realized P&L (`—` when zero or no sells)
  - Actions: Buy, Sell, Inspect
- **Closed Positions Section** (rendered only when `closed_holdings` non-empty, per ACD-047):
  - Section heading "Closed positions"
  - Table columns: Asset (Name + Ticker), Realized P&L, Last sold date
  - Actions: Inspect only (per ACD-049)

### States

- **Loading**: Skeleton screens for the header and table.
- **Empty (no positions)**: "No positions yet" + "Add Transaction" button.
- **Empty (all closed)**: "All positions are closed" + "Add Transaction" button + closed positions section still rendered below (ACD-050).
- **Error**: Generic error message with "Retry" button.

### User Flow

1. User clicks on an account row in the Account Table.
2. Route navigates to `/accounts/:id`; loading skeletons appear.
3. Data loads: user sees active positions with cost basis and the account totals.
4. If closed positions exist, they appear below the active table with realized P&L and last sold date.
5. User can add a transaction via the FAB or "Add Transaction" button.
