# Plan — Market Price Auto-Fetch Amendment (MKT-100..MKT-142)

> **Scope** — deltas only. Existing MKT rules (MKT-010..MKT-096) are already implemented; this plan covers the new auto-fetch surface plus the additive `AssetPrice.source` field. Existing manual / transaction-auto-record / price-history-edit write paths get a single-line touch each to stamp `source = Manual` (MKT-101).
>
> **Spec**: `docs/spec/market-price.md` (sections "Source field on AssetPrice 100–109" and "Auto-Fetch from External Provider 110–149")
> **Contract**: `docs/contracts/asset-contract.md` (new "Asset Price Fetch Tasks" section + `AssetPrice.source` field)
> **ADRs**: ADR-008 (Stooq primary), ADR-012 (latest-write-wins, source is metadata)
> **Error model**: this plan is the **first adopter** of `docs/error-model.md` — one flat enum per BC for new code, one composite per use case wrapping BC enums via `#[from]` + flat use-case-specific variants. Treat the rules in that doc as binding.

---

## New convention adopted

The auto-fetch surface introduces brand-new error types. They MUST follow `docs/error-model.md` strictly:

- **New BC enum `AssetError`** at `src-tauri/src/context/asset/error.rs` — flat, one variant per failure mode the fetch path actually surfaces. `#[serde(tag = "code")]`. Scoped to the new fetch surface ONLY. The existing `AssetApplicationError` / `AssetPriceApplicationError` / `CategoryApplicationError` / `AssetCrudError` / `AssetPriceError` / `CategoryCrudError` stay untouched (see "Follow-ups / Out-of-scope" for the retrofit ticket).
- **Reuse `AccountApplicationError` as-is** — it already exposes `AccountNotFound { account_id }` and `DatabaseError`, the only account-side variants the account-refresh path needs. Wrap directly via `#[from]`; no alias, no transition comment.
- **Per-use-case composites** with `#[serde(untagged)]`. Each fetch use case owns its own composite (`FetchAllAssetPricesError`, `FetchAccountAssetPricesError`) wrapping the BC enums plus flat use-case variants (`FetchAlreadyRunning`, `NoFetchableHoldings`, `UnknownError`).
- **Tauri command boundary**: the composite IS the wire-facing type — no mapper, no boundary type, no `Result<T, String>`.
- **Naming**: no rationale-as-comment ("Per the X rule", "Replaces the anyhow-era Y"). Doc comments describe what the code IS.

---

## Workflow TaskList

### Phase 1 — Backend (PR 1)

- [ ] Review architecture & rules (`ARCHITECTURE.md`, `docs/backend-rules.md`, `docs/ddd-reference.md`, `docs/error-model.md`, ADR-008, ADR-012)
- [ ] Database migration: filename per existing convention in `src-tauri/migrations/` (inspect the most recent migration for the exact `YYYYMMDDhhmmss_` or `YYYYMMDD_NNNN_` pattern). Body: `ALTER TABLE asset_prices ADD COLUMN source TEXT NOT NULL DEFAULT 'Manual';` then `UPDATE asset_prices SET source = 'Manual';` (idempotent — the DEFAULT handles new inserts; the explicit UPDATE makes the backfill intent self-documenting).
- [ ] `just migrate`
- [ ] `just prepare-sqlx` (regenerate `.sqlx` cache so the new `source` column reads/writes compile)
- [ ] Backend test stubs (`test-writer-backend` — all stubs written, red confirmed)
  - **New commands to stub** (contract domain: `asset`): `fetch_all_asset_prices`, `fetch_account_asset_prices` — full per-command coverage (success dispatch, `FetchAlreadyRunning`, `NoFetchableHoldings`, `AccountNotFound` for the account-scoped variant). These are the bulk of the new test surface.
  - **Modified functions** (existing-method touch-ups): [`AssetService::record_asset_price`], [`AssetService::update_asset_price`] (both now stamp `source = Manual` per MKT-101); [`SqliteAssetPriceRepository::upsert`], [`SqliteAssetPriceRepository::replace_atomic`], [`SqliteAssetPriceRepository::get_latest`], [`SqliteAssetPriceRepository::get_all_for_asset`], [`SqliteAssetPriceRepository::get_by_asset_and_date`] (now read/write the new `source` column); [`AssetPrice::new`], [`AssetPrice::restore`] (new `source` parameter).
