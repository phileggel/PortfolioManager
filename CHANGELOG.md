# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.0] - 2026-05-08

### Added

- cash + priced
  same-currency holdings, no FX.
- `OpeningBalance` rejected on the Cash Asset (CSH-061).
- Migration `202605060001` is documentation-only —
  `transaction_type` is TEXT.

Frontend

- New Deposit / Withdrawal modals + hooks mirroring Buy/Sell, with a
  shared `validateCashForm` helper (CSH-021/031). Withdraw + Buy/Edit
  surface `InsufficientCash` inline with localised balance + currency
  (CSH-081); `useTransactions` formats the payload-bearing variant
  centrally.
- Account Details: Deposit (always) and Withdraw (gated on cash > 0)
  buttons in the header, Global Value tile, and a no-cash banner.
- Cash row variant in the active holdings table — sorted to the top
  (CSH-092), no cost-basis / avg-price / realized-pnl cells, with
  inline Deposit / Withdraw actions (CSH-091).
- Suppress system Cash Assets in the Asset Manager and the Add / Edit
  / Open-Balance asset selectors (CSH-015/018/061), and the Cash
  Category from category lookups (CSH-017).
- Transaction list renders Deposit / Withdrawal type labels (CSH-101).
- New `useAccountDetailsView(accountId)` hook absorbs modal state +
  handlers + derived flags so `AccountDetailsView.tsx` is pure JSX +
  one hook call.
- New cash, validation, and `error.*` i18n keys in EN + FR.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

### Fixed

- stop shipping generate_bindings dev tool in installer
  The dev-only generate_bindings bin was packaged into Windows NSIS
  installers since at least v0.8.x. Gate it behind a Cargo feature so
  cargo build skips it; recipes and scripts opt in via
  --features generate-bindings. Release workflows also strip any stale
  binary restored from rust-cache before tauri-action runs.
- reject mutations on system Cash Asset (CSH-016)
  update_asset / archive_asset / unarchive_asset / delete_asset now reject
  inputs whose target asset has class == Cash with CashAssetNotEditable.
  archive_asset and delete_asset additionally surface NotFound for unknown
  ids (the new guard loads the asset). FE filtering (CSH-015/018) already
  prevents these calls in practice; this closes the direct-IPC gap.
- the scrim covers the whole viewport at 50%
  black + blur, and in a standalone preview there is no real content
  behind it, so dark-mode shots looked near-black and misrepresented
  the component. The panel-only pattern lets dark mode show the proper
  m3-surface-container tone behind the dialog.
- Documented the panel-only pattern in CLAUDE.md (project-specific
  visual-proof note).
- Kit-level update tracked separately on phileggel/claude-kit.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

- wire afterTest screenshot hook in wdio.conf.ts
  Captures a PNG to screenshots/e2e-failures/ on every failed test for
  post-mortem diagnosis. Filename: {suite}-{test}-{ISO-timestamp}.png.
  Also fixes pre-existing bug where SIGINT handler set undeclared `exit`
  instead of `cleanShutdown`, causing spurious "exited unexpectedly" logs.
- make E2E suite green for WebKit/HeadlessUI
  Replace flaky `*=` selectors with XPath, redesign tests around the
  HeadlessUI ComboboxField boundary (no automation in WebKit), and add
  data-testid + aria-label to modal close buttons. New ADR-007 documents
  the combobox limitation. All 18 E2E tests pass.
- remove spin buttons from numeric transaction inputs
- replace hardcoded placeholders in remaining modals
- replace hardcoded placeholders in buy and sell modals
- replace string-sentinel with typed AccountNotFound error

## [0.10.0] - 2026-05-03

### Added

- improve web lookup with Derivatives class and exchange
  Add Derivatives AssetClass (default_risk=5); maps Warrant/Option/Future/Rights (WEB-023)
  Add exchange field to AssetLookupResult; resolved from exchCode via static table (WEB-049)
  Sort results by AssetClass priority before 10-item truncation (WEB-048)
  Two-line result row in SearchPanel: code+name / type·exchange (WEB-031)
  Add formatAssetClass() presenter for human-readable class labels

### Fixed

- address PR review comments on exchange_code and i18n

## [0.9.1] - 2026-05-03

### Fixed

- use constant default for created_at ALTER TABLE
  SQLite rejects non-constant expressions (e.g. datetime('now')) in
  ALTER TABLE ADD COLUMN. This caused all app launches to fail with
  "Database initialization failed" since 2026-04-26.

Existing rows get the epoch sentinel '1970-01-01T00:00:00Z', which
sorts before any real timestamp and is correct for ORDER BY date, created_at.

## [0.9.0] - 2026-05-03

### Added

