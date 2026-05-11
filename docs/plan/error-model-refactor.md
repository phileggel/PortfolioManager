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
- Translated infra failure → **application** (project-specific tightening — see "Infra translation rule" below)

### Infra translation rule (project-specific tightening)

The kit doc gives an OR for infra failures: "translated at the application boundary into either a meaningful application error OR an opaque variant." This project picks the **first option as the default**, with the opaque variant reserved for the rare cases where no meaningful BC-level name exists (e.g. a true panic caught at the Tauri boundary). Concrete rules for this codebase:

1. **Each BC's `*ApplicationError` enum carries its own infra-class variant** (`DatabaseError`, and later `ExternalApiError`, `FileSystemError`, etc. as new infra dependencies appear in that BC). The variant is a unit variant — no payload, no `hint` field on the wire.
2. **The shared `InfrastructureError` type does NOT appear on the FE wire surface** — it must not be a leaf in any `*Error` composite, must not be a Tauri command's return type, and must not be Specta-derived if retained at all. It may survive only as a backend-internal type (today: not even that — services translate `anyhow::Error` from the repo trait directly).
3. **The application layer is the only place infra translation happens**, and it always does two things at the same site: (a) call `tracing::error!` to preserve the full diagnostic chain server-side, (b) return the typed `*ApplicationError::DatabaseError` variant. No `format!("{e:#}")` payload-building on the wire.
4. **Aggregate members** (e.g. Holding, Transaction inside the Account aggregate) follow their parent aggregate's BC — a `holding_repo` or `transaction_repo` failure surfaces as `AccountApplicationError::DatabaseError`, not a member-specific variant.
5. **Cross-BC infra failures** (e.g. an orchestrator calling `asset_service.get_asset_by_id()` which fails) propagate via the asset BC's translated variant (`AssetApplicationError::DatabaseError`) re-exported through the use-case composite via `#[from]`. The use case does not re-name the failure; its variants are reserved for cross-BC PRECONDITIONS the orchestrator itself enforces.

**Layer flow:**

```
Infrastructure   →  Application                  →  Boundary       →  Frontend
(repo trait)        (service / use case)            (Tauri cmd)
─────────────       ─────────────────────────       ─────────────     ─────────────
anyhow::Result   →  tracing::error!(...)         →  passes typed   →  { code:
(opaque, raw       AccountApplicationError::         error through      "DatabaseError" }
sqlx errors)       DatabaseError                    unchanged
```

**Why the shared `InfrastructureError` type is decorative today**: the repo trait returns `anyhow::Result<T>`, so the type info is already lost before the application layer sees the error. A shared `InfrastructureError` wrapper around `anyhow::Error` adds a layer of indirection without adding signal — the application layer can't pattern-match into it for finer-grained translation. Per-BC `*ApplicationError::DatabaseError` is the right granularity given this constraint, and is honestly named because each BC's repos are all SQLite-backed today.

**Future** (out of scope for the error-model refactor): if the repository trait is later typed (e.g. `Result<T, RepositoryError>` with variants like `UniqueViolation`, `ConnectionLost`), the application layer's translation can become more discriminating (`UniqueViolation` → `NameAlreadyExists` retry path, etc.). The wire contract stays the same — per-BC `*ApplicationError::*` variants. The shared `RepositoryError` becomes a backend-internal typed contract; still never on the FE wire.