- [ ] Backend implementation (minimal — make failing tests pass, green confirmed). In order:
  1. `AssetPriceSource` enum in `context/asset/domain/asset_price.rs` (`Manual | Stooq`, `#[derive(Serialize, Deserialize, Type, Clone, Debug, PartialEq, Eq)]`)
  2. `AssetPrice` struct gains `source: AssetPriceSource`; factories `new` / `restore` updated; existing tests adjusted
  3. `SqliteAssetPriceRepository` columns + queries updated to read/write `source` (SQLite TEXT discriminant matches enum variant name verbatim per ADR-008)
  4. `AssetService::record_asset_price` / `update_asset_price` hardcode `source = Manual` (MKT-101)
  5. New `context/asset/error.rs` flat `AssetError` enum (variants: `DatabaseError`; `AssetNotFound { id }` only if the dispatcher needs it — decide while implementing the dispatcher; if assets are loaded by the orchestrator via existing `AssetService::get_all_assets` / `AccountService` calls that already raise their own `*ApplicationError::NotFound`, skip this variant)
  6. `context/asset/domain/asset_price.rs` gains `PriceProvider: Send + Sync` trait + `#[cfg_attr(test, mockall::automock)]`. Method signature: `async fn fetch_price(&self, symbol: &str) -> Result<i64>` returning i64 micros, mappable to a per-asset skip on error (MKT-114).
  7. `context/asset/domain/stooq_symbol.rs` — pure function `derive_stooq_symbol(reference: &str) -> Option<String>` per ADR-008 (initial v1: lowercase + naive exchange-suffix mapping; the function is the single place future heuristics get added). Unit tests cover known shapes and the unmappable-returns-`None` branch.
  8. `context/asset/repository/stooq_client.rs` — `ReqwestStooqClient` implements `PriceProvider`. Hits `https://stooq.com/q/?s={symbol}&f=...&e=csv`, parses the CSV body, converts to i64 micros. Errors map to a per-asset skip in the dispatcher (MKT-114) — not surfaced.
  9. `use_cases/asset_price_fetch/mod.rs` declaring child modules
  10. `use_cases/asset_price_fetch/guard.rs` — `FetchGuard { running: AtomicBool }`; `try_acquire(&self) -> Option<FetchGuardLease>` returning an RAII `FetchGuardLease` whose `Drop` clears the flag (so panics unblock the next fetch). Single registered instance per app (managed via `Arc<FetchGuard>` Tauri state).
  11. `use_cases/asset_price_fetch/dispatcher.rs` — `Dispatcher` struct holding `Arc<dyn PriceProvider>`, `Arc<dyn AssetPriceRepository>`, `Arc<SideEffectEventBus>`, and an injected clock (`Arc<dyn Clock>` or `fn() -> NaiveDate` — pick the simplest pattern; tests need to fix "today" deterministically). `spawn(self: Arc<Self>, scope: Vec<(Asset, String)>, lease: FetchGuardLease)` spawns a Tokio task that iterates pre-derived `(Asset, symbol)` pairs (cash filter + symbol derivation already done by the use case per MKT-111 ordering), calls `provider.fetch_price(&symbol)`, **unconditionally upserts** on success with `source: Stooq` and `date = clock.today()` (MKT-102 — no read-before-write, no source-precedence check, per ADR-012 latest-write-wins), publishes `AssetPriceUpdated` per write (MKT-112). All per-asset HTTP / parse / upsert failures `tracing::warn!`'d and silently skipped (MKT-114). The `lease` is moved into the task and dropped at task end (success, normal completion, or panic — RAII unblocks the guard).
  12. `use_cases/asset_price_fetch/all.rs` — `FetchAllAssetPricesUseCase` with `Arc<AccountService>`, `Arc<AssetService>`, `Arc<FetchGuard>`, `Arc<Dispatcher>`. `run()`: (a) acquire guard, else `FetchAlreadyRunning`; (b) load all active holdings across all accounts via `AccountService`; (c) filter out system cash assets (MKT-116; use `crate::core::cash::system_cash_asset_id` to detect the prefix); (d) derive Stooq symbol per holding via `stooq_symbol::derive_stooq_symbol`, discarding entries where derivation returns `None`; (e) if the remaining `Vec<(Asset, String)>` is empty → `NoFetchableHoldings` (MKT-111 — "active AND derivable symbol"); (f) `Dispatcher::spawn(scope, lease)` (which takes ownership of the lease); (g) return `Ok(())`. + `FetchAllAssetPricesError` composite.
  13. `use_cases/asset_price_fetch/account.rs` — `FetchAccountAssetPricesUseCase` mirror with `account_id` parameter. `run(&self, account_id)`: existence check via `AccountService::get_by_id` (raises `AccountApplicationError::AccountNotFound` when `Ok(None)`), then steps (a)–(g) from `all.rs` scoped to the account's holdings. + `FetchAccountAssetPricesError` composite.
  14. `use_cases/asset_price_fetch/api.rs` — two Tauri commands wired through `State`
  15. Register `FetchGuard`, `FetchAllAssetPricesUseCase`, `FetchAccountAssetPricesUseCase`, `Arc<dyn PriceProvider>` (= `Arc::new(ReqwestStooqClient::new())`), and `Dispatcher` in `lib.rs` (parallel to existing `UpdateState`, `account_details_uc`, etc. wiring)
  16. Register both commands in `core/specta_builder.rs` `collect_commands![]`
- [ ] `just format` (rustfmt + clippy --fix)
- [ ] `reviewer-backend` → fix issues
- [ ] `just generate-types` (Specta regenerates `src/bindings.ts`) — re-run if `reviewer-backend` touches any Specta-exposed type (commands, error enums, shared structs)
- [ ] Compilation fixup (TypeScript errors from the new `source` field on `AssetPrice` reads in existing presenter / hook call sites — narrow, no UI work yet)
- [ ] `just check` — TypeScript clean
- [ ] `/smart-commit` — suggested title: `feat(asset): auto-fetch backend + source field`
- [ ] `/create-pr` — branch `feat/mkt-stooq-autofetch-be` → PR 1

### Phase 2 — Frontend (PR 2, branched off merged PR 1)