- isolate E2E tests with an ephemeral SQLite database
- implement frontend for TRX-042–058
  OpenBalanceModal + useOpenBalance hook; openHolding in gateway;
  Open Balance button in AccountDetailsView (TRX-055);
  OpeningBalance edit support in EditTransactionModal (TRX-051);
  i18n en+fr; TRX-046 future-date guard; presenter + edit modal
  unit tests (TRX-051–054)
- implement opening-balance transaction type

### Fixed

- change default tracing log level from debug to info
- harden wdio.conf.ts and align buy_sell note field
- Append Date.now() suffix to E2E_DATA_DIR to prevent collision on concurrent runs
- Wrap mkdirSync in try/catch with actionable error message
- Rename `exit` variable to `cleanShutdown` to avoid shadowing process.exit
- Change note: null to note: "" in seedBuy for Tauri 2 null-deserialization safety
- apply reviewer findings across frontend and backend
- apply reviewer findings and update docs
  Reviewer findings: Math.floor (TRX-047), TRX-055 button always visible,
  TRX-046 edit date guard, TRX-058 snackbar test, mount log removed,
  gateway.test.ts moved to feature root, i18n placeholder keys.
  Docs: plan ticked, roadmap row, todo tech-debt items, UL confirmed.
- use platform native root CAs for OpenFIGI HTTPS on Windows
- resolve error.undefined on account creation and window visibility

## [0.8.1] - 2026-05-01

### Fixed

- show empty state before error in account/asset/category tables

## [0.8.0] - 2026-05-01

### Added

- add web lookup dialog before Add Asset form
  Introduces WebLookupModal, SearchPanel, useWebLookupSearch, useWebLookupModal hooks.
  Extends useAddAsset with prefill?: AssetLookupResult (WEB-041/042).
  Wires AssetManager to open WebLookupModal instead of AddAssetModal.
  Adds i18n keys (en + fr) for the web_lookup block.
- add search_asset_web command and OpenFIGI client
- add price history CRUD commands and modal
- add pre-deletion summary dialog for non-empty accounts
  ACC-019: show holding+tx counts before deleting a non-empty account
  ACC-020: new get_account_deletion_summary Tauri command
  Fix ConfirmationDialog to not auto-close after async onConfirm (R13)
  Use tokio::try_join! for parallel count queries in service
- add UoW infrastructure foundation (Phase 5)
- add auto-record price checkbox and settings toggle
  Settings page gains a global auto-record toggle persisted in
  localStorage. Buy/sell/edit forms gain a RecordPriceCheckbox
  whose default snapshots the global toggle on create (always OFF
  on edit, MKT-052). 18 new tests; 6 new i18n keys (en + fr).
- auto-record asset price from transaction
  CreateTransactionDTO gains record_price: bool. RecordTransactionUseCase
  wires Arc<AssetService>; orchestrator upserts AssetPrice in the same
  DB tx and notifies after commit (MKT-055..062). 9 new tests.
  Frontend hooks default to record_price=false (UI wiring lands next).
- add Settings page with language override
- auto-detect system language and locale-aware number format
- add locale-aware number formatting for display values
- add market price entry and unrealized P&L display
- persist last visited section across sessions
- add closed position history
- guard delete against existing transactions
  DeleteAssetUseCase blocks hard-delete when any transaction references
  the asset. Mirrors ArchiveAssetUseCase pattern. SQLx cache updated.
- add buy-from-holding-row modal
  Buy (+) on holding row opens BuyTransactionModal instead of navigating to /transactions/new.
  Mirrors SellTransactionModal pattern (TRX-041). Modals moved to account_details/ (use-case boundary, fixes F22).
  IconButton gains success/error tonal variants. HoldingRow extracted. try/finally, useMemo, useCallback fixes.
- add currency field to Account entity
  Migration, domain, repository, service, API, bindings, account form,
  transaction modals. Exchange rate field now compares asset.currency vs
  account.currency (TRX-021, SEL-036).
- add archive eligibility guard (OQ-6)
  HoldingRepository.has_active_holdings_for_asset checks quantity > 0 across all accounts.
  ArchiveAssetUseCase guards then delegates to AssetService, keeping contexts isolated.
- implement sell transaction frontend with P&L display
- implement sell transaction backend with realized P&L
- improve account list page UX
- move back button and title into shell header

### Fixed

- abort E2E run when tauri build fails in onPrepare
- address reviewer-infra findings on E2E infrastructure
- guard isSubmitting reset in finally blocks
- reject archived-asset sell on update
- clear DateField parent state on invalid typed input
- replace hardcoded strings with i18n keys
- fix SEL-011 account field and SEL-026 average price retention
  SEL-011: add read-only Account field to SellTransactionModal
  SEL-026: preserve average_price (last known VWAP) when holding quantity reaches zero, per TRX-040

## [0.7.0] - 2026-04-26

### Added

