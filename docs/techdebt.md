# Tech Debt

Observations of code smells, brittle patterns, or pre-existing issues surfaced
during work that don't warrant immediate action. Format produced by the kit's
`/techdebt` skill — see `.claude/kit-tools.md`.

Entries are observations, not commitments. Triaged by `/whats-next` alongside
`docs/todo.md`. Remove an entry once it has been resolved or moved to
`docs/todo.md` for active work.

---

## 2026-05-10 — Migrate to FE gold layout (per kit proposals #21–#23)

- Found by: manual (post-FE-architecture delta scan)
- Where: src/ (top-level structure + features/account_details cross-imports)
- Context: branch `main` @ `114cb79`
- Severity: 🟡
- Observation: Three FE layout/coupling deltas surfaced by mirroring the BE architecture revisit on the frontend. The current shape works but encodes implicit conventions that diverge from the proposed kit gold layout (kit issues phileggel/claude-kit#21, #22, #23). Defer the migration until the kit ratifies the proposals; track here in the meantime.
  1. **`src/lib/update/` is a feature, mislocated.** It has full feature shape — `gateway.ts` + sub-feature folder (`update_banner/`) + hook + test — but lives under `src/lib/`. Per kit proposal #23, `lib/` (renamed `infra/`) hosts platform adapters only; features must live in `src/features/`. Move to `src/features/update/`. Mechanical rename + import-path update.

  2. **`features/account_details/{buy,sell}_transaction/` cross-imports from `features/transactions/`.** Today the imports are `RecordPriceCheckbox` (component), `TransactionFormData` (type), `validateTransactionForm` / `validateSellForm` (pure functions), and `useTransactions` (hook with state). Per the F23 reframing in kit proposal #21, the first three (primitives) become fine; the fourth (behavior coupling via a hook) remains a code smell. Either `account_details` owns its own thin wrapper around the gateway calls it needs, or the two features consolidate. Worth deciding _with_ the consolidation question (delta #3) rather than fixing the hook coupling alone.

  3. **`account_details` sub-feature bloat (8 sub-features).** Half of them — `buy_transaction`, `sell_transaction`, `deposit_transaction`, `withdrawal_transaction` — are conceptually transaction-recording flows and overlap with the `transactions/` feature. Two reasonable shapes: (a) consolidate the four into `transactions/` and let `account_details` stay focused on the holdings view, or (b) formalize the split — `account_details` owns "modals invoked from the holding row," `transactions/` owns "the transaction list page and its CRUD." Pick (b) as the lighter move; (a) is a bigger refactor.

  Migration is mechanical for #1 (folder move + ~5 import sites) and conventional for #2/#3 (depends on the consolidation decision). Cleanest as one or two dedicated PRs after the kit proposals land (so the project mirrors the kit-ratified spec).

## 2026-05-09 — Migrate to gold DDD layout (per kit proposals #17–#19)

- Found by: manual (post-PR-#12 design discussion)
- Where: src-tauri/src/ (top-level structure)
- Context: branch `main` @ `eb4e180`
- Severity: 🟡
- Observation: Three layout deltas from the agreed gold target (kit issues phileggel/claude-kit#17, #18, #19; mirrored in ADR-008 once authored). The current shape works but documents the architecture imperfectly to newcomers.
  1. **`service.rs` lives at the BC root, not in `application/`.** Inconsistent with `domain/` and `repository/` (which ARE folders). After PR 2b introduced `application/error.rs` per BC, the application layer has half its content in a folder, half at root. Migrate `service.rs` → `application/service.rs` per BC.

  2. **`repository/` should be `infrastructure/`** (DDD layer name). `repository/` is one TYPE of infrastructure; renaming protects against the day a BC adds an external API client, cache adapter, or message-queue subscriber (avoids proliferating peer folders). Today the folder only contains repository impls — stay flat (`infrastructure/{aggregate}.rs`) until non-repo infra arrives, then add siblings without nesting.

  3. **`core/` should be `shared/`**, restructured into the three DDD layer folders. `core/` overpromises ("central business logic" — but BCs ARE the business). Target shape:

     ```
     shared/
     ├── application/error.rs        ← shared InfrastructureError
     ├── domain/cash.rs              ← shared kernel (system_cash_asset_id)
     └── infrastructure/{db, event_bus, logger, specta_*, uow}
     ```

     `InfrastructureError` reclassifies as application-layer (it's the typed application translation of opaque infra failures, per the DDD doc's travel rule — the NAME describes the source, the LAYER is application).

  Migration is mechanical (folder moves + module-path updates, ~50–100 import sites total). Cleanest as a single dedicated chore PR after the kit proposals land (so the project mirrors the kit-ratified spec). Track in `docs/plan/error-model-refactor.md` § Out of scope (already lists "Folder reshape" as deferred — this entry expands the scope to all three deltas).

- Found by: reviewer-arch
- Where: src-tauri/src/context/asset/domain/asset.rs (Asset::is_cash + CSH-016 guards)
- Context: branch `refactor/move-asset-category-state-checks` @ `5d5ae8f`
- Severity: 🔴
- Observation: The CSH-016 invariant intends to protect the seeded system Cash Asset, but the guard predicate is `self.class == AssetClass::Cash` rather than a system-asset marker. As a result, any user-created cash-class asset is also silently blocked from edits, archiving, and deletion by the same code path. The behavior is pre-existing — PR `refactor/move-asset-category-state-checks` preserves it verbatim from the original `guard_not_cash`. Whether a user can actually create a cash-class asset (i.e. whether the issue is reachable) is unverified against the spec.

## 2026-05-08 — apply_deposit / apply_withdrawal leave rejected tx in self.transactions if replay_cash_holding fails

- Found by: reviewer-arch
- Where: src-tauri/src/context/account/domain/account.rs (Account::apply_deposit, Account::apply_withdrawal)
- Context: branch `refactor/cash-tx-aggregate-split` @ `2c2ea3e`
- Severity: 🟡
- Observation: Both `apply_deposit` and `apply_withdrawal` push the new transaction into `self.transactions` and then call `self.replay_cash_holding()?`. If the replay raises `InsufficientCash` (possible for a back-dated transaction that interleaves with prior cash-affecting txns), the method returns `Err` but the rejected tx is now in the in-memory aggregate. The pattern is pre-existing — `record_deposit`/`record_withdrawal` had the identical structure before PR `refactor/cash-tx-aggregate-split`. The corruption is in-memory only (the service drops the aggregate on `Err` without saving), so no persisted state is affected. A defensive `pop()` on replay failure would close the gap. The same pattern likely also applies to `buy_holding` / `sell_holding` and other history-mutating methods.

## 2026-05-08 — TRX-020 hardcoded date validation falls back to NaiveDate::MIN silently

- Found by: reviewer-backend
- Where: src-tauri/src/context/account/domain/transaction.rs (Transaction::validate, line ~263)
- Context: branch `refactor/cash-tx-aggregate-split` @ `2c2ea3e`
- Severity: 🔵
- Observation: `NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(chrono::NaiveDate::MIN)` falls back to `NaiveDate::MIN` (-262144 AD) if the literal ever stops parsing — a silent loss of the TRX-020 lower-bound semantic. `from_ymd_opt(1900, 1, 1)` is provably non-`None`, so `expect("hardcoded valid date")` is the safer form: it would panic loudly on a future regression rather than silently widen the accepted date range.

## 2026-05-08 — Two B33 trivial tests in transaction.rs (variant identity + distinctness)

- Found by: reviewer-backend
- Where: src-tauri/src/context/account/domain/transaction.rs (`opening_balance_variant_exists`, `transaction_type_variants_are_distinct`)
- Context: branch `refactor/cash-tx-aggregate-split` @ `2c2ea3e`
- Severity: 🔵
- Observation: `opening_balance_variant_exists` asserts a value equals itself; `transaction_type_variants_are_distinct` asserts the compiler-derived `PartialEq` distinguishes named variants. Both are tautological — they exercise the language, not the domain. Candidates for deletion in a B33 sweep alongside other trivial tests in the suite.

## 2026-05-10 — InfrastructureError.hint crosses IPC and round-trips

- Found by: reviewer-security
- Where: src-tauri/src/core/error.rs:21-30 (InfrastructureError::Unknown.hint) — applied across all *Error composites
- Context: branch `refactor/account-crud-typed-error` @ `cad6ee6`
- Severity: 🟡
- Observation: The `hint: String` field on `InfrastructureError::Unknown` is serialized into the IPC response and crosses the Tauri boundary verbatim. Each `format!("{e:#}")` call in service.rs forwards the full anyhow chain — SQLx error text, query fragments, file-system paths, OS diagnostics — to the frontend. The frontend then logs it via `logger.error(result.error)` which round-trips it back to the backend tracing log via `log_frontend`, producing duplicate logs and exposing diagnostic detail across IPC.

## 2026-05-10 — Tagged-at-boundary enums must avoid tuple variants

- Found by: reviewer-backend
- Where: src-tauri/src/context/account/domain/error.rs:9-25 (AccountDomainError::InvalidCurrency)
- Context: branch `refactor/account-crud-typed-error` @ `cad6ee6`
- Severity: 🔵
- Observation: The variant changed from tuple-style `InvalidCurrency(String)` to struct-style `InvalidCurrency { currency: String }` because serde's `#[serde(tag = "code")]` (internally-tagged) does not accept tuple variants. This is a breaking variant shape change for any downstream pattern-match on the tuple form. Today the only call site is `validate_currency` (updated in PR 5), but `AccountDomainError` is a public domain export through `account/mod.rs` and was previously reachable through anyhow downcasts. The same constraint will reach any other domain-error enum we expose at the FE boundary through an untagged composite — the pattern is now "no tuple variants on tagged-at-boundary enums."