- [ ] Frontend test stubs (`test-writer-frontend` — all stubs written, red confirmed)
  - **New gateway methods to stub**: [`accountDetailsGateway.ts:fetchAllAssetPrices`], [`accountDetailsGateway.ts:fetchAccountAssetPrices`].
  - **Modified functions**: [`useSettings.ts:toggleAutoFetch`] (new sibling of `toggleAutoRecordPrice`), [`presenter.ts:formatStaleness`] (new pure helper, MKT-140), [`presenter.ts:toHoldingRow`] (now formats staleness + source badge — MKT-140 / MKT-142), [`useRefreshGlobalPrices.ts:refresh`] (MKT-115 / MKT-133), [`useRefreshAccountPrices.ts:refresh`] (MKT-115 / MKT-133).
- [ ] Frontend implementation (minimal — make failing tests pass, green confirmed)
- [ ] `just format`
- [ ] `/visual-proof` — capture: AccountManager header (refresh button idle + spinner), AccountDetails header (refresh button idle + spinner), AccountDetails Current Price column (with staleness label + source badge variants Manual/Stooq + `—` fallback), PriceHistoryModal row (with source badge), SettingsPage (new auto-fetch toggle alongside auto-record-price). All in light + dark.
- [ ] `reviewer-frontend` → fix issues
- [ ] `/smart-commit` — suggested title: `feat(asset): auto-fetch frontend UI + settings toggle`
- [ ] `/create-pr` — branch `feat/mkt-stooq-autofetch-fe` → PR 2

### Phase 3 — E2E + closure (PR 3)

- [ ] E2E scenarios (`test-writer-e2e`) — happy path for each refresh entry point; in-flight rejection path; no-fetchable-holdings rejection path; source badge visible in AccountDetails + PriceHistoryModal after a fetch. Tests must stub the Stooq HTTP layer (use `ReqwestStooqClient` injection seam or run with `MockPriceProvider` registered via a test-only fixture — confirm pattern at write time)
- [ ] Run `npm run test:e2e` → green
- [ ] `reviewer-e2e` → fix issues in E2E files
- [ ] Cross-cutting reviews: `reviewer-arch` (always); `reviewer-sql` (migration touches `asset_prices`); `reviewer-infra` (new Tauri state + command registration in `lib.rs` and `specta_builder.rs`); `reviewer-security` (new outbound HTTP to `stooq.com` — capability allowlist + CSP)
- [ ] Update `ARCHITECTURE.md`:
  - Event bus row: `AssetPriceUpdated` now also fires from `use_cases/asset_price_fetch/dispatcher.rs`
  - Backend section: add `use_cases/asset_price_fetch/` subsection (mirror the `asset_web_lookup/` entry)
  - Backend section: add `context/asset/error.rs` to the asset BC listing
  - Backend section: add `context/asset/repository/stooq_client.rs` + the new `PriceProvider` trait + `derive_stooq_symbol` pure function
  - Migrations list: append the new `add_source_to_asset_prices` migration
  - Asset BC Tauri commands: add `fetch_all_asset_prices`, `fetch_account_asset_prices`
  - FE features: AccountManager gains a "Refresh prices" header button (MKT-130); Settings gains the auto-fetch toggle
- [ ] Update `docs/todo.md`:
  - Strike or rewrite the "(spec) — Amend MKT spec: add source field + Stooq auto-fetch rules" entry (now landed)
  - Add the asset-BC error retrofit entry (see Follow-ups below)
  - Confirm the existing "(mkt) — Surface fetch-task completion to FE" entry stays as-is — it intentionally remains open
- [ ] `spec-checker` → green
- [ ] `/smart-commit` — suggested title: `test(asset): e2e auto-fetch + docs`
- [ ] `/create-pr` — branch `feat/mkt-stooq-autofetch-e2e` → PR 3 → merge

---

## Migrations

| File                                                              | Columns / Operations                                                                                                           | Notes                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/migrations/{TIMESTAMP}_add_source_to_asset_prices.sql` | `ALTER TABLE asset_prices ADD COLUMN source TEXT NOT NULL DEFAULT 'Manual';` then `UPDATE asset_prices SET source = 'Manual';` | SQLite supports the `ADD COLUMN ... NOT NULL DEFAULT` form. The UPDATE is technically redundant given the DEFAULT, but it documents the backfill intent and is safe (idempotent). Filename `{TIMESTAMP}` follows the existing convention in `src-tauri/migrations/` — inspect the most recent migration file for the exact pattern before authoring. Run `just migrate` then `just prepare-sqlx` to regenerate `.sqlx` cache. |

---

## Detailed Implementation Plan

### Backend — Domain

**`src-tauri/src/context/asset/domain/asset_price.rs`** (modify):

- Add `enum AssetPriceSource { Manual, Stooq }` with `Serialize + Deserialize + specta::Type + Clone + Debug + PartialEq + Eq`. No `Finnhub` variant — deferred to KEY spec per ADR-008.
- `AssetPrice` struct gains `pub source: AssetPriceSource`.
- `AssetPrice::new` signature: `(asset_id, date, price, source)`. Validation unchanged.
- `AssetPrice::restore` signature: `(asset_id, date, price, source)`.
- Add the `PriceProvider` trait at the bottom of the file (mirrors `AssetPriceRepository`), `#[cfg_attr(test, mockall::automock)]`, `async fn fetch_price(&self, symbol: &str) -> Result<i64>`.

**`src-tauri/src/context/asset/domain/stooq_symbol.rs`** (new):

