# Business Rules — Account Management (ACC)

## Context

An `Account` represents a financial account owned by the user (e.g. brokerage account, savings account, life insurance). Each account can hold positions on assets (`Holding`) created via transactions.

This spec covers the CRUD lifecycle of accounts. The Account Details view (drilling into holdings and cost basis) is covered by [`docs/spec/account-details.md`](account-details.md).

---

## Entity Definition

### Account

A financial account owned by the user.

| Field              | Business meaning                                                                                                       |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `name`             | User-defined display name (e.g. "PEA Boursorama", "Livret A"). Unique among all accounts (case-insensitive).           |
| `update_frequency` | Frequency at which the user plans to update the account's data. Informational only — no automation is triggered today. |

---

## Business Rules

### Validation

**ACC-001 — Name normalisation (backend)** _(formerly R1)_: The `name` is normalised on receipt before any validation or storage: leading and trailing whitespace is stripped.

**ACC-002 — Account validation (backend)** _(formerly R2)_: An `Account` is valid if and only if its `name` (after ACC-001 normalisation) is non-empty, and its `update_frequency` is one of the five fixed values (`Automatic`, `ManualDay`, `ManualWeek`, `ManualMonth`, `ManualYear`). Any violation is rejected with an explicit error.

**ACC-003 — Name uniqueness (backend + frontend)** _(formerly R3)_: No two accounts may share the same name (comparison is performed on the normalised name per ACC-001, case-insensitive). Any create or edit operation resulting in a duplicate is rejected by the backend with an explicit error. ACC-001, ACC-002, and ACC-003 apply to both creation and modification.

**ACC-004 — UpdateFrequency fixed list (backend + frontend)** _(formerly R4)_: The update frequency is a fixed list of five non-customisable values. It is purely informational: no automation is triggered in the current version.

> Note: active scheduling behaviour is planned for a future release.

### Deletion

**ACC-005 — Cascade delete holdings (backend)** _(formerly R5)_: Deleting an `Account` is permanent and irreversible. All associated `Holding` records are deleted in cascade. The `Asset` records themselves are not affected.

**ACC-006 — Cascade delete transactions (backend)** _(formerly R6)_: Deleting an `Account` also deletes all transactions associated with that account. The `Asset` records referenced by those transactions are not affected.

> ⚠️ **Dependency**: ACC-006 requires the Transaction feature to be available ([`docs/spec/financial-asset-transaction.md`](financial-asset-transaction.md)). Until that table exists in the database, only the cascade on `Holding` (ACC-005) is active.

### Display

**ACC-007 — Default sort (frontend)** _(formerly R8 — sort behaviour)_: The account table is sorted by name ascending by default.

**ACC-008 — Table columns (frontend)** _(formerly R8 — column definition)_: The account table exposes the following columns:

| Column    | Content                                     | Sortable                                                                                              |
| --------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Name      | `account.name`                              | Yes                                                                                                   |
| Frequency | Human-readable label for `update_frequency` | Yes — sorted on the logical enum order: Automatic → ManualDay → ManualWeek → ManualMonth → ManualYear |
| Actions   | Edit button + Delete button                 | No                                                                                                    |

The account section title is displayed in the application shell header, not in the page body. A real-time search field (partial match, case-insensitive) filters the table by name.

**ACC-009 — Sort toggle (frontend)** _(formerly R9)_: Clicking a sortable column header toggles between ascending and descending order. A visual indicator on the header reflects the active sort direction.

### Navigation

**ACC-010 — Row navigation (frontend)**: The entire account table row is clickable and navigates to the Account Details view (`/accounts/:id`). Action buttons (Edit, Delete) use `stopPropagation` and do not trigger row navigation. A `ChevronRight` icon appears on hover as a visual affordance indicating the row is navigable. The row is keyboard-accessible: Tab to focus, Enter or Space to navigate.

### Empty and Error States

**ACC-011 — No search results (frontend)** _(formerly R10)_: If the search matches no accounts, the table displays a message distinct from the empty state. The message does not invite the user to create an account.

**ACC-012 — Empty state (frontend)** _(formerly R11)_: If no `Account` exists, the table displays an explicit empty state inviting the user to create their first account via the FAB.

**ACC-013 — Loading and error states (frontend)** _(formerly R12)_: The account table exposes a loading state and an error state with a Retry button if the initial load fails.

**ACC-014 — Inline backend errors (frontend)** _(formerly R13)_: The create modal and the edit modal remain open on backend error and only signal success (close + refresh) after a positive response. Any failure (duplicate, network error, backend error) displays an inline error message inside the active modal or dialog.

### Mutations

**ACC-015 — Default UpdateFrequency (frontend)** _(formerly R7)_: The create form pre-selects `ManualMonth` as the default frequency.

