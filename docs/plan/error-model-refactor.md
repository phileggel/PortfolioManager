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

### Rule B' (classification)

> An error is a **domain** error if and only if it is raised by an aggregate method (or value-object constructor) enforcing an invariant on its own loaded state or input.
> Anything raised by the service or use case layer — NotFound, uniqueness checks, cross-BC preconditions, infrastructure failures — is **application** (or **infrastructure** for opaque catch-alls).

Concretely:
- Aggregate method rejection → domain
- Service-level pre-check → application (move into aggregate if the rule is intrinsic to the entity)
- Use-case orchestrator rejection (cross-BC) → application
- Translated infra failure → application or opaque `Infrastructure(hint)` catch-all

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

### PR 3+ — One service per session, same pattern

Each follow-up PR refactors one service's methods to typed Results, applying the same conformance pattern. Order TBD based on user pain / coupling. Rough list:

- `AccountService` non-cash methods: `create_account`, `update_account`, `delete_account`, `archive`/`unarchive`, `buy`, `sell`, `correct_transaction`, `cancel_transaction`, plus cross-BC guard queries
- `AssetService` methods: `create_asset`, `update_asset`, `archive`, `unarchive`, `delete`, `add_price`, `update_price`, `delete_price`, `create_category`, `update_category`, `delete_category`
- Use-case orchestrators: `OpenHoldingUseCase`, `ArchiveAssetUseCase`, `DeleteAssetUseCase`, `AccountDeletionUseCase`, `AccountDetailsUseCase`, `AssetWebLookupUseCase`

Each follow-up PR also:
- Introduces or extends the relevant `*ApplicationError` enum
- Updates the corresponding boundary `*CommandError` to compose untagged
- Splits aggregate methods if needed (per the "no domain composite" rule)
- Stays under ~600 LOC

### PR final — `refactor: remove anyhow from service signatures` (cleanup)

When all services are migrated, a small final PR removes the last `use anyhow` lines from `service.rs`/`orchestrator.rs` and tightens the `reviewer-backend` agent / `backend-rules.md` to enforce the new shape going forward.

## Out of scope (deferred)

- **Folder reshape** — keep `service.rs` at BC root for now; later refactor can move `service.rs` into `application/` across all BCs at once (separate chore)
- **Reviewer rule** — adding a `B??` rule to `docs/backend-rules.md` requiring typed Results on application services; do at the end after the pattern is fully proven
- **Frontend gateway error handling** — FE will receive the same wire shape, but downstream FE code may want to handle the new richer error structure differently. Out of scope; flag as a follow-up if the FE team raises it.
- **Per-command vs per-method error enums** at the boundary — the current per-command pattern stays. Whether to consolidate later is a separate design discussion.

## Tracking

This plan supersedes the two relevant entries in `docs/todo.md`. After PR 1 lands, those TODO entries should be updated to reference this plan and reflect progress. After all PRs are done, both entries can be removed.