- `pub fn derive_stooq_symbol(reference: &str) -> Option<String>` — v1 implementation: lowercases the reference; if the reference already contains a `.` (already a Stooq-style symbol), passes it through lowercased; otherwise returns `Some(lowercased)` as the bare ticker (US default) — confirm exact heuristic when implementing against ADR-008's "lowercasing the ticker and appending an exchange suffix" guidance. Unmappable inputs (empty / non-ASCII) return `None`. Pure function — no I/O.
- Unit tests for: bare ticker (`"AAPL"` → `"aapl"`), Euronext-style (per ADR-008 example `TTE` with French context → `"tte.fr"` — confirm during implementation whether v1 carries exchange info), unmappable returns `None`. Re-derive the exact transformation rules from ADR-008 during the implementation step (no spec mandate forces a particular shape beyond "derived from `Asset.reference`").

**`src-tauri/src/context/asset/domain/mod.rs`** (modify):

- Export `AssetPriceSource`, `PriceProvider`, `derive_stooq_symbol`.

### Backend — Application / Error

**`src-tauri/src/context/asset/error.rs`** (new):

- Flat `pub enum AssetError` per `docs/error-model.md`:
  ```
  #[serde(tag = "code")]
  enum AssetError {
      DatabaseError,
      // (Only add other variants when the dispatcher / use case actually surfaces them.
      // The orchestrator's "load assets" path goes through AccountService /
      // AssetService whose existing *ApplicationError types already carry NotFound;
      // those flow up via their own #[from] wrappers in the composite.)
  }
  ```
- Decide during implementation whether `AssetNotFound { id }` is needed. If the dispatcher silently skips unknown asset ids per MKT-114, it isn't.

**`src-tauri/src/context/asset/mod.rs`** (modify):

- Re-export `AssetError` from `error.rs`.

### Backend — Infrastructure

**`src-tauri/src/context/asset/repository/asset_price.rs`** (modify):

- All `INSERT ... ON CONFLICT DO UPDATE` and `SELECT` statements include the `source` column.
- `AssetPrice::restore` call sites pass `r.source.parse::<AssetPriceSource>()?` (use `FromStr`/`TryFrom` — or a small `match` on the TEXT discriminant; pick the simpler path at implementation time).
- `upsert(price)` writes `price.source.as_str()` into the column (TEXT discriminant matches enum variant name verbatim per ADR-008).
- `replace_atomic(asset_id, original_date, new_price)` writes `new_price.source`.
- Update existing `#[cfg(test)] mod tests` to seed the new `source` column.

**`src-tauri/src/context/asset/repository/stooq_client.rs`** (new):

- `pub struct ReqwestStooqClient { client: reqwest::Client }`
- `impl PriceProvider for ReqwestStooqClient`
- `fetch_price(&self, symbol)` builds `https://stooq.com/q/?s={symbol}&f=sd2t2ohlcv&i=d&e=csv` (confirm field list against ADR-008 / a quick manual probe), parses the CSV, extracts the close price, converts to i64 micros via `decimal_to_micros` (locate or add a helper in `core/` if missing). Non-2xx, parse-failure, or missing-row paths return `Err(anyhow::anyhow!(...))` — the dispatcher translates to a silent per-asset skip (MKT-114).
- `pub fn new() -> Self` — `reqwest::Client::builder().timeout(Duration::from_secs(10)).build()`.

**`src-tauri/src/context/asset/repository/mod.rs`** (modify):

- Re-export `ReqwestStooqClient`.

### Backend — Use case

**`src-tauri/src/use_cases/asset_price_fetch/`** (new folder):