**Migration discipline**: this rule is enforced PR-by-PR going forward. PR 5 still ships with `Infrastructure(InfrastructureError)` as a leaf in `AccountCrudError` (it predates this rule's ratification). PR 6+ migrates one BC at a time per the surgical update principle — no big-bang rewrite. The rule applies to every NEW typed surface added from this point on.

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
| Account CRUD         | create/update (delete narrowed to InfraErr; archive/unarchive don't exist)    | AccountAppErr, AccountDomainErr, InfraErr                                                 | `AccountCrudError`                                                   | service                                  | ✅ done in PR 5 |
| Asset CRUD           | create/update/archive/unarchive/delete (reads narrowed to AssetAppErr)        | AssetAppErr, AssetDomainErr, CategoryAppErr (NO InfraErr — translated to AppErr::DatabaseError) | `AssetCrudError`                                                     | service                                  | ✅ done in PR 7 |
| Category CRUD        | create/update/delete (get_categories narrowed to CategoryAppErr)              | CategoryAppErr, CategoryDomainErr (NO InfraErr — translated to AppErr::DatabaseError)     | `CategoryCrudError`                                                  | service                                  | ✅ done in PR 6 |
| Asset price          | record/update/delete asset_price                                              | AssetAppErr, AssetPriceAppErr, AssetPriceDomainErr (NO InfraErr — translated to AppErr::DatabaseError) | `AssetPriceError` (collapse 3 over-split composites into 1)          | service                                  | ✅ done in PR 8 |
| Archive/Delete asset | archive_asset, delete_asset use cases                                         | AssetCrudErr, AccountAppErr, ArchiveAssetAppErr / DeleteAssetAppErr (NO InfraErr — translated to AppErr::DatabaseError) | `ArchiveAssetError` / `DeleteAssetError` (separate; surfaces differ) | use case                                 | ✅ done in PR 9 |
| Account details      | get_account_details                                                           | AccountAppErr (NO InfraErr — translated to AppErr::DatabaseError)                         | `AccountApplicationError` directly (single-leaf read — no composite needed) | use case                                 | ✅ done in PR 10 |
| Account deletion     | get_account_deletion_summary (read-only — actual delete is `account::delete_account` covered in PR 5) | AccountAppErr (NO InfraErr — translated to AppErr::DatabaseError) | `AccountApplicationError` directly (single-leaf read — no composite needed) | use case | ✅ done in PR 11 |
| Web lookup           | lookup_asset (OpenFIGI)                                                       | WebLookupAppErr (NetworkError — translated from raw HTTP/network failure)                 | `WebLookupApplicationError` directly (single-leaf, single-variant)   | use case                                 | ✅ done in PR 12 |

**PR 3 family-merge note**: The original map split "Cash recording" (deposit/withdrawal) and "Holding transaction" (buy/sell/correct/cancel) into separate families. PR 3 collapsed them — both share an identical leaf set (AccountAppErr + AccountOpErr + TxDomainErr + InfraErr) because they're all kinds of holding transaction (cash deposit/withdrawal IS a holding transaction against the System Cash Asset, CSH-014). One composite (`HoldingTransactionError`) covers all six commands. Also: `get_transactions` (read-only) was narrowed to `Result<Vec<Transaction>, InfrastructureError>` directly — the wider composite is reserved for write commands.

**PR 4 leaf-split note**: Three variants previously in `OpeningBalanceDomainError` (`AssetNotFound`, `ArchivedAsset`, `OpeningBalanceOnCashAsset`) were migrated to a new `OpenHoldingApplicationError` enum (use-case-owned, in `use_cases/holding_transaction/error.rs`) per the rejection-layer rule (`docs/ddd-reference.md` § Errors) — they're cross-BC asset-check rejections born at the orchestrator, not aggregate invariants. `OpeningBalanceDomainError` now holds only the genuinely-domain `InvalidTotalCost` (raised by `Account::open_holding` on its own input).

**PR 4 layering exception (worth ratifying explicitly)**: `AccountService::open_holding` returns `OpenHoldingError`, which is owned by `use_cases/holding_transaction/`. This inverts the canonical dependency arrow (BC service depending on a use-case-owned type). The pragmatic alternative — defining a smaller `OpenHoldingServiceError` in `account/application/` and nesting composites — was rejected as over-engineered for a single method. The choice is **acceptable for exactly one method** and should NOT be cargo-culted: the standard pattern (PR 3 precedent: `HoldingTransactionError` in `account/application/` covering 6 methods) is to define the composite in the BC that owns the failure leaves. Future families with cross-BC-leaves should prefer the nested-composite approach unless the same one-method economics apply. May be promoted to an ADR if the pattern recurs.

**PR 5 read-narrowing standardisation**: Three commands (`get_accounts`, `delete_account`, `get_asset_ids_for_account`) were narrowed to `Result<_, InfrastructureError>` directly rather than returning the wider `AccountCrudError` composite — they have no domain-rejection paths (reads only fail on infra; delete cascades at the DB level). This continues the precedent set by PR 3 (`get_transactions`) and is now the standard pattern: **the composite is reserved for commands with at least one domain or application leaf; pure-infra surfaces use `InfrastructureError` directly**. Apply the same test to every future command in PRs 6+. Also: `AccountDomainError::InvalidCurrency(String)` was migrated from tuple to struct variant `{ currency: String }` because serde's internally-tagged enums (`#[serde(tag = "code")]`) don't accept tuple variants — recorded in `docs/techdebt.md` as a constraint applying to any domain enum exposed at the FE boundary.

**PR 6 first enforcement of the gold infra-translation rule**: Category CRUD is the first family migrated under the new "Infra translation rule (project-specific tightening)" ratified above. Differences from PR 5's shape:

- `CategoryCrudError` composite has **NO `Infrastructure` leaf** — infra failures translate at the application layer into `CategoryApplicationError::DatabaseError` (unit variant, no `hint` payload).
- The shared `InfrastructureError` type does NOT appear in the FE wire surface for any Category command.
- `get_categories` narrowed to `Result<_, CategoryApplicationError>` (just the leaf, since only `DatabaseError` is reachable) — replaces the PR 5 pattern of narrowing reads to the shared `InfrastructureError`. Read-narrowing now returns the BC's typed application leaf, NOT the shared infra type.
- Per the rejection-layer rule, `CategoryDomainError::NotFound(String)` and `DuplicateName` were migrated OUT of the domain layer into `CategoryApplicationError::NotFound { id }` and `DuplicateName` (both service-layer rejections). The remaining `CategoryDomainError` variants (`LabelEmpty`, `SystemReadonly`, `SystemProtected`) are correctly retained as domain-class.
- A new `application/` directory was created in `asset/` (kit v4.4 gold layout B0/B38) — surgical creation only because we needed it for the new error file. The asset BC's broader gold-layout migration (`core/`/`repository/` renames) remains deferred per the bit-by-bit policy.
- One follow-up tracked: `get_category_by_id` kept on `anyhow::Result` because Asset CRUD (PR 7+) still calls it with anyhow expectations. Will be narrowed when Asset CRUD migrates.

**PR 7 cross-aggregate composition**: Asset CRUD introduces the first cross-aggregate composition leaf — `AssetCrudError::CategoryApplication(#[from] CategoryApplicationError)` — for the category lookup in `create_asset` / `update_asset`. The category leaf propagates verbatim per composition-over-redefinition (NotFound) and infra-translation (DatabaseError); the asset BC does not re-translate. This pattern generalises to any future composite that depends on cross-aggregate (or eventually cross-BC) typed surfaces. Two service methods (`get_asset_by_id`, `seed_cash_asset`) kept on anyhow because the holding-tx orchestrator still consumes them through anyhow; will narrow when that orchestrator's sweep lands. `record_asset_price` / `get_asset_prices` (Asset price family — PR 8+) updated minimally to raise `AssetApplicationError::NotFound { id }` instead of the removed `AssetDomainError::NotFound`. Use-case mappers (`archive_asset`, `delete_asset`) updated to downcast `AssetCrudError` (composite) instead of bare `AssetDomainError`; the use-case Tauri boundaries themselves stay on anyhow until their dedicated PR. Per kit v4.4 gold layout B0/B38, the asset BC's `application/` directory is now populated with both Category and Asset application leaves.

**PR 8 same-BC cross-aggregate composition**: Asset price family extends the cross-aggregate composition pattern within the same BC — `AssetPriceError::AssetApplication(#[from] AssetApplicationError)` for the cross-aggregate asset-existence check in `record_asset_price` / `get_asset_prices`. New `AssetPriceApplicationError` (`PriceNotFound { asset_id, date }`, `DatabaseError`) replaces the deleted `AssetPriceDomainError::NotFound` per the rejection-layer rule (a repo-lookup miss is service-level, not aggregate-invariant). Domain factory `AssetPrice::new` was tightened from `anyhow::Result` to `Result<Self, AssetPriceDomainError>`; new `InvalidDateFormat { date }` variant surfaces malformed dates typed instead of opaque `Unknown`. Three over-split command-error enums and their downcast mappers in `asset/api.rs` collapsed into one composite — the four asset_price commands now return `Result<_, AssetPriceError>` directly.

**PR 9 first cross-BC composite + AccountApplicationError follow-on**: `archive_asset` and `delete_asset` use cases promote their standalone `*Error` enums into untagged composites following the PR 4 (`OpenHoldingError`) precedent — use-case-owned `*ApplicationError` leaves (`ArchiveAssetApplicationError::ActiveHoldings`, `DeleteAssetApplicationError::ExistingTransactions`) live in `use_cases/{archive,delete}_asset/error.rs`, and the composites compose `AssetCrudError` + `AccountApplicationError` + the use-case leaf. **Boyscout follow-on**: `AccountApplicationError` gained a `DatabaseError` variant under the gold infra-translation rule (account BC parity with asset / category BCs); the two cross-BC service methods consumed by these orchestrators (`AccountService::has_active_holdings_for_asset`, `has_holding_entries_for_asset`) were tightened from `anyhow::Result<bool>` to `Result<bool, AccountApplicationError>` with infra translation at the same site. Use-case `api.rs` boundary types (`ArchiveAssetCommandError` / `DeleteAssetCommandError`) and their downcast mappers deleted; commands return the composites directly. The standalone `Infrastructure(InfrastructureError)` leaf in `AccountCrudError` (carried over from PR 5) is **not yet removed** — pending the PR final sweep when `AccountService` write commands migrate to the gold infra rule.

**PR 10 single-leaf read narrowing (no composite needed)**: `get_account_details` is the second instance of the PR 6 read-narrowing pattern (after `get_categories`) — the command returns `Result<AccountDetailsResponse, AccountApplicationError>` directly because the only reachable leaf is `AccountApplicationError` (`AccountNotFound { account_id }` from the load + `DatabaseError` from any repo translation). The `AccountDetailsError` standalone enum and `AccountDetailsCommandError` boundary type were both deleted — no composite, no mapper, no `error.rs` file. **Boyscout follow-on**: `AccountService::get_by_id` and `get_holdings_for_account` were tightened from `anyhow::Result` to `Result<_, AccountApplicationError>` with infra translation at the call site; the cross-BC `asset_service.get_asset_by_id` calls inside the orchestrator translate locally to `AccountApplicationError::DatabaseError` (FK integrity violations included — they shouldn't happen but get logged as data corruption). Side effect: `HoldingTransactionUseCase::ensure_cash_for` lost its hand-written `InfrastructureError::Unknown { hint: format!(...) }` map_err and now relies on `From<AccountApplicationError> for HoldingTransactionError` via `?` — leaner one-line call.

**PR 11 third single-leaf read narrowing**: `get_account_deletion_summary` follows the same pattern — the family map's "Account deletion" entry was misleading (no actual delete-with-cascade command exists; the use case is purely a read for the FE delete-confirmation dialog). Returns `Result<AccountDeletionSummary, AccountApplicationError>` directly — no `AccountDeletionError` to define, no `AccountDeletionCommandError` boundary type, no mapper. **Boyscout follow-on**: `AccountService::get_deletion_summary` tightened from `anyhow::Result<(u32, u32)>` to `Result<(u32, u32), AccountApplicationError>` with infra translation around the `tokio::try_join!` of the two count repos.

**PR 12 single-leaf single-variant rename**: `lookup_asset` is the simplest case — pure-infra failure surface (HTTP only, no domain rejections). The `WebLookupCommandError` boundary type was already single-variant (`NetworkError`); this PR moves it from `api.rs` into a new use-case-owned `error.rs` as `WebLookupApplicationError`, tightens `AssetWebLookupUseCase::search` and `collect_keyword_hits` to `Result<_, WebLookupApplicationError>` with `tracing::warn!` at each translation site (3 sites: ISIN map, keyword search, share-class enrichment), and drops the api.rs mapper. The `OpenFigiClient` trait stays on `anyhow::Result` (HTTP-layer interface unchanged). **Boyscout follow-on**: 3 existing `propagates_*_error_as_anyhow` tests upgraded from `.is_err()` checks to typed-variant `matches!` assertions and renamed (`*_failure_translates_to_network_error`).

**Composite count**: ~30 commands → ~9 families → ~9 composites.

### PR 3+ — One family per PR

Each follow-up PR migrates ONE family from anyhow-mapped boundary types to a typed composite per the table above. Each family also:

- Introduces (or extends) the relevant `*ApplicationError` enum
- Composes via `#[from]` on each leaf and `#[serde(untagged)]` on the composite
- Reuses the shared `core::InfrastructureError` (no new per-BC `Unknown { hint }`)
- Splits aggregate methods if needed (per the "no domain composite" rule)
- DELETES any boundary type / mapper that no longer adds work (the composite IS the FE contract)
- Stays under ~600 LOC

### PR final — `refactor: drop InfrastructureError from FE wire surface` (✅ done — branch `refactor/anyhow-drop-final`)

The closing PR removed the last `anyhow::Result` returns reachable from the FE wire surface, deleted the shared `core::InfrastructureError` type entirely (the only remaining users were dead doc references after PRs 6–12 migrated every composite), and translated infra failures at the application layer into per-BC `*ApplicationError::DatabaseError` (typed, payload-free) with the diagnostic chain preserved server-side via `tracing::error!`.

Specifically:

- **Account BC** — `service::get_all` / `delete` / `get_asset_ids_for_account` / `get_transactions` now return `Result<_, AccountApplicationError>` (was `InfrastructureError` / `anyhow`); `get_holding_by_account_asset` / `get_transaction_by_id` typed (was `anyhow`); all `create` / `update` map_err sites translate to `DatabaseError`. `HoldingTransactionError` and `AccountCrudError` lost their `Infrastructure` leaf.
- **Asset BC** — `service::get_asset_by_id` typed (was `anyhow`). `seed_cash_asset` kept on `anyhow` — mixed BC error sources (asset_repo + category_repo + domain validation) would either lose origin information under a single typed translation or require a composite that exists only for an internal seed path.
- **Use cases** — `OpenHoldingError` gained `AssetApplication(#[from] AssetApplicationError)` so the orchestrator's `?` works cleanly with the now-typed `get_asset_by_id`; the orchestrator's `ensure_cash_for` translates to `AccountApplicationError::DatabaseError`.
- **Specta** — `.typ::<InfrastructureError>()` removed from the builder; `src-tauri/src/core/error.rs` deleted; the `pub use error::InfrastructureError;` re-export removed from `core/mod.rs`.
- **FE** — `src/bindings.ts` regenerated; `accountGateway.{getAccounts,deleteAccount}` and `transactionGateway.{getTransactions,getAssetIdsForAccount}` now return `AccountApplicationError`; gateway tests' `{ code: "Unknown", hint: ... }` fixtures replaced with `{ code: "DatabaseError" }`.

Backend-rules tightening (the `B??` rule reviewer-backend should enforce going forward) remains as a follow-up — see `docs/techdebt.md`.

## Out of scope (deferred)

- **Folder reshape** — keep `service.rs` at BC root for now; later refactor can move `service.rs` into `application/` across all BCs at once (separate chore)
- **Reviewer rule** — adding a `B??` rule to `docs/backend-rules.md` requiring typed Results on application services; do at the end after the pattern is fully proven
- **Frontend gateway error handling** — FE will receive the same wire shape, but downstream FE code may want to handle the new richer error structure differently. Out of scope; flag as a follow-up if the FE team raises it.
- **Per-command vs per-method error enums** at the boundary — the current per-command pattern stays. Whether to consolidate later is a separate design discussion.

## Tracking

This plan supersedes the two relevant entries in `docs/todo.md`. After PR 1 lands, those TODO entries should be updated to reference this plan and reflect progress. After all PRs are done, both entries can be removed.
