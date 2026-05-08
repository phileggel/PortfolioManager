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