- `mod.rs` — re-export `FetchAllAssetPricesUseCase`, `FetchAllAssetPricesError`, `FetchAccountAssetPricesUseCase`, `FetchAccountAssetPricesError`, `FetchGuard`.
- `guard.rs` — `pub struct FetchGuard { running: AtomicBool }`; `pub fn new() -> Self`; `pub fn try_acquire(self: &Arc<Self>) -> Option<FetchGuardLease>`. `FetchGuardLease` impls `Drop` to clear the flag — RAII so panics unblock the next fetch.
- `dispatcher.rs` — `pub struct Dispatcher { provider, price_repo, event_bus, clock }` where `clock` is an injected source of "today" (e.g. `Arc<dyn Clock>` with a `today() -> NaiveDate` method, or `Fn() -> NaiveDate` trait object) so tests can fix the date deterministically. `pub fn spawn(self: Arc<Self>, scope: Vec<(Asset, String)>, lease: FetchGuardLease)` spawns a `tokio::task::spawn` task that iterates pre-derived `(asset, symbol)` pairs (cash filtering + symbol derivation are the use case's responsibility per MKT-111), calls `provider.fetch_price(&symbol)` (on `Err`, log `tracing::warn!` and continue per MKT-114), builds `AssetPrice::new(asset.id, clock.today(), price, AssetPriceSource::Stooq)`, calls `price_repo.upsert` (unconditional — no read-before-write, per ADR-012 latest-write-wins / MKT-102), publishes `Event::AssetPriceUpdated`. `lease` is moved into the task and dropped at task end (success, normal completion, or panic).
- `all.rs` — `FetchAllAssetPricesUseCase` + `FetchAllAssetPricesError`. Per `docs/error-model.md` § Recipes § Use-case composite, the composite mixes wrapper variants (via `#[from]`) for the BC enums AND flat use-case-specific variants as siblings — no nested `*GuardError` enum (that's an anti-pattern):
  ```rust
  #[derive(thiserror::Error, Debug, Serialize, specta::Type)]
  #[serde(untagged)]
  pub enum FetchAllAssetPricesError {
      #[error(transparent)] Asset(#[from] AssetError),
      #[error(transparent)] Account(#[from] AccountApplicationError),
      #[error("A fetch task is already running")] FetchAlreadyRunning,
      #[error("No fetchable holdings in scope")] NoFetchableHoldings,
      #[error("Unexpected error")] UnknownError,
  }
  ```
- `account.rs` — `FetchAccountAssetPricesUseCase` + `FetchAccountAssetPricesError`. Mirrors `all.rs` plus the existence-check call `account_service.get_by_id(account_id).await?` whose `AccountApplicationError::AccountNotFound` propagates via the `#[from] AccountApplicationError` arm.
- `api.rs`:

  ```rust
  #[tauri::command] #[specta::specta]
  pub async fn fetch_all_asset_prices(
      uc: State<'_, Arc<FetchAllAssetPricesUseCase>>,
  ) -> Result<(), FetchAllAssetPricesError> { uc.run().await }

  #[tauri::command] #[specta::specta]
  pub async fn fetch_account_asset_prices(
      uc: State<'_, Arc<FetchAccountAssetPricesUseCase>>,
      account_id: String,
  ) -> Result<(), FetchAccountAssetPricesError> { uc.run(&account_id).await }
  ```

**`src-tauri/src/use_cases/mod.rs`** (modify):

- `pub mod asset_price_fetch;`

**`src-tauri/src/lib.rs`** (modify):

- Construct `let stooq_client: Arc<dyn PriceProvider> = Arc::new(ReqwestStooqClient::new());`
- Construct `let fetch_guard = Arc::new(FetchGuard::new());`
- Construct `let dispatcher = Arc::new(Dispatcher::new(stooq_client.clone(), asset_price_repo.clone(), event_bus.clone()));`
- Construct `let fetch_all_uc = Arc::new(FetchAllAssetPricesUseCase::new(account_service.clone(), asset_service.clone(), fetch_guard.clone(), dispatcher.clone()));`
- Construct `let fetch_account_uc = Arc::new(FetchAccountAssetPricesUseCase::new(account_service.clone(), asset_service.clone(), fetch_guard.clone(), dispatcher.clone()));`
- `app_handle.manage(fetch_all_uc);`, `app_handle.manage(fetch_account_uc);`, `app_handle.manage(fetch_guard.clone());` (parallel to existing `manage` calls around line 180). Exposing `FetchGuard` via `manage` gives integration tests a direct handle to assert guard behavior without going through a use case.

**`src-tauri/src/core/specta_builder.rs`** (modify):

- Append `fetch_all_asset_prices`, `fetch_account_asset_prices` to the `collect_commands![]` list.

### Frontend

**`src/features/account_details/gateway.ts`** (modify):

- Add `fetchAllAssetPrices(): Promise<Result<null, FetchAllAssetPricesError>>` → `commands.fetchAllAssetPrices()`.
- Add `fetchAccountAssetPrices(accountId): Promise<Result<null, FetchAccountAssetPricesError>>` → `commands.fetchAccountAssetPrices(accountId)`.
- (Note: gateway placement piggy-backs on the existing `account_details` feature since the refresh button on `AccountDetailsView` lives in the same feature. The "global refresh" button placement in `AccountManager` borrows the same gateway via re-export — see `features/accounts/gateway.ts` mod below.)

**`src/features/accounts/gateway.ts`** (modify):

- Add a thin `fetchAllAssetPrices()` method that calls `commands.fetchAllAssetPrices()` directly. Per F3 ("gateway owns the commands.\* call within the feature that triggers it"), do NOT re-export from `accountDetailsGateway` — the AccountManager refresh button belongs to the `accounts` feature and owns its own gateway call.

**`src/infra/autoFetchStorage.ts`** (new — sits next to `autoRecordPriceStorage.ts` per the `lib/` → `infra/` migration; if `src/lib/autoRecordPriceStorage.ts` still lives in `lib/` at implementation time, follow the existing convention and use `src/lib/autoFetchStorage.ts` to stay surgical):

- `getAutoFetch(): boolean` reads `localStorage` key `"auto_fetch_prices"` (default `false`).
- `setAutoFetch(enabled: boolean): void` writes it.
- Mirrors `autoRecordPriceStorage.ts` exactly.

**`src/features/settings/useSettings.ts`** (modify):

- Add `autoFetch: boolean` state and `toggleAutoFetch()` — wired to the new storage helpers (MKT-120).

**`src/features/settings/SettingsPage.tsx`** (modify):

- Add a second `<section>` block above the existing auto-record-price section per the spec's UX draft "Sits above the existing transaction-related toggle to group all price-related settings together". New i18n keys: `settings.auto_fetch_label`, `settings.auto_fetch_description`.

**`src/features/settings/useSettings.test.ts`** (modify):

- Add tests mirroring the existing `autoRecordPrice` set — toggling sets storage, initial read reflects storage.

**`src/features/shell/`** (or wherever app-level mount lifecycle lives) — confirm at implementation time:

- New one-shot effect on mount: if `getAutoFetch()` is `true`, call `accountDetailsGateway.fetchAllAssetPrices()` fire-and-forget (MKT-121). Place it next to the existing event-subscription mount logic — likely in `App.tsx` or `MainLayout.tsx`. **DO NOT** place inside `AccountManager` (the page may not be the first route the user lands on; auto-fetch must be session-wide).

**`src/features/accounts/AccountManager.tsx`** (modify):

- Add a "Refresh prices" `Button` to the manager layout header. Wire to a new colocated hook `useRefreshGlobalPrices.ts`:
  - `useRefreshGlobalPrices()` — `isPending` state; `refresh()` calls gateway, narrows the error on `code`, dispatches snackbar: success (`mkt.fetch_dispatched`), `FetchAlreadyRunning` (`mkt.fetch_already_running`), `NoFetchableHoldings` (`mkt.fetch_no_holdings`), `DatabaseError`/`UnknownError` (`error.DatabaseError`). MKT-115 / MKT-133. Button disabled + spinner while `isPending` (MKT-133).
- New file: `src/features/accounts/refresh_prices/useRefreshGlobalPrices.ts` + `.test.ts`. (Sub-feature folder under `features/accounts/` per F1.)

**`src/features/account_details/refresh_prices/`** (new sub-feature folder):

- `useRefreshAccountPrices.ts(accountId)` mirrors the global hook but calls `fetchAccountAssetPrices(accountId)`. Adds `AccountNotFound` narrow → snackbar `error.AccountNotFound` (already an established key — confirm at implementation time, fall back to `error.DatabaseError` if absent).
- `useRefreshAccountPrices.test.ts` mirrors the global test.

**`src/features/account_details/account_details_view/AccountDetailsView.tsx`** (modify):

- Add the refresh button to the page header (next to "Open balance" / "Add transaction"). Wire to `useRefreshAccountPrices(accountId)`.

**`src/features/account_details/shared/presenter.ts`** (modify):

- `toHoldingRow` now formats:
  - **Staleness label** (MKT-140) — computes day delta between `current_price_date` and today's local date. Returns `"—"` when no date, `"Updated today"` when delta is 0, `"Updated Nd ago"` otherwise. Add a pure helper `formatStaleness(currentPriceDate: string | null, today: Date)` for testability.
  - **Source badge** (MKT-142) — formats `current_price_source` (newly added field on `HoldingDetail` if the BE chooses to surface it; confirm during BE implementation whether `current_price_source` is added to the `HoldingDetail` DTO or whether the FE refetches via `getAssetPrices` for the badge). **Default approach**: extend `HoldingDetail` BE-side to include `current_price_source: Option<AssetPriceSource>` so the FE doesn't need a second IPC call per row. This is the lower-friction path — note this as a BE Phase 1 task addition if the spec/contract requires it (re-read MKT-142 to confirm).
- New i18n keys: `mkt.staleness_today`, `mkt.staleness_days_ago`, `mkt.source_manual`, `mkt.source_stooq`.

**`src/features/account_details/price_history/PriceHistoryModal.tsx`** (modify):

- Each row gains a source badge to the right of the date (MKT-141). Reuse the same badge component used for MKT-142 — extract to `account_details/shared/SourceBadge.tsx` if it's the second consumer.

**i18n updates** (`src/i18n/locales/{en,fr}/common.json`):

- New keys: `settings.auto_fetch_label`, `settings.auto_fetch_description`, `mkt.fetch_dispatched`, `mkt.fetch_already_running`, `mkt.fetch_no_holdings`, `mkt.staleness_today`, `mkt.staleness_days_ago`, `mkt.source_manual`, `mkt.source_stooq`, `account.refresh_prices`, `account_details.refresh_prices`.

### E2E

**`e2e/account_details/auto_fetch.test.ts`** (new, location confirmed by test-writer-e2e):

- Setup: seed an account with two priced active holdings + one cash holding; install a fake `PriceProvider` registered via a test-mode toggle in `lib.rs` (confirm pattern with reviewer-infra). Alternative: mount a local HTTP fixture for Stooq via `wiremock` or a Rust test fixture (test-writer-e2e decides).
- Happy path: click "Refresh prices" on AccountDetails → snackbar "Fetching prices…" → wait for `AssetPriceUpdated` → assert badge updates to "Stooq" + staleness "Updated today".
- In-flight rejection: trigger two refreshes back-to-back → second one yields snackbar "Fetch already in progress".
- No-holdings rejection: account with only cash → snackbar "No holdings to fetch".
- System cash skip: verify cash holding row is unchanged after a fetch (MKT-116).

---

## Rules Coverage

| Rule    | Layer    | Task                                                                                                                                                                                                                              | Notes                                                                                                                                                                   |
| ------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| MKT-100 | backend  | `AssetPriceSource` enum in `domain/asset_price.rs`                                                                                                                                                                                | Variants `Manual                                                                                                                                                        | Stooq`; `Finnhub` deferred to KEY |
| MKT-101 | backend  | `AssetService::record_asset_price`, `update_asset_price` hardcode `source = Manual`                                                                                                                                               | Existing-method touch; covered by adjusted existing tests                                                                                                               |
| MKT-102 | backend  | `dispatcher.rs` upsert writes `source = Stooq`                                                                                                                                                                                    | New code                                                                                                                                                                |
| MKT-110 | backend  | `derive_stooq_symbol` in `domain/stooq_symbol.rs`; called by the use cases when building the dispatch scope                                                                                                                       | Pure fn — unit-testable                                                                                                                                                 |
| MKT-111 | backend  | Both use cases reject `NoFetchableHoldings` after pre-deriving symbols and discarding non-derivable entries (so "active AND derivable" matches the spec wording)                                                                  | Composite flat variant                                                                                                                                                  |
| MKT-112 | backend  | `dispatcher.rs` publishes `AssetPriceUpdated` per write                                                                                                                                                                           | Reuses existing event                                                                                                                                                   |
| MKT-113 | backend  | `FetchGuard::try_acquire` returns `None` → composite `FetchAlreadyRunning`                                                                                                                                                        | RAII lease for panic safety                                                                                                                                             |
| MKT-114 | backend  | `dispatcher.rs` per-asset HTTP / parse / upsert try/catch → `tracing::warn!` + continue                                                                                                                                           | Silent skip; no FE surface                                                                                                                                              |
| MKT-115 | frontend | `useRefreshGlobalPrices` / `useRefreshAccountPrices` narrow + snackbar (3 branches: dispatch-success "Fetching prices…", in-flight rejection "Fetch already in progress", no-fetchable-holdings rejection "No holdings to fetch") | `[unit-test-needed]` — one test per branch                                                                                                                              |
| MKT-116 | backend  | Both use cases filter `system-cash-*` prefix from the scope via `core::cash` helper before symbol derivation                                                                                                                      | New code                                                                                                                                                                |
| MKT-120 | frontend | `useSettings.autoFetch` + storage helper                                                                                                                                                                                          | `[unit-test-needed]`                                                                                                                                                    |
| MKT-121 | frontend | Mount-once effect in `App.tsx` / `MainLayout.tsx`                                                                                                                                                                                 | Fire-and-forget; conditional on `getAutoFetch()`                                                                                                                        |
| MKT-122 | backend  | `FetchAllAssetPricesUseCase::run`                                                                                                                                                                                                 | New code                                                                                                                                                                |
| MKT-130 | frontend | "Refresh prices" button on `AccountManager` header                                                                                                                                                                                | New sub-feature `refresh_prices/`                                                                                                                                       |
| MKT-131 | frontend | "Refresh prices" button on `AccountDetailsView` header                                                                                                                                                                            | New sub-feature `refresh_prices/`                                                                                                                                       |
| MKT-132 | backend  | `FetchAccountAssetPricesUseCase::run` rejects via `AccountApplicationError::AccountNotFound` (from `get_by_id` on `Ok(None)`)                                                                                                     | Wrapped in composite                                                                                                                                                    |
| MKT-133 | frontend | `isPending` state in both refresh hooks disables button + shows spinner                                                                                                                                                           | `[unit-test-needed]`                                                                                                                                                    |
| MKT-140 | frontend | `presenter.ts:formatStaleness` + Current Price secondary label                                                                                                                                                                    | `[unit-test-needed]` (pure fn; new file or new export)                                                                                                                  |
| MKT-141 | frontend | `PriceHistoryModal.tsx` row badge                                                                                                                                                                                                 | New component / reuse                                                                                                                                                   |
| MKT-142 | frontend | `AccountDetailsView` Current Price column source badge                                                                                                                                                                            | `[unit-test-needed]` — `presenter.ts:toHoldingRow` derives the badge label from `current_price_source`; requires `current_price_source` on `HoldingDetail` (BE Phase 1) |

**Modified-function coverage** (`[unit-test-needed]` summary for `test-writer-frontend`):

- `useSettings.ts:toggleAutoFetch` (new sibling of `toggleAutoRecordPrice`)
- `presenter.ts:formatStaleness`
- `presenter.ts:toHoldingRow` (now consumes `current_price_source`)
- `useRefreshGlobalPrices.ts:refresh`
- `useRefreshAccountPrices.ts:refresh`

---

## PR Plan

**Strategy**: `3 PRs` (recommended by user; matches the BE → FE → E2E split convention from § PR strategy in `CLAUDE.md`)

**Estimate** (rough — verify after Phase 1 stubs land):

- **BE (Phase 1)**: ~14 files, ~600 LOC churn (migration + 1 enum + 1 trait + 1 pure fn + 1 Reqwest client + 5 use-case files + lib.rs/specta_builder wiring + existing service / repo touches + AssetPrice DTO field add)
- **FE (Phase 2)**: ~12 files, ~450 LOC churn (gateway methods + 2 new sub-feature folders × 2 files + settings toggle + presenter staleness + source badge + i18n bilingual keys + tests)
- **E2E + closure (Phase 3)**: ~5 files, ~200 LOC (one E2E test file + ARCHITECTURE.md / todo.md / spec-check)

The BE estimate exceeds the "≥500 LOC OR ≥20 files" trigger; the FE adds enough visual surface that bundling with BE would explode the diff. Three-PR split is the right call.

### PR 1 — Backend

- **Title**: `feat(asset): auto-fetch backend + source field`
- **Scope**: migration + domain (`AssetPriceSource`, `PriceProvider` trait, `derive_stooq_symbol`) + repository (`source` column R/W, `ReqwestStooqClient`) + asset BC `error.rs` (new flat `AssetError`) + use case folder + `lib.rs` / `specta_builder.rs` wiring + bindings regeneration. Includes the `record_asset_price` / `update_asset_price` `source = Manual` stamp (MKT-101) and a small `account_details` use-case touch to add `current_price_source: Option<AssetPriceSource>` to the `HoldingDetail` DTO (needed by MKT-142 so the FE doesn't need a per-row IPC for the badge — see sanity check #1). The PR therefore touches both the asset BC and the `account_details` use case; the diff is still bounded (~14 files).
- **Workflow checkpoints terminated**: all of Phase 1.
- **Dependency**: none — branches off `main`.
- **Branch suffix**: `feat/mkt-stooq-autofetch-be`.

### PR 2 — Frontend

- **Title**: `feat(asset): auto-fetch frontend UI + settings toggle`
- **Scope**: gateway additions + auto-fetch storage + `useSettings` toggle + Settings UI + global refresh sub-feature + per-account refresh sub-feature + presenter staleness / source badge + price-history source badge + i18n keys + tests + `/visual-proof`.
- **Workflow checkpoints terminated**: all of Phase 2.
- **Dependency**: PR 1 merged. Branch `feat/mkt-stooq-autofetch-fe` rebases off updated `main`.
- **Branch suffix**: `feat/mkt-stooq-autofetch-fe`.

### PR 3 — E2E + closure

- **Title**: `test(asset): e2e auto-fetch + docs closure`
- **Scope**: E2E suite for refresh flows + ARCHITECTURE updates + `docs/todo.md` updates + `spec-checker` run + cross-cutting reviewer fixes.
- **Workflow checkpoints terminated**: all of Phase 3.
- **Dependency**: PR 2 merged. Branch `feat/mkt-stooq-autofetch-e2e` rebases off updated `main`.
- **Branch suffix**: `feat/mkt-stooq-autofetch-e2e`.

---

## Follow-ups / Out-of-scope

- **(asset) — Collapse `AssetApplicationError` + `AssetPriceApplicationError` + `CategoryApplicationError` + their composites into one flat `AssetError`** per the new error-model rule. This amendment introduces `AssetError` _additively_ for the new fetch surface only; the existing CRUD / price-history / category surfaces keep their per-aggregate enums. File via `/techdebt` after PR 1 lands (or piggy-back on the existing "(contracts) — Migrate account-contract.md and update-contract.md to wire-only framing" entry if a single tracking item makes sense — confirm with the user). The same applies to the account BC: `AccountApplicationError` should eventually become `AccountError` per the gold standard.
- **(mkt) — Surface fetch-task completion to FE for end-of-task user feedback** — already in `docs/todo.md`; keep open. Today only dispatch-time feedback exists (MKT-115); the "12 prices updated, 3 skipped" summary is a future spec amendment.
- **(pfd) — Relocate the global "Refresh prices" button from `AccountManager` to the Portfolio Dashboard page header** when PFD ships. The current placement is the closest existing "global" surface; no transition comment about future relocation per the locked decisions.
- **(asset) — OpenFIGI 429 + Finnhub fallback** — both deferred to KEY spec per ADR-008 / ADR-011 and existing `docs/todo.md` entries. The fetch surface is single-provider in v1.
- **(asset) — Stooq symbol derivation edge cases** — ADR-008 anticipates a future `stooq_symbol: Option<String>` opt-out column on `Asset` once real-world derivation failures surface. Out of scope; reactive feature.

---

## Pre-implementation sanity checks

Before starting Phase 1, confirm during implementation:

1. **Source column on `HoldingDetail`** — MKT-142 wants the Current Price column to badge the source. The cleanest path is `HoldingDetail.current_price_source: Option<AssetPriceSource>` populated by the existing `AccountDetailsUseCase` orchestrator alongside the existing `current_price` / `current_price_date` lookup. If this is added in PR 1, it stays a Phase-1-scoped BE change; if deferred to PR 2 the FE has to refetch per asset (rejected — extra IPC for a display label). **Decision**: add to `HoldingDetail` in PR 1; treat as a tiny additive BE field, not a contract change requiring `/contract` rerun (the `AccountDetailsResponse` shape lives in `use_cases/account_details/orchestrator.rs` DTO module, not the asset contract).
2. **`AssetService::seed_cash_asset` and active-holding queries** — confirm the existing `AccountService::get_holdings_for_account` returns enough data to derive "(account_id, asset_id) pairs needing a price" without per-asset round-trips. If not, the use case may need a new `AccountService` helper. The simpler shape is preferred — re-check at implementation time.
3. **Composite shape per `docs/error-model.md`** — re-read § Recipes § Use-case composite carefully before authoring. The example places flat variants (`FetchAlreadyRunning`, `NoFetchableHoldings`, `UnknownError`) DIRECTLY inside the composite as siblings of the `#[from]` wrappers. They are NOT in a nested `*GuardError` enum (that's listed under Anti-patterns). The composite is a flat `#[serde(untagged)]` union with both wrapper and flat arms.
4. **Capability allowlist** — `tauri.conf.json` and the capabilities directory must permit outbound HTTP to `https://stooq.com/`. `reviewer-security` will catch this in Phase 3; surface earlier if obvious.
5. **`reqwest` already in dependencies** — confirmed via `Cargo.toml` (used by `asset_web_lookup` for OpenFIGI). No new crate needed.
