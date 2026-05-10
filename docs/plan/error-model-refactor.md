# Error Model Refactor — Plan

Multi-PR refactor to bring the backend error model into conformance with `docs/ddd-reference.md` § Errors.

---

## Why

Today's services return `anyhow::Result<T>` and the API boundary downcasts at runtime. This:

- Loses the type-system guarantee that "this method can only emit errors X, Y, Z"
- Allows `RepoError::NotFound` and similar infrastructure errors to leak across the application boundary
- Spreads the same error variants across multiple boundary types (the duplication PR #5 began removing for cash)

The DDD doc now codifies the canonical shape (domain wrapped, application born, infrastructure translated). This refactor migrates the codebase to it, one service at a time.

## References

- `docs/ddd-reference.md` § Errors — canonical model
- PR #5 — introduced `#[serde(untagged)]` boundary composition for cash (`RecordDepositCommandError` / `RecordWithdrawalCommandError`)
- `docs/todo.md` — original entries:
  - "Convert service layer methods to typed Result returns"
  - "Roll out untagged-composition pattern for boundary error types"

## Locked rules

### Rejection-layer rule (canonical reference)

The classification rule that governs every PR in this refactor — internally referred to as **"Rule B'"** through PR descriptions 1–4 — is now codified by the kit as the **rejection-layer rule** in [`docs/ddd-reference.md`](../ddd-reference.md) § Errors. That is the canonical statement; consult it when in doubt.

Kit alignment: codified in **claude-kit v4.4.0** (synced 2026-05-10), alongside the related anemic-domain rule **B37** in `docs/backend-rules.md`. PRs 1–4 of this plan were authored against the same definition before it had a name in the kit; the framing was correct, only the citation moved.

Quick reminder of the test (full version in the kit doc):

- Aggregate method rejection → **domain**
- Service-level pre-check (`NotFound`, uniqueness) → **application** (move into the aggregate if the rule is intrinsic — see B37)
- Use-case orchestrator rejection (cross-BC) → **application**
- Translated infra failure → **application** or opaque `Infrastructure(hint)` catch-all

### Layering

```
Boundary (api.rs)             ←  *CommandError      composes via #[serde(untagged)]
Application (service.rs)      ←  *ServiceError      composes domain leaves + app + infra
                                 *ApplicationError  per BC, e.g. AccountApplicationError
Domain (domain/*.rs)          ←  *DomainError       leaf enums, one per concept
```

### Composition over redefinition

Each error variant defined exactly once at its owning layer. Higher layers compose via `#[from]` + `#[serde(untagged)]`. No variant duplication across enums.

### No "domain composite" wrappers

If an aggregate method needs to emit multiple kinds of errors, split the method (e.g., separate value-object construction from aggregate application) so each method returns a single leaf. Composition only happens at the application layer where the orchestration is meaningful.

## PR sequence

### PR 1 — `refactor(asset/category): move state checks into aggregates`

**Scope**: behavior-preserving move of 4 state-checks from `service.rs` into the corresponding aggregate methods. No new error types; no boundary changes; no FE wire-shape changes.

**Variants moved (each stays in its existing enum, classification stays domain):**

- `AssetDomainError::Archived` → `Asset::update_from`
- `AssetDomainError::CashAssetNotEditable` → `Asset::update_from`, `archive`, `unarchive`, `delete` (every mutating method)
- `CategoryDomainError::SystemReadonly` → `Category::update_from`
- `CategoryDomainError::SystemProtected` → `Category::delete`

**Acceptance**:

- Existing service tests still assert the same errors
- New aggregate-level tests assert the checks happen inside the aggregate
- `just check-full` green
- ~150–300 LOC

### PR 2 — `refactor(account): typed Result on cash methods + AccountApplicationError`

**Scope**: introduce the typed-Result pattern on the cash slice of `AccountService`. Delete `CashOperationError` composite. Split `Account::record_deposit` / `record_withdrawal` into value-object construction (`Transaction::new_deposit`/`new_withdrawal`) + aggregate application (`Account::apply_deposit`/`apply_withdrawal`).

**Changes**:

- New file `context/account/application/error.rs` with `AccountApplicationError { AccountNotFound, NameAlreadyExists }`
- Move `AccountNotFound` and `NameAlreadyExists` out of `AccountDomainError`
- New `Transaction::new_deposit` / `new_withdrawal` constructors returning `TransactionDomainError`
- New `Account::apply_deposit` / `apply_withdrawal` returning `AccountOperationError`
- New `RecordDepositServiceError` / `RecordWithdrawalServiceError` composing leaves + `AccountApplicationError` + `InfrastructureError`
- `AccountService::record_deposit` / `record_withdrawal` return the new typed errors
- Boundary types `RecordDepositCommandError` / `RecordWithdrawalCommandError` updated to compose the new shape
- Delete `CashOperationError`
- Test updates

**Acceptance**:

- Same Tauri wire shape (FE bindings unchanged in shape)
- `just check-full` green
- No `anyhow::Error` returned from refactored AccountService methods
- ~400–600 LOC

### Failure-surface-family map (target end-state for PR 3+)

A **failure-surface-family** = methods sharing the same set of leaf errors. ONE composite per family, owned by the layer that holds the FE contract (orchestrator or service). All composites compose the **shared `core::InfrastructureError`** (no per-BC catch-all redefinition). Composites live under `application/error.rs` (service-owned) or `use_cases/{flow}/error.rs` (orchestrator-owned).

| Family               | Methods                                                                       | Leaves                                                                                    | Composite (target)                                                   | Owner                                    | Status          |
| -------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------- | --------------- |
| Holding transaction  | record_deposit, record_withdrawal, buy_holding, sell_holding, correct, cancel | AccountAppErr, AccountOpErr, TxDomainErr, InfraErr                                        | `HoldingTransactionError`                                            | service (delegated through orchestrator) | ✅ done in PR 3 |
| Open holding         | open_holding                                                                  | AccountAppErr, OpenHoldingAppErr, OpeningBalanceDomainErr, TxDomainErr (subset), InfraErr | `OpenHoldingError`                                                   | use case (cross-BC archived check)       | ✅ done in PR 4 |
| Account CRUD         | create/update/delete/archive/unarchive account                                | AccountAppErr, AccountDomainErr, InfraErr                                                 | `AccountCrudError` (rename `AccountCommandError`)                    | service                                  | PR 5+           |
| Asset CRUD           | create/update/archive/unarchive/delete asset                                  | AssetAppErr (new), CategoryAppErr (new), AssetDomainErr, InfraErr                         | `AssetCrudError` (rename `AssetCommandError`)                        | service                                  | PR 5+           |
| Category CRUD        | create/update/delete category                                                 | CategoryAppErr, CategoryDomainErr, InfraErr                                               | `CategoryCrudError`                                                  | service                                  | PR 5+           |
| Asset price          | record/update/delete asset_price                                              | AssetAppErr, AssetPriceDomainErr, InfraErr                                                | `AssetPriceError` (collapse 3 over-split composites into 1)          | service                                  | PR 5+           |
| Archive/Delete asset | archive_asset, delete_asset use cases                                         | AssetAppErr, AssetDomainErr, AccountAppErr, InfraErr                                      | `ArchiveAssetError` / `DeleteAssetError` (separate; surfaces differ) | use case                                 | PR 5+           |
| Account details      | get_account_details                                                           | AccountAppErr, InfraErr                                                                   | `AccountDetailsError`                                                | use case                                 | PR 5+           |
| Account deletion     | delete_account_with_assets                                                    | AccountAppErr, AssetAppErr, InfraErr                                                      | `AccountDeletionError`                                               | use case                                 | PR 5+           |
| Web lookup           | lookup_asset_via_openfigi                                                     | mostly InfraErr, maybe WebLookupAppErr                                                    | `WebLookupError`                                                     | use case                                 | PR 5+           |

**PR 3 family-merge note**: The original map split "Cash recording" (deposit/withdrawal) and "Holding transaction" (buy/sell/correct/cancel) into separate families. PR 3 collapsed them — both share an identical leaf set (AccountAppErr + AccountOpErr + TxDomainErr + InfraErr) because they're all kinds of holding transaction (cash deposit/withdrawal IS a holding transaction against the System Cash Asset, CSH-014). One composite (`HoldingTransactionError`) covers all six commands. Also: `get_transactions` (read-only) was narrowed to `Result<Vec<Transaction>, InfrastructureError>` directly — the wider composite is reserved for write commands.

**PR 4 leaf-split note**: Three variants previously in `OpeningBalanceDomainError` (`AssetNotFound`, `ArchivedAsset`, `OpeningBalanceOnCashAsset`) were migrated to a new `OpenHoldingApplicationError` enum (use-case-owned, in `use_cases/holding_transaction/error.rs`) per the rejection-layer rule (`docs/ddd-reference.md` § Errors) — they're cross-BC asset-check rejections born at the orchestrator, not aggregate invariants. `OpeningBalanceDomainError` now holds only the genuinely-domain `InvalidTotalCost` (raised by `Account::open_holding` on its own input).

**PR 4 layering exception (worth ratifying explicitly)**: `AccountService::open_holding` returns `OpenHoldingError`, which is owned by `use_cases/holding_transaction/`. This inverts the canonical dependency arrow (BC service depending on a use-case-owned type). The pragmatic alternative — defining a smaller `OpenHoldingServiceError` in `account/application/` and nesting composites — was rejected as over-engineered for a single method. The choice is **acceptable for exactly one method** and should NOT be cargo-culted: the standard pattern (PR 3 precedent: `HoldingTransactionError` in `account/application/` covering 6 methods) is to define the composite in the BC that owns the failure leaves. Future families with cross-BC-leaves should prefer the nested-composite approach unless the same one-method economics apply. May be promoted to an ADR if the pattern recurs.

**Composite count**: ~30 commands → ~9 families → ~9 composites.

### PR 3+ — One family per PR

Each follow-up PR migrates ONE family from anyhow-mapped boundary types to a typed composite per the table above. Each family also:

- Introduces (or extends) the relevant `*ApplicationError` enum
- Composes via `#[from]` on each leaf and `#[serde(untagged)]` on the composite
- Reuses the shared `core::InfrastructureError` (no new per-BC `Unknown { hint }`)
- Splits aggregate methods if needed (per the "no domain composite" rule)
- DELETES any boundary type / mapper that no longer adds work (the composite IS the FE contract)
- Stays under ~600 LOC

### PR final — `refactor: remove anyhow from service signatures` (cleanup)

When all services are migrated, a small final PR removes the last `use anyhow` lines from `service.rs`/`orchestrator.rs`, removes the redundant `*::Unknown` variants on per-command boundary types (replaced by shared `InfrastructureError`), and tightens the `reviewer-backend` agent / `backend-rules.md` to enforce the new shape going forward.

## Out of scope (deferred)

- **Folder reshape** — keep `service.rs` at BC root for now; later refactor can move `service.rs` into `application/` across all BCs at once (separate chore)
- **Reviewer rule** — adding a `B??` rule to `docs/backend-rules.md` requiring typed Results on application services; do at the end after the pattern is fully proven
- **Frontend gateway error handling** — FE will receive the same wire shape, but downstream FE code may want to handle the new richer error structure differently. Out of scope; flag as a follow-up if the FE team raises it.
- **Per-command vs per-method error enums** at the boundary — the current per-command pattern stays. Whether to consolidate later is a separate design discussion.

## Tracking

This plan supersedes the two relevant entries in `docs/todo.md`. After PR 1 lands, those TODO entries should be updated to reference this plan and reflect progress. After all PRs are done, both entries can be removed.