**ACC-016 — Create via FAB (frontend)** _(formerly R14)_: A floating action button (bottom-right) opens a create modal with a Name field (required) and an `UpdateFrequency` selector. Submission is blocked if the name is empty or whitespace-only. After successful creation the modal closes and the table refreshes. On backend error, an inline error message is shown per ACC-014.

**ACC-017 — Edit (frontend)** _(formerly R15)_: The Edit button opens a modal with the Name and `UpdateFrequency` fields pre-filled. After saving, the modal closes and the table refreshes. On backend error, an inline error message is shown per ACC-014.

**ACC-018 — Delete empty account (frontend)** _(formerly R16)_: If the `Account` has no `Holding`, the Delete button opens a standard confirmation dialog. Confirmation triggers deletion (ACC-005, ACC-006); after deletion the dialog closes and the table refreshes.

**ACC-019 — Delete non-empty account (frontend)** _(formerly R17)_: If the `Account` has at least one `Holding`, the Delete button opens a reinforced confirmation dialog stating the number of holdings and transactions that will be permanently deleted. Counts are retrieved via the backend command defined in ACC-020. Confirmation triggers deletion and cascade (ACC-005, ACC-006); after deletion the dialog closes and the table refreshes.

> ⚠️ **Dependency**: ACC-019 depends on ACC-020 (pre-deletion count command), which crosses the `account/` and `transaction/` bounded contexts and requires a dedicated use case. Until ACC-020 is implemented, ACC-018 applies for all deletions (standard dialog without counts).

**ACC-020 — Pre-deletion count query (backend)** _(new)_: A backend command `get_account_deletion_summary(account_id)` returns the number of active holdings and the number of transactions associated with the account. Because this read spans `context/account/` (holdings) and `context/transaction/` (transactions), it must be implemented as a use case in `use_cases/` — not as a bounded-context command — per ADR-003 and ADR-004.

---

## User Workflow

```
[User opens "Accounts"]
  → Table (default sort: Name asc) + FAB
          │
          ├─ [Search] → Real-time filter by name (ACC-008)
          │              → No match → distinct message (ACC-011)
          │
          ├─ [Row click] → Navigate to Account Details (ACC-010)
          │
          ├─ [Column header] → Toggle asc/desc sort + visual indicator (ACC-009)
          │
          ├─ [FAB] → Create modal (Name + Frequency, default ManualMonth)
          │            → Inline error if name empty or duplicate (ACC-014)
          │            → Account created → modal closed → table refreshed
          │
          ├─ [Edit] → Edit modal (Name + Frequency pre-filled)
          │            → Inline error if duplicate (ACC-014)
          │            → Saved → modal closed → table refreshed
          │
          └─ [Delete]
              ├─ Empty account → Standard dialog (ACC-018) → Confirm → Delete (ACC-005, ACC-006)
              └─ Non-empty account → Reinforced dialog (ACC-019, holding + tx count)
                                     → Confirm → Delete + cascade (ACC-005, ACC-006)
```

---

## UX Draft

### Entry point

**Accounts** — item in the main navigation drawer.

### Main component

Full-width table, default sort by Name ascending. Floating action button (bottom-right). Each row is fully clickable (navigates to Account Details, ACC-010) with a `ChevronRight` hover affordance. Edit and Delete buttons per row (stop propagation).

### States

- **Empty**: message inviting the user to create their first account via the FAB (ACC-012)
- **Loading**: loading indicator in the table (ACC-013)
- **Load error**: error message + Retry button (ACC-013)
- **No search results**: message distinct from empty state (ACC-011)
- **Inline error**: message displayed inside the active modal or dialog (ACC-014)
- **Delete empty account confirmation**: standard confirmation dialog (ACC-018)
- **Delete non-empty account confirmation**: reinforced dialog — "This account contains X holding(s) and Y transaction(s). All data will be permanently deleted." (ACC-019)
- **Mutation success**: silent today (modal/dialog closes, table refreshes); snackbar feedback planned (see Future Improvements)

### User flow

1. User opens the Accounts page (section title shown in the shell header).
2. User clicks a row → navigates to Account Details for the selected account (ACC-010).
3. User clicks FAB → modal → enters name and selects frequency → submits → account created.
4. User clicks Edit → pre-filled modal → modifies → saves → modal closes.
5. User clicks Delete (empty account) → standard dialog → confirms → account deleted.
6. User clicks Delete (account with positions) → reinforced dialog → confirms → account + positions + transactions deleted.

---

## Future Improvements

- **Archiving**: Implement account archiving (soft-delete) instead of permanent deletion to preserve transaction history.
- **Success feedback**: Implement snackbar notifications for all mutations (ACC-014 currently only covers error feedback).
