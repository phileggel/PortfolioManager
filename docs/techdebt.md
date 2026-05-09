# Tech Debt

Observations of code smells, brittle patterns, or pre-existing issues surfaced
during work that don't warrant immediate action. Format produced by the kit's
`/techdebt` skill — see `.claude/kit-tools.md`.

Entries are observations, not commitments. Triaged by `/whats-next` alongside
`docs/todo.md`. Remove an entry once it has been resolved or moved to
`docs/todo.md` for active work.

---

## 2026-05-08 — System-asset guard discriminates on AssetClass::Cash, not on a system marker
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
