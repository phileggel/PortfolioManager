# TODO

<!-- Add new tech debt and backlog items here. Format: ## (domain) — Short title -->

## (spec) — PFD (Portfolio Dashboard) unblocked, no spec written

`docs/spec-index.md` lists PFD as `planning — paused — blocked on cash-tracking spec`. Cash-tracking shipped on 2026-05-06, so the blocker is lifted, but no `docs/spec/portfolio-dashboard.md` has been written yet. Next step when picked up: run `/spec-writer portfolio-dashboard` to author the cross-account aggregate-view spec (KPIs + per-account list, per the registry description), then the standard `/contract` → `feature-planner` flow. Update `docs/spec-index.md` to drop the "paused — blocked on cash-tracking spec" suffix at the same time.

## (spec) — Triage stale `docs/dashboard.md`

`docs/dashboard.md` is a loose planning doc for an **Account Performance Dashboard** (per-account, time-series, "Performance" tab inside `AccountAssetDetailsView`). It is not in the trigram registry, has no TRIGRAM-NNN numbering, and depends on two specs that do not exist (`docs/operations.md`, `docs/account-currency.md`). It also references stale concepts (`Operation`, separate `AssetAccount.average_price`) that predate the current `Transaction` + `Holding` model. Decide: (a) rewrite against the current domain and promote to a real TRIGRAM spec, or (b) retire the doc and fold the useful ideas into a future ACD extension. Leaving it as-is means it keeps surfacing in spec audits as drift.

## (frontend) — Standardise Zustand store seeding in component tests

Several modal-level vitest test files seed `useAppStore` via the manual-selector wrapper pattern (`vi.mock("@/lib/store", () => ({ useAppStore: vi.fn((selector) => selector({...})) }))`) instead of the `useAppStore.setState({...})` in `beforeEach` pattern prescribed by `docs/test_convention.md` line 70-83. The wrapper bypasses Zustand's actual selector subscription so any derived/memoised selector logic goes untested. Files using the legacy pattern: `AddTransactionModal.test.tsx`, `EditTransactionModal.test.tsx`, `OpenBalanceModal.test.tsx`, `useAddTransaction.test.ts`, `useEditTransactionModal.test.ts`, `useTransactionList.test.ts`, `useBuyTransaction.test.ts`, `useHeaderConfig.test.ts`. Mechanical migration — each file replaces the `vi.mock` block with a `beforeEach` that calls `useAppStore.setState(...)` and removes the per-mock seeding. Surfaced during the cash spec coverage review (CSH-018).

## (backend) — Roll out untagged-composition pattern for boundary error types

Following PR #5 review, `RecordDepositCommandError` and `RecordWithdrawalCommandError` now compose `AccountOperationError | TransactionDomainError | CashCommandBoundaryError` via `#[serde(untagged)]` instead of redefining variants. The same pattern should be rolled out to the older boundary types — `TransactionCommandError`, `OpenHoldingCommandError`, `AccountCommandError`, `AssetCommandError`, `AccountDetailsCommandError`, `ArchiveAssetCommandError`, `DeleteAssetCommandError`, `CategoryCommandError`, `AssetPriceCommandError`, `UpdateAssetPriceCommandError`, `DeleteAssetPriceCommandError`, `WebLookupCommandError`, `AccountDeletionCommandError` — so the entire boundary layer stops duplicating domain error variants. Each conversion is mechanical (compose existing domain enums + a per-command boundary-only enum for `Unknown` / `*NotFound`). Out of scope for the cash PR to keep its diff focused.

## (backend) — Convert service layer methods to typed Result returns

Service layer (`AccountService`, `AssetService`) currently returns `anyhow::Result<T>`. Domain errors get wrapped and downcast at the API boundary. The api.rs mappers downcast each domain error type explicitly, which works but loses the type-system guarantee that "this method can only return errors X, Y, Z". Convert each public service method to a typed `Result<T, ConcreteError>` where `ConcreteError` is either a single domain error enum or a small composition. Spawning point: cash methods (`record_deposit`, `record_withdrawal`) — they're new, contained, and only emit `AccountOperationError` / `TransactionDomainError` / `AccountDomainError`. Surfaced during PR #5 review (2026-05-06).

## (backend) — Consolidate transaction-recording command contracts

After the `use_cases/holding_transaction/` consolidation, two pre-existing contract inconsistencies remain in the moved commands. Surfaced during the cash-tracking spec review (2026-05-05); deliberately deferred from the consolidation refactor to keep that PR scope-minimal.

**1. Per-command error enums.** `buy_holding`, `sell_holding`, `correct_transaction`, `cancel_transaction` share one permissive `TransactionCommandError` enum whose variants are a union of every error any of them might emit. `account-contract.md` already documents per-command subsets (e.g. `cancel_transaction` only emits `TransactionNotFound`/`DbError`) but the type system can't enforce that. Split into `BuyHoldingCommandError`, `SellHoldingCommandError`, `CorrectTransactionCommandError`, `CancelTransactionCommandError` matching what the contract already claims. `open_holding` already follows this pattern. Frontend impact: error-handler types in gateway/forms regenerate.

**2. Parameter style.** `correct_transaction(id: String, account_id: String, dto: CorrectTransactionDTO)` and `cancel_transaction(id: String, account_id: String)` mix primitives + DTO; the rest are DTO-only. Move `id`/`account_id` into the DTOs for consistency. Frontend impact: gateway call sites change.

