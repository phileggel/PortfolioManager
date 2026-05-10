# TODO

<!-- Add new tech debt and backlog items here. Format: ## (domain) — Short title -->

## (asset) — Promote ISIN to canonical identifier alongside ticker

`Asset.reference` is currently a single field that ends up holding either an ISIN or a ticker depending on how the asset was created (ISIN search → ISIN; keyword search → ticker; manual → whatever the user typed). This makes the AST uniqueness check semantic noise — the same instrument can be created twice as `AI` and `FR0000120073` and the two records won't dedup.

Industry convention: ISIN is the canonical identity (stable across rebrands, globally unique by ISO 6166), ticker is a venue-specific display label that can change (e.g. `TOT → TTE` for Total → TotalEnergies in 2021).

**Proposed shape (additive, no breaking migration):**

- New nullable column `isin: Option<String>` on `Asset`
- Existing `reference` field becomes the human-friendly ticker (rename to `ticker` if breaking is acceptable; otherwise leave as-is and treat the field as ticker)
- Uniqueness check switches to ISIN-when-present, ticker-when-not
- Add Asset form: ticker required, ISIN optional
- Web lookup ISIN-path → both filled; keyword-path → ticker only (OpenFIGI's free `/v3/mapping` response doesn't expose ISIN, so we cannot recover it for keyword-discovered assets)
- Manual creation: ticker required, ISIN optional with a "lookup ISIN" affordance for the user

**Why it's not done now:** the just-shipped WEB-050 fix already surfaces the right primary listing for free-text searches; the user pain that motivated this discussion is resolved. Adding a second identifier field is a 1–2 day Workflow-A feature (migration, domain entity, validation, AST spec edit, gateway, presenter, form, tests) and only pays off once a downstream feature actually consumes ISIN.

**Spawning point:** wire it in as part of the first downstream ISIN consumer (dividend tracking, broker import/export, corporate-action handling). At that point the cost is amortized into the feature that needs it. Surfaced during the WEB-050 review (2026-05-08).

## (spec) — PFD (Portfolio Dashboard) unblocked, no spec written

`docs/spec-index.md` lists PFD as `planning — paused — blocked on cash-tracking spec`. Cash-tracking shipped on 2026-05-06, so the blocker is lifted, but no `docs/spec/portfolio-dashboard.md` has been written yet. Next step when picked up: run `/spec-writer portfolio-dashboard` to author the cross-account aggregate-view spec (KPIs + per-account list, per the registry description), then the standard `/contract` → `feature-planner` flow. Update `docs/spec-index.md` to drop the "paused — blocked on cash-tracking spec" suffix at the same time.

## (backend) — Error-model refactor (multi-PR)

Tracked in `docs/plan/error-model-refactor.md`. Migrates services from `anyhow::Result` to typed Result with composed error enums per `docs/ddd-reference.md` § Errors. Supersedes the previous "untagged-composition rollout" and "convert services to typed Result" TODOs.

Status (2026-05-10): PRs 1–5 shipped — asset/category state-checks (PR 1), cash typed Result + shared `InfrastructureError` (PR 2), holding-transaction unification (PR 3), open-holding typed (PR 4), Account CRUD typed (PR 5). The plan doc was tightened in PR 5 with the project-specific **infra translation rule** (per-BC `*ApplicationError::DatabaseError`; shared `InfrastructureError` does NOT appear on the FE wire) — PR 6+ enforces it. 7 families remaining (Account details, Category CRUD, Asset CRUD, Asset price, Archive/Delete asset, Account deletion, Web lookup) — see plan doc § Failure-surface-family map.

## (backend) — `correct_transaction` / `cancel_transaction` parameter style

