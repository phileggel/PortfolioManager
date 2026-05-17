# Business Rules — Market Price (MKT)

## Context

The Market Price feature allows users to record the current market value of a financial asset manually. It is the first phase of market price support; automated price feeds are a future feature that will slot into the same data model.

A price is recorded per asset (not per holding) and is timestamped: multiple entries can accumulate over time, one per date per asset. The Account Details view uses the most recently dated price to display the current value, unrealized gain/loss, and performance percentage for each active holding.

This spec is a **feature spec** spanning two domains: price recording belongs to the `asset` bounded context; display of current price and derived values extends the `use_cases/account_details/` use case. See `docs/spec/account-details.md` for the baseline Account Details behaviour that this spec extends.

By default, recording a buy or sell transaction does **not** automatically create a price record. `Transaction.unit_price` is the price transacted at (a cost-basis input); `AssetPrice.price` is the current market value of the asset. Conflating them by default would show cost as current price, making unrealized P&L meaningless. As an explicit opt-in (see MKT-050+), the user can choose — globally or per transaction — to also persist the transacted unit price as the asset's market price for the transaction date.

All financial values are stored as `i64` micro-units per [ADR-001](../adr/001-use-i64-for-monetary-amounts.md).

---

## Entity Definition

### AssetPrice

Represents a manually recorded market price for a financial asset on a specific date. Owned by the `asset` bounded context.

| Field      | Business meaning                                                                                                                                                                                                                                                                                                       |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `asset_id` | The asset whose market price this record describes.                                                                                                                                                                                                                                                                    |
| `date`     | The calendar date this price observation applies to (ISO 8601, e.g. `2026-04-26`). Date is the user's local calendar date at write time, not the asset market's timezone.                                                                                                                                              |
| `price`    | Market price per unit in the asset's native currency (i64 micros, ADR-001).                                                                                                                                                                                                                                            |
| `source`   | Provenance of this price record (see MKT-100 for variants). `Manual` for user-entered values (including those auto-recorded from a transaction's `record_price=true` flag); a provider name (e.g. `Stooq`) for auto-fetched values. Metadata for traceability; does not influence read/write precedence (per ADR-012). |

> The combination `(asset_id, date)` is unique: only one price per asset per day. Recording a second price for the same `(asset_id, date)` pair overwrites the first (MKT-025), regardless of source (per ADR-012). Correction by re-recording remains valid. Standalone edit and delete of individual entries are also supported via the price history view (MKT-070+).

### HoldingDetail (extended)

The `HoldingDetail` DTO defined in the ACD spec gains five new fields populated by this feature.

| Field                | Business meaning                                                                                                                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `asset_currency`     | ISO 4217 currency code of the asset (e.g. `"USD"`). Required for the price label in the modal (MKT-023) and for the currency-mismatch check (MKT-034). Always present.                                 |
| `current_price`      | Most recently dated `AssetPrice.price` for this asset, in asset currency (i64 micros). `None` if no price has ever been recorded.                                                                      |
| `current_price_date` | ISO date string of the price observation used as `current_price`. `None` when `current_price` is `None`.                                                                                               |
| `unrealized_pnl`     | Unrealized gain or loss in account currency (i64 micros). `None` when no price exists or when asset and account currencies differ (MKT-034). `0` when current price equals average price (not `None`). |
| `performance_pct`    | `unrealized_pnl / cost_basis × 100`, expressed as i64 micros (e.g. 5.25 % = 5 250 000). `None` when `unrealized_pnl` is `None` or `cost_basis` is zero. `0` when `unrealized_pnl` is zero.             |

### AccountDetailsResponse (extended)

The `AccountDetailsResponse` DTO gains one new field.

| Field                  | Business meaning                                                                                                                                                                  |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `total_unrealized_pnl` | Sum of `unrealized_pnl` (i64 micros) across all active holdings where a value is computable (same-currency with a recorded price). `None` when no holding has a computable value. |

---

## Business Rules

### Eligibility and Initiation (010–019)

**MKT-010 — Entry point (frontend)**: An "Enter price" action is available on each **active** holding row in the Account Details view, alongside the existing Buy and Sell actions. It is not shown on closed holdings.

**MKT-011 — Modal pre-fill — asset and date (frontend)**: Opening the "Enter price" form pre-fills the asset name as a read-only label and the date field with today's date (editable by the user, e.g. to backdate to yesterday's closing price).

**MKT-012 — Modal pre-fill — price (frontend)**: When `HoldingDetail.current_price_date` equals today's ISO date at the time the modal is opened, the price field is pre-filled with `HoldingDetail.current_price`. When the latest recorded price is from a prior date, the price field opens empty.

**MKT-013 — No additional backend call for pre-fill (frontend)**: The pre-fill data (MKT-011, MKT-012) is sourced from the `HoldingDetail` already loaded by the Account Details view. Opening the modal requires no additional IPC request.

### Recording a Price (020–029)

**MKT-020 — Required fields (frontend)**: The price form requires a non-empty date and a non-empty price. The submit button is disabled while either field is empty. The asset is implicit from the entry point and is not modifiable by the user.

**MKT-021 — Price validation (frontend + backend)**: A valid price is strictly greater than zero. The backend rejects a submitted price of zero or below with a specific error. The frontend validates inline and disables the submit button until corrected.

**MKT-022 — Date validation (frontend + backend)**: A valid date is a well-formed ISO 8601 calendar date (`YYYY-MM-DD`) that is not in the future. Any past date is accepted; no lower bound applies (users may backdate historical prices). The backend rejects an invalid or future date with a specific error. The frontend validates inline and disables the submit button until corrected.

**MKT-023 — Price currency (frontend + backend)**: The price is stored in the asset's native currency. No currency conversion is applied at recording time. The asset's currency code is displayed as a read-only label next to the price input field so the user knows which currency they are entering.

