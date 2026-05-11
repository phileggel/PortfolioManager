# Error Model

Reference for handling errors in this codebase. Directive, not historical.

---

## Rules

1. **Repos return `anyhow::Error`.** The application layer translates to a typed enum. Domain and use-case layers never see `anyhow`.
2. **Infra failures translate to `*ApplicationError::DatabaseError`** at the application layer. Unit variant — no payload on the wire. The diagnostic chain is logged server-side via `tracing::error!` at the same site as the translation.
3. **Domain errors are raised by aggregate methods on their own loaded state.** Anything else (NotFound from a repo lookup, cross-aggregate uniqueness checks, cross-BC orchestration verdicts) is an application error.
4. **Composites at the boundary, tagged leaves underneath.** Composites use `#[serde(untagged)]`; leaves use `#[serde(tag = "code")]`. Wire shape is always `{ code: "VariantName", ...payload }`.
5. **New variants go in the leaf, not the composite.** The composite reflects what its leaves expose; never re-declare leaf codes inside the composite.

---

## Decision tree

> I'm adding or changing an error path. Where does it go?

- **Raised by an aggregate method on its own loaded state?** (e.g. `Account::apply_withdrawal` rejecting `InsufficientCash`)
  → Domain leaf (`{bc}/domain/error.rs`).

- **Raised by a service-layer check** (NotFound from `repo.get_by_id`, uniqueness pre-check, cross-aggregate gating)?
  → Application leaf (`{bc}/application/error.rs`).

- **Raised by a use-case orchestrator** (cross-BC verdict like `ActiveHoldings`, `ExistingTransactions`, `OpeningBalanceOnCashAsset`)?
  → Use-case-owned application leaf (`use_cases/{name}/error.rs`).

- **Raised by an infra failure** (sqlx error, repo I/O, connection lost)?
  → Do NOT add a new variant. Translate to the relevant `*ApplicationError::DatabaseError` at the call site:

  ```rust
  repo.something().await.map_err(|e| {
      tracing::error!(target: BACKEND, ..context fields.., err = ?e, "service_method: what failed");
      AccountApplicationError::DatabaseError
  })?;
  ```

- **Need a payload on the wire?**
  → Use a struct variant. Tuple variants don't survive `#[serde(tag = "code")]`.

  ```rust
  { code: "Oversell"; available: number; requested: number }
  ```

- **Need a new command surface that composes errors from multiple sources?**
  → New composite in the layer that owns the surface (BC for BC writes, use-case for cross-BC orchestration). Each leaf via `#[from]`.

---

## Recipes

### Leaf enum

```rust
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum AccountApplicationError {
    #[error("Account not found: {account_id}")]
    AccountNotFound { account_id: String },

    #[error("Account name already exists")]
    NameAlreadyExists,

    #[error("An unexpected database error occurred")]
    DatabaseError,
}
```

### Composite

```rust
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum HoldingTransactionError {
    #[error(transparent)]
    Application(#[from] AccountApplicationError),
    #[error(transparent)]
    Operation(#[from] AccountOperationError),
    #[error(transparent)]
    Validation(#[from] TransactionDomainError),
}
```

### Service method signature

```rust
pub async fn record_deposit(...) -> Result<Transaction, HoldingTransactionError> {
    let mut account = load_account(&*self.account_repo, account_id).await?;  // AccountApplicationError → ?
    let tx = Transaction::new_deposit(...)?;                                  // TransactionDomainError → ?
    let tx = account.apply_deposit(tx)?;                                      // AccountOperationError → ?
    save_account(&*self.account_repo, &mut account).await?;                   // AccountApplicationError → ?
    Ok(tx)
}
```

### Tauri command boundary

The composite IS the FE-facing contract. No mapper, no boundary type:

```rust
#[tauri::command]
#[specta::specta]
pub async fn record_deposit(
    uc: State<'_, HoldingTransactionUseCase>,
    dto: DepositDTO,
) -> Result<Transaction, HoldingTransactionError> {
    uc.record_deposit(...).await
}
```

### Frontend handling

The wire shape is a flat union of every leaf's variants. Narrow on `code`:

```ts
const result = await accountGateway.recordDeposit(dto);
if (result.status === "error") {
    switch (result.error.code) {
        case "AccountNotFound":   // ...
        case "InsufficientCash":  // ...
        case "DatabaseError":     // i18n key error.DatabaseError
        // ...
    }
}
```

---

## Anti-patterns

- ❌ Returning `anyhow::Result<T>` from an application service method that surfaces to a Tauri command.
- ❌ Adding a `Database` / `Infrastructure` / `Unknown` variant carrying a `String` hint to the FE.
- ❌ `format!("{e:#}")` into a wire-visible payload.
- ❌ Re-declaring leaf variant codes inside a composite (e.g. flattening `AccountApplicationError`'s codes into `HoldingTransactionError`).
- ❌ Putting NotFound in the domain layer (it's a service-layer translation of `Ok(None)`).
- ❌ Using tuple variants on a leaf (`#[serde(tag = "code")]` rejects them — use struct variants).
- ❌ Two leaves of the same composite with the same `code` discriminant under `#[serde(untagged)]` (silent collision; first arm wins).
- ❌ `panic!` / `unwrap` / `expect` in production paths. Tests only.
- ❌ Documenting per-leaf variants in the composite's docstring (rots the moment a leaf changes — point at the leaf type instead).
- ❌ Comments like `// Replaces the anyhow-era X` or `// Per the Y rule` (rationale-as-comment; doc what the code IS, not what it used to be).

---

## Where things live

| What | Where |
|---|---|
| Per-BC application leaves | `src-tauri/src/context/{bc}/application/error.rs` |
| Per-BC domain leaves | `src-tauri/src/context/{bc}/domain/` (each aggregate has its own error type) |
| Use-case composites and leaves | `src-tauri/src/use_cases/{name}/error.rs` |
| All composites + leaves on the FE wire | `src/bindings.ts` (auto-generated; do not edit) |
| Per-command reachable code surface | `docs/contracts/{domain}-contract.md` |
| Layering rules (domain vs application) | `docs/ddd-reference.md` § Errors |
| Backend coding rules | `docs/backend-rules.md` |

---

## Known limits

These are documented soft spots. Do not paper over them; do not pretend they don't exist.

- **`AccountOperationError` and `OpeningBalanceDomainError` raised by aggregate methods that return `anyhow::Result`.** Service-layer bridges (`to_holding_tx_error`, `to_open_holding_error` in `account/service.rs`) downcast the typed errors out and translate the rest to `DatabaseError`. The bridges are intentional until aggregate methods are split into typed factory + apply pairs. New aggregate methods should return typed `Result` directly.

- **Composite codes are an upper bound, not the actual reachable set per command.** When `AccountApplicationError` gains a variant, every composite that includes it advertises that variant — even commands that can't raise it. The actual per-command surface lives in `docs/contracts/{domain}-contract.md`. Keep the contract narrower than the type when relevant; use the contract (not the type) to drive FE exhaustiveness.

- **`#[serde(untagged)]` matches first arm in declaration order.** Two leaves of the same composite with overlapping `code` discriminants will collide silently. Verify uniqueness when adding a leaf.

- **Specta + serde derives leak into the domain layer.** Domain error types must derive `Serialize + specta::Type` to be composable into FE-wire enums. Acceptable trade for typed bindings; constrains future non-Specta consumers.

- **Repos use `type Error = anyhow::Error`.** Translation happens at the application layer. Do not propagate `anyhow` past the service boundary.