Work order: do (1) first; (2) is optional and lower value. (1) becomes notably more relevant once cash lands, because `InsufficientCash { current_balance_micros, currency }` would otherwise pollute the shared enum and bleed into `cancel_transaction`'s TS type even though it only fires from a replay-violation edge case.

## (backend) — Promote BC application services to traits, mock with mockall

`AccountService` and `AssetService` are concrete structs, so cross-BC orchestrators (`HoldingTransactionUseCase`, `ArchiveAssetUseCase`, `DeleteAssetUseCase`, `AccountDetailsUseCase`, …) cannot mockall-mock them and instead test against real services + in-memory SQLite. That's against the spirit of `docs/backend-rules.md` B34 ("Tests for services and orchestrators SHOULD mock external dependencies using mockall-generated mocks") — repositories already follow B34 via `#[cfg_attr(test, mockall::automock)]` on each domain.rs trait, but the service layer above them does not.

Extract a trait per service (e.g. `AccountServiceContract`, `AssetServiceContract`) listing the methods orchestrators call, annotate with `#[cfg_attr(test, mockall::automock)]`, and have orchestrators inject `Arc<dyn AccountServiceContract>` / `Arc<dyn AssetServiceContract>`. Then rewrite the orchestrator inline tests to use the generated `MockAccountService` / `MockAssetService` instead of `setup_pool` + real repositories — true unit isolation, faster, no DB dependency. Surfaced during PR #4 review (2026-05-06).

## (backend) — Carry diagnostic hint in OpenHoldingCommandError::Unknown

`use_cases/holding_transaction/api.rs:90-96, 102-107` map "impossible" `TransactionDomainError` / `AccountDomainError` variants to `Unknown` after a `tracing::error!`. The user sees an opaque message with no correlation. Consider `Unknown { hint: String }` or returning a debug-only correlation id so support reports can be triaged. Pre-existing behaviour, not introduced by the refactor — surfaced during the inline review of `120e5ba`.

## (backend) — Introduce dependency injection container for service wiring

`lib.rs` manually constructs and wires all repositories, services, and use cases in a single `block_on` closure. As the number of bounded contexts grows this becomes hard to maintain. Introduce a lightweight DI approach (e.g. a dedicated `AppContainer` struct or a builder pattern) to decouple service construction from app bootstrap, make the dependency graph explicit, and simplify testing of the wiring itself.

## (deps) — Upgrade reqwest to 0.13

`reqwest 0.12.28` is a major version behind (`0.13.3` available). Breaking changes: TLS default switches from native-tls to rustls+aws-lc; `query()`/`form()` are now optional features; several deprecated methods removed. Current feature flags (`rustls-tls-native-roots`, `json`) need review against the new defaults before upgrading. See `docs/dep-audit-2026-05-05.md`.

## (deps) — serialize-javascript CVE in @wdio/mocha-framework (GHSA-5c6j-r48x-rmvq, CVE-2026-34043)

`@wdio/mocha-framework@9.27.1` depends on `mocha` which pins `serialize-javascript <=7.0.4`. Two high-severity CVEs: RCE via RegExp.flags (GHSA-5c6j-r48x-rmvq) and CPU exhaustion DoS (CVE-2026-34043, fixed in 7.0.5). devDependency only — E2E test runner, not in the production bundle. Upstream fix tracked in [mocha#5872](https://github.com/mochajs/mocha/issues/5872). Do NOT run `npm audit fix --force` (downgrades @wdio to v6, breaking). Re-evaluate when mocha releases with serialize-javascript 7.0.5+.

## (deps) — Update specta to rc.23

`tauri-specta rc.21` pins `specta = "=2.0.0-rc.22"` (exact version). Wait for `tauri-specta rc.22+` before upgrading to `specta rc.23` + `specta-typescript 0.0.10`.
Status (2026-04-27): `specta rc.23` available, `tauri-specta` still blocked at `rc.21`.

## (ci) — Standardise ubuntu runner version across all workflows

`coverage.yml` uses `ubuntu-latest` (currently resolves to 24.04 on GitHub-hosted runners) while `e2e.yml` and `release-manual.yml` (linux path) use `ubuntu-22.04`. If webkit2gtk or other apt packages behave differently between versions, coverage and e2e jobs may diverge silently. Pin all jobs to `ubuntu-22.04` until 24.04 compatibility is validated, then move all to 24.04 together.

## (ci) — Remove npm ci from security-audit.yml npm-audit job

`npm audit` (npm 7+) reads `package-lock.json` directly and does not require a full install. Removing the `npm ci` step and `cache: npm` from the npm-audit job in `security-audit.yml` cuts ~30 s of network I/O from every weekly run.

## (scripts) — Fix #!/bin/bash shebang in scripts/build.sh

`scripts/build.sh` uses `#!/bin/bash` instead of `#!/usr/bin/env bash`. This will fail on systems where bash is not at `/bin/bash` (NixOS, some BSDs). Change to `#!/usr/bin/env bash` to match all other scripts in the project.

## (deps) — Accepted risk: RUSTSEC-2023-0071 (rsa Marvin Attack)

`cargo audit` flags `rsa 0.9.10` (timing sidechannel, CVSS 5.9 medium) with no upstream fix. Pulled transitively via `sqlx-mysql 0.8.6` because the `sqlx` macro crate compiles all backends regardless of enabled features. We only enable `sqlite`, so the vulnerable RSA path is never reached at runtime. Re-evaluate when sqlx ships a fix or when we change DB backend.