**MKT-024 — i64 storage (backend)**: The price is stored as i64 micro-units per ADR-001. The frontend transmits the human-readable decimal; the backend converts to micros at the IPC boundary.

**MKT-025 — Upsert by (asset, date) (backend)**: If a price record already exists for the same `(asset_id, date)` pair, it is overwritten with the new value regardless of either row's `source` (per ADR-012; latest-write-wins). Otherwise a new record is created. This is transparent to the user; the form behaves identically for new and existing entries. The `source` value written by this command is governed by MKT-101.

**MKT-026 — AssetPriceUpdated event (backend)**: After a successful upsert, the backend publishes an `AssetPriceUpdated` event on the event bus. This event carries no payload; it is a bare signal consistent with `AssetUpdated`, `CategoryUpdated`, `AccountUpdated`, and `TransactionUpdated`. It is published by the `asset` bounded context per B4. The Tauri frontend event discriminant string is `"AssetPriceUpdated"` — the variant name is forwarded as-is by the event forwarder, matching the convention for all existing events.

**MKT-027 — In-flight state (frontend)**: While the upsert request is in progress, the submit button is disabled and displays a spinner to prevent double-submission.

**MKT-028 — Success feedback (frontend)**: On success, the modal closes and a snackbar confirms the price was recorded.

**MKT-029 — Error feedback (frontend)**: On validation failure or backend rejection, the modal remains open. An inline error message is shown adjacent to the invalid field. The user can correct and resubmit without reopening the form.

### Display in Account Details (030–039)

**MKT-030 — Current price column (frontend + backend)**: The Account Details active holdings table gains a "Current Price" column. For each holding row, it displays `HoldingDetail.current_price` formatted in the asset's native currency. When `current_price_date` is available, it is shown as a secondary label indicating the age of the data — formatted as "Updated today" when the date equals the user's local date, or "Updated Nd ago" otherwise. See MKT-140 for the staleness-label rules introduced alongside auto-fetch.

**MKT-031 — Latest price resolution (backend)**: The `AccountDetailsUseCase` retrieves the most recently dated `AssetPrice` for each active holding's asset via `AssetService`, per ADR-004 (use cases inject services, not repositories). If no record exists for an asset, `current_price` and `current_price_date` are `None`. A failure in the price lookup does not abort the overall `get_account_details` response; it degrades gracefully by returning `None` for the affected holding's price fields.

**MKT-032 — No-price placeholder (frontend)**: When `current_price` is `None` for a holding, the "Current Price", "Unrealized P&L", and "Performance %" columns display "—" for that row.

**MKT-033 — Unrealized P&L — same currency (backend)**: When the asset's native currency equals the account's currency (the gate condition defined in MKT-034), the backend computes `unrealized_pnl = (current_price − average_price) × quantity` using i128 intermediates before scaling back to i64, consistent with ACD-024. Both `current_price` and `average_price` are expressed in the same currency under this condition, making the subtraction valid. The result is included in `HoldingDetail.unrealized_pnl`. A zero result is returned as `0`, not `None`.

**MKT-034 — Unrealized P&L — currency mismatch (frontend + backend)**: When the asset currency differs from the account currency, `HoldingDetail.unrealized_pnl` and `HoldingDetail.performance_pct` are `None`. The frontend displays "—" in those columns. No exchange-rate conversion is attempted in this phase; multi-currency unrealized P&L is deferred to a future iteration.

**MKT-035 — Performance % (backend)**: When `unrealized_pnl` is available and `cost_basis` is non-zero, the backend computes `performance_pct = unrealized_pnl × 100 / cost_basis` as i64 micros using i128 intermediates and Rust integer division (truncation toward zero). Example: 5.25 % = 5 250 000 micros; −3.7 % = −3 700 000 micros. A zero result is returned as `0`, not `None`. When `cost_basis` is zero, `performance_pct` is `None`.

**MKT-036 — Reactivity (frontend)**: The Account Details event subscription adds `AssetPriceUpdated` alongside the existing `TransactionUpdated` and `AssetUpdated` subscriptions (ACD-039). Upon receiving `AssetPriceUpdated`, the view re-fetches account details, ensuring that newly recorded prices and all derived values (unrealized P&L, performance %, totals) are reflected immediately without a manual page refresh.

**MKT-037 — AssetPriceUpdated event registration (backend + frontend)**: The `AssetPriceUpdated` event is added to the event bus enum alongside `AssetUpdated`, `CategoryUpdated`, `AccountUpdated`, and `TransactionUpdated`. It is published exclusively by the `asset` bounded context. The global store treats it as a locally-handled event (no global data re-fetch triggered). `ARCHITECTURE.md` must be updated to register `AssetPriceUpdated` in the event bus table and to document that `useAccountDetails` subscribes to it alongside `TransactionUpdated` and `AssetUpdated`.

### Account Summary (040–049)

**MKT-040 — Total unrealized P&L (backend)**: `AccountDetailsResponse.total_unrealized_pnl` is the sum of `unrealized_pnl` across all active holdings for which the value is computable (same-currency holdings with a recorded price). Holdings with a currency mismatch or no recorded price are excluded from the sum and contribute nothing. When no holding qualifies, the field is `None`.

**MKT-041 — Total unrealized P&L display (frontend)**: The Account Details summary row displays `total_unrealized_pnl`. When the value is `None`, the summary shows "—". When the value is a number (including zero), it is displayed as-is; per-row "—" placeholders already communicate which individual holdings were excluded from the sum.

**MKT-042 — Correction by re-recording (backend)**: An `AssetPrice` record can be corrected by re-recording a price for the same `(asset_id, date)`, which overwrites the existing entry (MKT-025). Standalone edit (MKT-083, MKT-084) and delete (MKT-090) via the price history view are also supported.