`correct_transaction(id: String, account_id: String, dto: CorrectTransactionDTO)` and `cancel_transaction(id: String, account_id: String)` mix primitives + DTO; the rest of the holding-transaction commands are DTO-only. Move `id`/`account_id` into the DTOs for consistency. Frontend impact: gateway call sites change. Surfaced during cash-tracking spec review (2026-05-05); per-command-error-enums concern from the original entry is subsumed by `docs/plan/error-model-refactor.md` PR 3.

## (backend) — Promote BC application services to traits, mock with mockall

`AccountService` and `AssetService` are concrete structs, so cross-BC orchestrators (`HoldingTransactionUseCase`, `ArchiveAssetUseCase`, `DeleteAssetUseCase`, `AccountDetailsUseCase`, …) cannot mockall-mock them and instead test against real services + in-memory SQLite. That's against the spirit of `docs/backend-rules.md` B34 ("Tests for services and orchestrators SHOULD mock external dependencies using mockall-generated mocks") — repositories already follow B34 via `#[cfg_attr(test, mockall::automock)]` on each domain.rs trait, but the service layer above them does not.

Extract a trait per service (e.g. `AccountServiceContract`, `AssetServiceContract`) listing the methods orchestrators call, annotate with `#[cfg_attr(test, mockall::automock)]`, and have orchestrators inject `Arc<dyn AccountServiceContract>` / `Arc<dyn AssetServiceContract>`. Then rewrite the orchestrator inline tests to use the generated `MockAccountService` / `MockAssetService` instead of `setup_pool` + real repositories — true unit isolation, faster, no DB dependency. Surfaced during PR #4 review (2026-05-06).

## (backend) — Introduce dependency injection container for service wiring

`lib.rs` manually constructs and wires all repositories, services, and use cases in a single `block_on` closure. As the number of bounded contexts grows this becomes hard to maintain. Introduce a lightweight DI approach (e.g. a dedicated `AppContainer` struct or a builder pattern) to decouple service construction from app bootstrap, make the dependency graph explicit, and simplify testing of the wiring itself.

## (deps) — Upgrade reqwest to 0.13

`reqwest 0.12.28` is a major version behind (`0.13.3` available). Breaking changes: TLS default switches from native-tls to rustls+aws-lc; `query()`/`form()` are now optional features; several deprecated methods removed. Current feature flags (`rustls-tls-native-roots`, `json`) need review against the new defaults before upgrading. See `docs/dep-audit-2026-05-05.md`.

## (deps) — serialize-javascript CVE in @wdio/mocha-framework (GHSA-5c6j-r48x-rmvq, CVE-2026-34043)

`@wdio/mocha-framework@9.27.1` depends on `mocha` which pins `serialize-javascript <=7.0.4`. Two high-severity CVEs: RCE via RegExp.flags (GHSA-5c6j-r48x-rmvq) and CPU exhaustion DoS (CVE-2026-34043, fixed in 7.0.5). devDependency only — E2E test runner, not in the production bundle. Upstream fix tracked in [mocha#5872](https://github.com/mochajs/mocha/issues/5872). Do NOT run `npm audit fix --force` (downgrades @wdio to v6, breaking). Re-evaluate when mocha releases with serialize-javascript 7.0.5+.

## (deps) — Update specta to rc.23

`tauri-specta rc.21` pins `specta = "=2.0.0-rc.22"` (exact version). Wait for `tauri-specta rc.22+` before upgrading to `specta rc.23` + `specta-typescript 0.0.10`.
Status (2026-04-27): `specta rc.23` available, `tauri-specta` still blocked at `rc.21`.

## (deps) — Accepted risk: RUSTSEC-2023-0071 (rsa Marvin Attack)

`cargo audit` flags `rsa 0.9.10` (timing sidechannel, CVSS 5.9 medium) with no upstream fix. Pulled transitively via `sqlx-mysql 0.8.6` because the `sqlx` macro crate compiles all backends regardless of enabled features. We only enable `sqlite`, so the vulnerable RSA path is never reached at runtime. Re-evaluate when sqlx ships a fix or when we change DB backend.