- add Settings page with language override
- auto-detect system language and locale-aware number format
- add locale-aware number formatting for display values
- add market price entry and unrealized P&L display
- persist last visited section across sessions
- add closed position history
- guard delete against existing transactions
  DeleteAssetUseCase blocks hard-delete when any transaction references
  the asset. Mirrors ArchiveAssetUseCase pattern. SQLx cache updated.
- add buy-from-holding-row modal
  Buy (+) on holding row opens BuyTransactionModal instead of navigating to /transactions/new.
  Mirrors SellTransactionModal pattern (TRX-041). Modals moved to account_details/ (use-case boundary, fixes F22).
  IconButton gains success/error tonal variants. HoldingRow extracted. try/finally, useMemo, useCallback fixes.
- add currency field to Account entity
  Migration, domain, repository, service, API, bindings, account form,
  transaction modals. Exchange rate field now compares asset.currency vs
  account.currency (TRX-021, SEL-036).
- add archive eligibility guard (OQ-6)
  HoldingRepository.has_active_holdings_for_asset checks quantity > 0 across all accounts.
  ArchiveAssetUseCase guards then delegates to AssetService, keeping contexts isolated.
- implement sell transaction frontend with P&L display
- implement sell transaction backend with realized P&L
- improve account list page UX
- move back button and title into shell header

### Fixed

- reject archived-asset sell on update
- clear DateField parent state on invalid typed input
- replace hardcoded strings with i18n keys
- fix SEL-011 account field and SEL-026 average price retention
  SEL-011: add read-only Account field to SellTransactionModal
  SEL-026: preserve average_price (last known VWAP) when holding quantity reaches zero, per TRX-040

## [0.6.0] - 2026-04-19

### Added

- add /transactions/new page as DDD entry point
- add transaction list page
- add get_asset_ids_for_account command
- add toast notification infrastructure
- reorder nav, translate labels, add About modal
- Reorder drawer: Accounts, Assets, Categories
- Translate nav labels via i18n (nav.\* keys)
- Replace /about route with AboutModal triggered from sidebar
- Add app description and license to About modal
- add TanStack Router with hash history
  ACD-011: replace useState-based nav with URL routing.
  Routes: /assets, /accounts, /accounts/$accountId, /categories, /about.
  Back button and direct linking now work via hash history.
- implement account details view with cost basis
  ACD-010 to ACD-041: holdings list, cost basis, loading/empty/error states
- add purchase transactions with VWAP and server-side total
  Backend: CreateTransactionDTO omits total_amount (TRX-026); orchestrator
  computes it via compute_total. Frontend: computeTotalMicro for display
  preview only. VWAP recalculation on create/update/delete (TRX-030).
  Atomic DB transaction for transaction + holding upsert (TRX-027).

### Fixed

- correct VWAP cost basis calculation
- Fix double MICRO division in VWAP numerator
- Add fees to cost basis via total_amount (TRX-030)
- Align VWAP formula with TRX-026 total_amount
- Update spec TRX-030 to reflect correct formula
- fix asset prefill on buy modal and action.select i18n key
  AddTransactionModal was always mounted so useState init ran once with
  undefined prefillAssetId; fix via key prop on modal (TRX-011).
  fees default changed from "0" to "" to match quantity placeholder.
  action.select i18n key added to fr and en locales.

## [0.5.0] - 2026-04-05

### Added

- remove unused footer and settings

## [0.4.1] - 2026-04-05

### Fixed

- updater issues

## [0.4.0] - 2026-04-05

### Added

- implement account CRUD with full backend and frontend

### Fixed

- stage Cargo.lock in release commit

## [0.3.0] - 2026-04-04

### Added

- add auto-update feature with banner and about page
  Backend: tauri-plugin-updater, use_cases/update_checker (check/download/install
  commands). R18 fix: emit db:migration_error event instead of panicking so the
  frontend error screen is reachable. UpdateState managed before DB init.
  Frontend: update banner state machine (idle→available→downloading→ready/error),
  about page manual check (R25-R27), shell gateway, migration loading/error screens.

## [0.2.0] - 2026-03-29

### Added

- add archive/unarchive, reference validation and UX improvements
- Implement R1-R20: mandatory reference, archive/unarchive, duplicate warning,
  load-error state with retry, isSubmitting guard, showArchived toggle
- Rename factory methods: update_from->with_id, from_storage->restore (B1)
- Store fetches all assets (active+archived); AssetTable filters by showArchived
- Add tests: validateAsset (R9), presenter (R11); fix no-op R9 test in useAddAsset

## [0.1.0] - 2026-03-29

### Added

- Initial release — portfolio management desktop app (Tauri 2 + React 19 + Rust)
- Asset CRUD with categories, currency, risk level, and asset class
- Category management with system-protected default category
- Dashboard scaffold
- i18n support (fr / en)