**MKT-043 — Unknown asset rejection (backend)**: The backend rejects `record_asset_price` with a specific error if `asset_id` does not refer to a known asset. In normal use the asset is always selected from active holdings, making this case unreachable from the UI; the guard exists for API-level correctness.

### Auto-record from Transactions (050–069)

This section extends the buy/sell transaction flow defined in `docs/spec/financial-asset-transaction.md` (TRX) and `docs/spec/sell-transaction.md` (SEL). When the user opts in, recording a buy or sell transaction also writes an `AssetPrice` record so the transacted unit price becomes the asset's market price for the transaction date. Standalone edit and delete of `AssetPrice` records are now supported via the price history view (MKT-070+).

**MKT-050 — Global auto-record toggle (frontend)**: The Settings page exposes a toggle "Automatically record transaction price as market price". The toggle defaults to OFF. The user's choice persists across sessions on the current device. The toggle controls only the default state of the per-transaction checkbox (MKT-052); it never bypasses or replaces that checkbox.

**MKT-051 — Per-transaction checkbox (frontend)**: The buy and sell transaction forms (both creation forms — modal and the standalone /transactions/new page — and their edit variants) display a checkbox "Use this price as the market price for {transaction date}" placed immediately before the form's primary submit action, after all data fields. The label's date placeholder reflects the form's current `date` field value and updates live when the user changes it.

**MKT-052 — Checkbox default state (frontend)**: When the form opens to **create** a new transaction, the checkbox initial state equals the current value of the global toggle (MKT-050). When the form opens to **edit** an existing transaction, the checkbox initial state is always OFF, regardless of the global toggle.

**MKT-053 — Checkbox snapshot semantics (frontend)**: The checkbox initial state (MKT-052) is read from the global toggle once at form open. Subsequent changes to the global toggle do not propagate into already-open forms; the user keeps whatever state they have already set in the open form.

**MKT-054 — Submit payload (frontend + backend)**: The frontend forwards the checkbox state as a `record_price: bool` field added to the existing `CreateTransactionDTO` already used by `add_transaction` and `update_transaction` (per the project's Specta single-DTO convention). The backend never reads the global toggle directly — the per-call flag carried in the DTO is the only signal that determines whether a price is recorded.

**MKT-055 — Auto-write as a separate frontend call (frontend)**: When `record_price` is `true` and `tx.unit_price > 0` (see MKT-061 for the zero-price exception), the frontend calls `record_asset_price(asset_id, tx.date, tx.unit_price)` as a separate Tauri command **after** the transaction command (`add_transaction` / `update_transaction`) has returned successfully. The write targets `(asset_id = tx.asset_id, date = tx.date, price = tx.unit_price)` and uses the same upsert semantics as MKT-025 (insert on absence, replace on `(asset_id, date)` collision). The price is taken in the asset's native currency; `tx.unit_price` already excludes fees per the TRX domain definition, so no fee adjustment is applied. Validation rules MKT-021 (price > 0) and MKT-022 (date not in future) hold by construction: TRX-020 enforces `tx.date` not in the future, and the `tx.unit_price > 0` precondition is enforced by MKT-061.

**MKT-056 — No atomicity between transaction and price write (frontend)**: The transaction insert/update and holding recomputation (TRX-027 / SEL-025) are committed atomically by the backend command. The `record_asset_price` call (MKT-055) is a separate, independent command issued by the frontend after the transaction commits. There is no shared database transaction between the two operations. If the transaction succeeds and the price write subsequently fails, the transaction remains committed; no rollback occurs. This is a deliberate trade-off documented in ADR-006.

**MKT-057 — AssetPriceUpdated event on auto-record (backend)**: After `record_asset_price` commits successfully (i.e. MKT-061 did not skip the call), the backend publishes the `AssetPriceUpdated` event defined in MKT-026 via `AssetService::notify_asset_price_updated()`. This event is in addition to the `TransactionUpdated` event published by the transaction command. The two events are independent signals; their relative order is unspecified and irrelevant because subscribers refetch idempotently. When `record_price` is `false` or MKT-061 skipped the call, no `AssetPriceUpdated` event is published.

**MKT-058 — Conflict — silent overwrite (backend)**: If an `AssetPrice` record already exists at `(tx.asset_id, tx.date)` when `record_price` is `true`, it is silently overwritten with `tx.unit_price` via the same upsert semantics as MKT-025. No prompt or warning is shown to the user; the form behaves identically whether or not a same-day price already exists.

**MKT-059 — Edit lifecycle — price independence (backend)**: Editing a transaction does not modify or remove any `AssetPrice` record previously written by that transaction. When the user re-saves an edited transaction with `record_price = true`, the upsert (MKT-055) targets the transaction's _current_ `tx.date` and _current_ `tx.unit_price`. If the user changed the transaction date during the edit, the price record at the prior date is left untouched and remains in storage; the upsert lands at the new date as a separate `(asset_id, date)` row. The same applies if the user changed the unit price: only the row at the current date is overwritten.

**MKT-060 — Delete lifecycle — price independence (backend)**: Deleting a transaction does not remove any `AssetPrice` record previously written by that transaction. `AssetPrice` records are independent of the transaction lifecycle: once persisted, they are governed solely by MKT rules (upsert via MKT-025; standalone delete via MKT-090).

**MKT-061 — Zero unit_price skip (backend)**: If `record_price` is `true` and `tx.unit_price` is `0` (a valid transaction per TRX-020 / SEL-020 — gifted or inherited assets), the orchestrator silently skips the `AssetPrice` write. The transaction itself proceeds normally and commits per its own validation rules; no `AssetPriceUpdated` event is published; no error is surfaced to the user. Rationale: a zero market price would conflict with MKT-021 (price > 0) and is not a meaningful signal of the asset's market value.

**MKT-062 — Auto-record failure is best-effort (frontend)**: A failure of the `record_asset_price` call (MKT-055) does not surface an error to the user. The transaction is already committed (MKT-056) and the frontend treats the price write as fire-and-forget: failures are logged as warnings and silently dropped. No inline error, snackbar, or retry affordance is shown specifically for the price write. The user may still record the price manually via the price history modal (MKT-070+).

### Price History CRUD (070–092)

**MKT-070 — Price history entry point (frontend)**: A "Price history" action is available on each active holding row in Account Details, alongside the existing Buy, Sell, and "Enter price" actions. It is not shown on closed holdings.

**MKT-071 — Price history list contents (frontend)**: The price history view lists all `AssetPrice` records for the selected asset, sorted by date descending (most recent first). Each row displays the date and the price formatted in the asset's native currency. Edit and Delete affordances are shown on each row. `AssetPrice` is per asset (not per holding), so the history shows all recorded prices regardless of which account the entry point was reached from.

**MKT-072 — Backend query for price history (backend)**: The `asset` bounded context exposes a `get_asset_prices(asset_id)` command that returns all `AssetPrice` records for a given `asset_id`, ordered by date descending. The returned list includes all historical entries, not only the most recent. The command returns a specific error if `asset_id` does not refer to a known asset; a successful response with an empty list is returned when the asset exists but has no recorded prices.

**MKT-073 — Empty state (frontend)**: When no `AssetPrice` records exist for the asset, the price history view displays a message indicating no prices have been recorded yet, and offers an affordance to add the first price.

**MKT-074 — Loading and error states (frontend)**: While price records are being fetched, the price history view displays a loading indicator. A fetch failure surfaces an inline error with a retry affordance; the view remains open rather than closing.

**MKT-075 — In-view "Add price" affordance (frontend)**: The price history view includes an action to add a new price entry, opening the same recording form governed by MKT-020–MKT-029 (same validation, same upsert semantics, same `AssetPriceUpdated` publication). After a successful add, the history list refreshes to include the new entry.

**MKT-076 — View reactivity after mutations (frontend)**: After a successful add (MKT-075), edit (MKT-086), or delete (MKT-092), the price history view re-fetches its list via MKT-072. The `AssetPriceUpdated` event emitted by the backend (MKT-026, MKT-085, MKT-091) independently causes the Account Details view to re-fetch, keeping the current-price column and derived values in sync without any additional coordination by the price history view.

**MKT-080 — Edit action (frontend)**: Each row in the price history list has an Edit action that opens an edit form pre-filled with that row's date and price.

**MKT-081 — Edit form fields (frontend)**: The edit form displays the asset name as a read-only label, an editable date field, and an editable price field, both pre-filled with the current values of the selected entry. The asset's currency code is shown as a read-only label next to the price field, consistent with MKT-023.

**MKT-082 — Edit validation (frontend + backend)**: The edit form applies the same validation as price recording: price must be strictly greater than zero (MKT-021) and date must be a well-formed ISO 8601 calendar date not in the future (MKT-022). The backend rejects invalid values with the same specific errors. The frontend validates inline and disables the submit button until all fields are valid.

**MKT-083 — Edit semantics — same date (backend)**: The `asset` bounded context exposes an `update_asset_price(asset_id, original_date, new_date, new_price)` command that returns `()` on success. When `original_date` equals `new_date`, the backend updates the price in place at the existing `(asset_id, date)` row; a single-row in-place update is inherently atomic. The command returns `NotFound` if the record at `(asset_id, original_date)` does not exist, or `Unknown` for any other failure.

**MKT-084 — Edit semantics — date changed (backend)**: When `original_date` differs from `new_date` in an `update_asset_price` call (MKT-083), the backend deletes the record at `(asset_id, original_date)` and upserts a new record at `(asset_id, new_date)`. If a record already exists at the new date, it is silently overwritten, consistent with MKT-025. The deletion and the upsert are atomic within a single database transaction; a failure in either step rolls back the entire operation.

**MKT-085 — AssetPriceUpdated after successful edit (backend)**: After a successful edit (MKT-083 or MKT-084), the backend publishes an `AssetPriceUpdated` event consistent with MKT-026. No event is published if the edit fails.

**MKT-086 — Edit success feedback (frontend)**: On a successful edit, the edit form closes, the price history list refreshes (MKT-076), and a snackbar confirms the price was updated.

**MKT-087 — Edit error feedback (frontend)**: On a validation failure or backend rejection, the edit form remains open. An inline error message is shown adjacent to the invalid field. The user can correct and resubmit without reopening the form.

**MKT-094 — Edit in-flight state (frontend)**: While the edit submit request is in progress, the submit button is disabled and displays a spinner to prevent double-submission, consistent with MKT-027.

**MKT-095 — asset_id is immutable on edit (backend)**: The `update_asset_price` command (MKT-083) does not accept a new `asset_id`. The asset an `AssetPrice` belongs to cannot be changed after creation; only the date and price are modifiable.

**MKT-088 — Delete action (frontend)**: Each row in the price history list has a Delete action.

**MKT-089 — Delete confirmation dialog (frontend)**: Triggering the Delete action opens a confirmation dialog that identifies the date and price of the record to be removed. The user must explicitly confirm before deletion proceeds.

**MKT-090 — Delete command (backend)**: The `asset` bounded context exposes a `delete_asset_price(asset_id, date)` command that returns `()` on success. If no record exists at `(asset_id, date)`, the command returns `NotFound`. Any other failure returns `Unknown`.

**MKT-091 — AssetPriceUpdated after deletion (backend)**: After a successful deletion, the backend publishes an `AssetPriceUpdated` event consistent with MKT-026. The Account Details view re-fetches via MKT-036; if the deleted entry was the most recently dated price for the asset, the holding row falls back to the next most recent price or shows "—" if no records remain.

**MKT-092 — Delete success feedback (frontend)**: On a successful deletion, the price history list refreshes (MKT-076) and a snackbar confirms the price record was removed.

**MKT-093 — In-flight state for delete (frontend)**: While the delete request is in progress (after the user confirms in MKT-089), the Delete action for the targeted row is disabled to prevent double-submission.

**MKT-096 — Delete error feedback (frontend)**: If the delete request fails, an error banner is shown at the top of the price history list. The view remains open and the targeted entry is not removed from the list. The user may retry.

### Source field on AssetPrice (100–109)

These rules apply to all paths that write `AssetPrice` (manual entry MKT-020+, transaction auto-record MKT-050+, and auto-fetch — see "Auto-Fetch from External Provider").

**MKT-100 — `AssetPriceSource` enum (backend)**: `AssetPrice.source` is of type `AssetPriceSource`, with variants `Manual | Stooq` in v1 (`Finnhub` added when the KEY spec ships). Exposed on the frontend wire surface.

**MKT-101 — `source: Manual` on user-driven paths (backend)**: Every user-driven write sets `source = Manual` — both `record_asset_price` (manual entry MKT-020+, transaction auto-record MKT-050+) and `update_asset_price` (price-history edit MKT-083, MKT-084). An auto-fetched row edited via the price-history flow therefore becomes `Manual`. The frontend never passes a source value.

**MKT-102 — `source: Stooq` on fetched paths (backend)**: Every write produced by a fetch path (launch MKT-122, global refresh MKT-130, account refresh MKT-132) sets `source = Stooq` (and the corresponding provider variant when more are added per MKT-100).

### Auto-Fetch from External Provider (110–149)

This section adds an automated price-update mechanism that complements the existing manual entry paths (MKT-020+, MKT-050+). Auto-fetch retrieves current prices from an external provider on app launch and on user demand. The choice of external provider is captured in ADR-008 (Stooq primary; future Finnhub fallback per the KEY spec, deferred to a follow-up phase).

#### Fetch task definitions

- **Auto-fetch**: a task called at application launch.
- **Global refresh**: a task manually triggered by the user to refresh all values (button on global dashboard).
- **Account refresh**: a task triggered by the user on an account detail page to refresh all values for that account (button on account detail).

#### Shared behaviors (110–119)

**MKT-110 — Symbol derivation (backend)**: The external provider symbol is derived from `Asset.reference` per ADR-008. When a valid symbol cannot be derived, the asset is skipped per MKT-114.

**MKT-111 — Empty-holdings short-circuit (backend)**: When the task's scope contains no holding asset that is both active (quantity > 0) and has a derivable provider symbol (MKT-110), every fetch task (launch MKT-122, global refresh MKT-130, account refresh MKT-132) completes immediately with no external calls.

**MKT-112 — `AssetPriceUpdated` on fetch success (backend)**: Every successful `AssetPrice` write produced by a fetch publishes `AssetPriceUpdated` per MKT-026.

**MKT-113 — In-flight guard, one fetch at a time (backend)**: Only one fetch task may run at a time across all three paths (launch MKT-122, global refresh MKT-130, account refresh MKT-132). If a fetch task is already running, a subsequent task call is rejected with a specific error.

**MKT-114 — Asset silently skipped on per-asset failure (backend)**: In a fetch task (launch MKT-122, global refresh MKT-130, account refresh MKT-132), an individual asset is silently skipped (no row written, no error surfaced) when either its symbol cannot be derived OR the provider fetch fails (network/HTTP/parse error, logged as warning). The task continues with the remaining assets.

**MKT-115 — Manual refresh feedback (frontend)**: For user-triggered fetch actions (global refresh MKT-130, account refresh MKT-131), the frontend surfaces feedback via snackbar:

- On successful dispatch by the backend: a snackbar acknowledges the fetch has started (e.g. "Fetching prices…").
- On in-flight error (MKT-113): a snackbar indicates a fetch is already in progress; the user's action is rejected without disrupting the ongoing fetch.

The launch auto-fetch (MKT-121) is silent (no snackbar).

**MKT-116 — System cash assets excluded (backend)**: System cash assets (per CSH spec, identified by their `system-cash-*` reference) are excluded from every fetch task scope (launch MKT-122, global refresh MKT-130, account refresh MKT-132). They have no external market price.

#### Auto-fetch (120–125)

**MKT-120 — Auto-fetch setting (frontend)**: An auto-fetch setting is present on the Settings page. The setting defaults to `OFF` and persists across sessions on the current device. When `ON`, the auto-fetch feature is enabled; when `OFF`, it is disabled.

**MKT-121 — Auto-fetch call (frontend)**: If the setting (MKT-120) is `ON`, the frontend calls the auto-fetch task once per session, after initial app mount. The call is fire-and-forget (the frontend does not await the backend response).

**MKT-122 — Auto-fetch start (backend)**: The auto-fetch task scope is all active holdings across all accounts (subject to MKT-111, MKT-116). Auto-fetch is acknowledged synchronously; per-asset results are signaled via `AssetPriceUpdated` (MKT-112).

#### Manual refresh (130–134)

**MKT-130 — Global refresh (frontend)**: The global refresh action is triggered by the user on the global dashboard. It uses the same backend entry point as the auto-fetch call (MKT-122) and therefore shares its scope (all active holdings across all accounts). The call is fire-and-forget.

**MKT-131 — Account refresh (frontend)**: The account refresh action is triggered by the user on an account detail page. The account identifier is transmitted to the backend. The call is fire-and-forget.

**MKT-132 — Account refresh (backend)**: Account refresh on an unknown account is rejected with a specific error. Otherwise account refresh is acknowledged synchronously; per-asset results are signaled via `AssetPriceUpdated` (MKT-112). The task scope is the active / derivable holdings for the specified account (subject to MKT-111, MKT-116).

**MKT-133 — Refresh button in-flight state (frontend)**: While a manual refresh is being acknowledged (between the user click and the dispatch-success snackbar of MKT-115, or the in-flight error snackbar), the corresponding refresh button is disabled and displays a spinner to prevent double-clicks.

#### Display (140–149)

**MKT-140 — Staleness indicator (frontend)**: The "Current Price" column's secondary label shows "Updated today" when the most recent `AssetPrice.date` for the asset equals today's local date, "Updated Nd ago" otherwise (N integer day delta), "—" when no price exists.

**MKT-141 — Source badge in price history (frontend)**: Each row in the price-history modal (MKT-071) displays a badge with the row's `source` value.

**MKT-142 — Source badge in Current Price column (frontend)**: The Account Details "Current Price" column displays a badge alongside the price (or near the staleness label MKT-140) showing the source of the most recent `AssetPrice` record. Same styling as MKT-141.

---

## Workflow

```
Account Details (active holding row)
    → "Enter price" button
    → PriceModal opens (no extra fetch — uses HoldingDetail data)
        date = today (editable)
        price = current_price if current_price_date == today, else empty
        → user enters price, adjusts date if needed
        → submit
            backend: validate price > 0 and date ≤ today
            backend: upsert AssetPrice(asset_id, date, price)
            backend: publish AssetPriceUpdated (bare signal)
        → modal closes + snackbar
        → Account Details re-fetches on AssetPriceUpdated
        → holding row: current price, unrealized P&L, performance % updated
```

### Workflow — Price history CRUD (MKT-070+)

```
Account Details (active holding row)
    → "Price history" button (MKT-070)
    → PriceHistoryView opens; list fetched via MKT-072
        → sorted date descending; Edit + Delete per row
        → "Add price" action → same flow as "Enter price" (MKT-020–029)

Edit row:
    → Edit form opens pre-filled with (date, price)
    → user changes date and/or price
    → submit
        same date → in-place update (MKT-083)
        new date  → delete old + upsert at new date (MKT-084)
        on failure → rollback, form stays open (MKT-087)
    → success: form closes + snackbar + list refreshes + AssetPriceUpdated (MKT-085, MKT-086)
    → Account Details re-fetches on AssetPriceUpdated (MKT-076)

Delete row:
    → confirmation dialog identifies (date, price) (MKT-089)
    → user confirms
        backend: delete_asset_price(asset_id, date) (MKT-090)
        backend: publish AssetPriceUpdated (MKT-091)
    → list refreshes; Account Details re-fetches (MKT-076)
    → if deleted entry was most recent price: holding row shows "—" or next price
```

### Workflow — Auto-record from a buy/sell transaction (MKT-050+)

```
Settings page
    → "Automatically record transaction price as market price" toggle
    → choice persisted to localStorage (default OFF)

Buy/Sell transaction form (create or edit)
    → checkbox "Use this price as the market price for {date}"
        create mode → default = global toggle snapshot at open (MKT-052)
        edit mode   → default = OFF (MKT-052)
    → user submits
        frontend: record_price: bool added to CreateTransactionDTO (MKT-054)
        backend (atomic DB transaction):
            ├─ insert/update Transaction
            └─ recompute Holding (TRX-027 / SEL-025)
        → commit; on any failure rollback the transaction DB operation
            transaction context publishes TransactionUpdated              (B8)
        → if record_price && tx.unit_price > 0 (MKT-055, MKT-061):
            frontend calls record_asset_price(asset_id, date, price)     (separate command, best-effort)
            on success: asset context publishes AssetPriceUpdated         (MKT-057, B8)
            on failure: logged as warning, silently dropped               (MKT-056, MKT-062)
        → Account Details re-fetches via AssetPriceUpdated                (MKT-036)
```

### Workflow — Auto-fetch on launch and on user demand (MKT-100+)

```
App launch
    → frontend completes initial mount
    → frontend reads auto-fetch setting from FE store                      (MKT-120)
        if OFF (default): no launch call; user can still trigger refresh
        if ON: frontend calls auto-fetch task once per session             (MKT-121, fire-and-forget)
    → backend handler dispatches background job and returns immediately   (MKT-122)
    → background job:
        ├─ if no active/derivable holdings in scope: exit early           (MKT-111)
        for each active holding asset:
            ├─ derive provider symbol from Asset.reference                (MKT-110, ADR-008)
            ├─ if symbol unmappable OR provider fetch fails: skip silently (MKT-114, logged warning)
            └─ on success: upsert (asset_id, today, price, source=Stooq)  (MKT-025, MKT-102)
                          publish AssetPriceUpdated                       (MKT-112)
    → subscribers re-fetch on AssetPriceUpdated                            (MKT-036)

User clicks "Refresh prices" on the global dashboard
    → frontend calls global refresh                                        (MKT-130, fire-and-forget)
    → backend uses the same entry point as launch                          (MKT-122)
    → events fire per MKT-112; UI updates reactively

User clicks "Refresh prices" on an account detail page
    → frontend calls account refresh with account_id                       (MKT-131, fire-and-forget)
    → backend: rejects with specific error if account_id unknown          (MKT-132)
    → backend dispatches a background job scoped to that account          (MKT-132)
        if no active/derivable holdings in scope: exit early              (MKT-111)
        system cash assets excluded                                        (MKT-116)
        otherwise same per-asset behavior as launch                        (MKT-110, MKT-114, MKT-112)

In-flight guard (all fetch paths)
    → if any fetch task (launch, global, account) is already running,
      a subsequent task call is rejected with a specific error            (MKT-113)

User-triggered refresh feedback (FE)
    → button disabled + spinner while awaiting BE ack                     (MKT-133)
    → snackbar on dispatch success ("Fetching prices…")                   (MKT-115)
    → snackbar on in-flight rejection ("Fetch already in progress")       (MKT-115)
```

---

## UX Draft

### Entry Point

"Enter price" icon button on each active holding row in Account Details, in the actions column alongside Buy and Sell. Not shown on closed holdings.

### Main Component

Small modal dialog. No navigation — stays within Account Details.

### Form Fields

| Field          | Default                                   | Editable |
| -------------- | ----------------------------------------- | -------- |
| Asset name     | Pre-filled from holding row               | No       |
| Date           | Today                                     | Yes      |
| Price          | Today's existing price if any, else empty | Yes      |
| Currency label | Asset's native currency code              | No       |

### States

- **Submit in-flight** (MKT-027): Submit button disabled + spinner while persisting.
- **Validation / backend error** (MKT-029): Inline error adjacent to the invalid field; modal stays open.
- **Success** (MKT-028): Modal closes; snackbar "Price recorded."
- **No price (holding row)**: "—" in Current Price, Unrealized P&L, Performance % columns.
- **Currency mismatch (holding row)**: Current Price shown in asset currency; Unrealized P&L and Performance % show "—".

### User Flow

1. User views Account Details for an account.
2. User clicks "Enter price" on a holding row.
3. Modal opens immediately (no fetch) with asset name, today's date, and price pre-filled if a same-day entry exists.
4. User types the current market price (in asset currency, shown as a label).
5. User optionally changes the date (e.g. to use yesterday's closing price).
6. User submits.
7. Backend validates, upserts the price, publishes `AssetPriceUpdated`.
8. Modal closes, snackbar confirms.
9. Account Details re-fetches: the holding row now shows current price, unrealized P&L, performance %.

### UX Draft — Auto-record from Transactions (MKT-050+)

#### Settings page

A new toggle row "Automatically record transaction price as market price" sits alongside the existing language preference. Default OFF. State persists across sessions on the current device.

#### Buy and sell transaction forms

A checkbox is added directly above the submit button in:

- the buy creation modal,
- the sell creation modal,
- the standalone /transactions/new form (whether the transaction is a buy or a sell),
- the edit variants of all the above.

| Field             | Default                                                 | Editable |
| ----------------- | ------------------------------------------------------- | -------- |
| Auto-record price | Snapshot of global toggle on create; always OFF on edit | Yes      |

The label updates live with the form's date field: "Use this price as the market price for 2026-04-27".

#### States

- **Checkbox unchecked**: no behaviour change; the form behaves exactly as before this feature.
- **Checkbox checked + submit success**: the standard transaction success path (snackbar, modal close, list refresh) is unchanged. The new `AssetPriceUpdated` event causes Account Details to refresh its market-price columns transparently.
- **Submit failure with checkbox checked**: the form remains open with the standard inline transaction error feedback. No price is written because the transaction did not commit. The user can untick the checkbox or correct the inputs and retry.
- **Price write failure after successful transaction**: the transaction is committed and the modal closes normally. The price write failure is silent (MKT-062); the user can record the price manually via the price history modal (MKT-070+).
- **Same-day price already recorded**: no warning shown; the existing entry is silently overwritten on submit (MKT-058).
- **Zero `unit_price`**: when the buy/sell unit price is `0` (gifted asset, TRX-020), the auto-record step is silently skipped per MKT-061. The transaction itself succeeds normally; the checkbox state has no observable effect in this case.

#### User flow — global default

1. User opens Settings.
2. User flips "Automatically record transaction price as market price" ON.
3. User opens a buy form anywhere in the app — the auto-record checkbox is pre-checked.
4. User submits; the unit price is recorded as the day's market price in addition to the transaction.

#### User flow — per-transaction override

1. Global toggle is OFF (default).
2. User opens a sell form for an asset they want to also stamp a market price for today.
3. User ticks the auto-record checkbox manually before submitting.
4. User submits; the price is recorded for this transaction only. The next form opens with the box unchecked again.

### UX Draft — Price History CRUD (MKT-070+)

#### Entry Point

"Price history" icon button on each active holding row in Account Details, in the actions column alongside Buy, Sell, and "Enter price". Not shown on closed holdings.

#### Main Component

Modal or side panel listing all recorded `AssetPrice` entries for the selected asset (per asset, not per account).

#### States

- **Loading**: spinner while prices are fetched on open (MKT-074).
- **Empty**: "No prices recorded yet" message with an "Add price" button (MKT-073).
- **Populated**: date-descending list; each row shows date, price (in asset currency), Edit button, Delete button.
- **Edit form** (inline within modal, or nested modal): date + price fields, pre-filled; same inline error feedback as "Enter price" form (MKT-087).
- **In-flight (edit)**: Submit button disabled + spinner while the edit request is in progress (MKT-094).
- **Delete confirmation** (standard ConfirmationDialog): identifies date + price of the record to be removed (MKT-089).
- **In-flight (delete)**: Delete button for the targeted row is disabled while the request is in progress (MKT-093).
- **Delete error**: error banner at the top of the list; entry remains visible (MKT-096).
- **Fetch error**: inline error with retry (MKT-074).

#### User Flow — View and delete a stale price

1. User views Account Details and notices a price in the current-price column that looks wrong.
2. User clicks "Price history" on that holding row.
3. Modal opens; list is fetched and displayed date-descending.
4. User locates the stale entry, clicks Delete.
5. Confirmation dialog appears identifying the date and price.
6. User confirms.
7. Backend deletes the record, publishes `AssetPriceUpdated`.
8. History list refreshes; Account Details re-fetches — holding row now shows the next most recent price or "—".

#### User Flow — Correct a price with a wrong date

1. User opens "Price history" for a holding.
2. User finds an entry recorded on the wrong date (e.g. recorded on 2026-04-28 but should be 2026-04-27).
3. User clicks Edit on that row.
4. Edit form opens pre-filled with the wrong date and the price.
5. User changes the date to 2026-04-27.
6. User submits.
7. Backend deletes the 2026-04-28 record and upserts at 2026-04-27 (MKT-084).
8. Edit form closes; history list refreshes; snackbar confirms (MKT-086).

### UX Draft — Auto-Fetch from External Provider (MKT-100+)

#### Settings page

The existing Settings page (host of MKT-050's transaction auto-record toggle) gains a second toggle: "Automatically fetch prices on launch" (MKT-120). Default OFF. Sits above the existing transaction-related toggle to group all price-related settings together.

#### Global dashboard — Refresh action

A "Refresh prices" button is added to the global dashboard, placed in the page header. Pressing it triggers the global refresh (MKT-130). The button is disabled with a spinner while the refresh is being acknowledged (MKT-133); on dispatch, a snackbar acknowledges the fetch (MKT-115). Per-asset failures during the fetch degrade silently per MKT-114. On in-flight collision (MKT-113), a snackbar indicates a fetch is already in progress (MKT-115); the user's action is rejected without disrupting the ongoing fetch.

#### Account Details — Refresh action

A "Refresh prices" button is added to the Account Details view, placed in the page header. Pressing it triggers the account refresh (MKT-131) scoped to that account's holdings. Same feedback and in-flight semantics as the global refresh (MKT-113, MKT-114, MKT-115, MKT-133).

#### Account Details — Current Price column secondary label and source badge

The "Current Price" column displays the price as before (MKT-030), with a secondary label below showing the staleness phrasing per MKT-140: "Updated today" or "Updated Nd ago". A small source badge (per MKT-142) sits alongside the price or near the staleness label so the user can tell auto-fetched values from manual entries without opening the price history. Subdued typography on both label and badge to avoid competing with the price value itself.

#### Price-history modal — Source badge

Each row in the price-history list (MKT-071) gains a small badge to the right of the date showing the row's `source` value: "Manual", "Stooq", etc. (MKT-141). The badge uses neutral styling — it's informational, not a status pill.

#### States

- **Initial launch with auto-fetch OFF (default)**: the dashboard renders with whatever stored prices exist; missing prices show "—". The user can hit "Refresh prices" on the global dashboard or on an account detail page to fetch on demand.
- **Initial launch with auto-fetch ON (user-enabled)**: prices appear as the background job completes; per-row updates are reactive via `AssetPriceUpdated` events. No global spinner or progress UI — the UI fills in row-by-row as data arrives.
- **Refresh button awaiting ack**: button disabled + spinner until BE acknowledges dispatch (MKT-133).
- **Refresh dispatched**: snackbar "Fetching prices…" (MKT-115); rows update reactively as the background job completes.
- **Concurrent refresh attempt**: BE returns the in-flight error (MKT-113); snackbar "Fetch already in progress" (MKT-115); ongoing fetch undisturbed.
- **Account refresh on unknown account**: BE returns a specific error (MKT-132); FE surfaces it via the standard error pipeline.
- **Asset with no provider coverage**: row's "Current Price" stays at "—" indefinitely. The user can enter a manual value via "Enter price" (MKT-010).
- **System cash holding**: excluded from fetch scope (MKT-116); no warning, no badge change.

#### User flow — first launch of the day (auto-fetch enabled by the user)

1. User opens the app.
2. Frontend mounts. Auto-fetch setting is `ON` (MKT-120), so the frontend calls the auto-fetch task (MKT-121). Backend returns immediately and dispatches the background job (MKT-122).
3. The dashboard renders immediately with stored prices (likely yesterday's for active holdings).
4. As each asset's fetch completes in the backend job, `AssetPriceUpdated` events fire; the Current Price column updates reactively; the staleness label flips to "Updated today".
5. User sees a fully refreshed dashboard within seconds (typical case: ~5–50 holdings).

#### User flow — enable auto-fetch

1. User opens Settings.
2. User toggles "Automatically fetch prices on launch" to ON.
3. Setting persists in the FE store; no immediate fetch (the launch trigger fires once per session at mount).
4. On the next launch, the auto-fetch task runs (MKT-121).
5. User can still hit "Refresh prices" (global or account) at any time to fetch on demand.

#### User flow — manual override of a fetched value

1. Auto-fetch writes `AssetPrice(AAPL, 2026-05-17) = $192, source: Stooq`.
2. User notices the "Stooq" badge and disagrees with the value (e.g. corporate-action edge case).
3. User opens "Enter price" or "Price history", enters $189.
4. Backend writes `AssetPrice(AAPL, 2026-05-17) = $189, source: Manual` — overwrites the Stooq row (per ADR-012; MKT-025, MKT-101).
5. Account Details shows $189; the badge becomes "Manual".
6. On the next launch, auto-fetch will overwrite $189 with the new day's Stooq value (per ADR-012). The user's correction is for today; tomorrow brings tomorrow's price.

---

## Open Questions

- [x] **OQ-1** — Should the standalone "Enter price" action on the holding row (MKT-010) be removed? **Decision: keep both.** MKT-010 remains as a fast path for the common case (record today's price without opening history). MKT-070 adds a second "Price history" action for review and corrections. The holding row will have five actions: Buy, Sell, Enter price, Price history, and the transaction magnifier.

None — all open questions have a recorded decision.
