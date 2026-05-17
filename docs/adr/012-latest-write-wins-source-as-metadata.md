# ADR 012 — Latest-write-wins for source-qualified entities; source field is metadata, not precedence

**Date**: 2026-05-16
**Status**: Accepted — supersedes ADR-010

## Context

ADR-010 introduced a "Manual overrides External" precedence rule for source-qualified entities (`AssetPrice`, `CurrencyRate`). The rule had auto-fetch flows check for a Manual row before writing an External row, skipping silently when one existed. The stated goal was to protect users from auto-fetch silently overwriting an intentional manual entry.

In the immediate follow-up review (same day as ADR-010 landed), the rule was challenged on common-case grounds:

- **Onboarding** — a user types a last-known price when adding an asset; the next auto-fetch should _replace_ that placeholder, not be blocked by it.
- **Backfill** — a user types a historical price for a date Stooq does not cover; this is a different `(asset, date)` row and never conflicts with auto-fetch in the first place.
- **No-coverage assets** — Stooq does not cover the asset; Manual is the only source ever written; no conflict.
- **Trade-derived prices** (`record_price=true`) — the user just executed a real trade at that price; this is more current than any prior manual entry on the same day.
- **User overrides because the auto-fetched value looks wrong** — the rare case ADR-010 was protecting. The user re-typing is one easy action.

Optimising the four common cases at the cost of one extra user action in the rare case is the better trade. The "Manual is a pin" assumption that ADR-010 baked in does not match how this app is actually used: Manual entries are usually placeholders or fills for what auto-fetch will eventually provide, not deliberate pins.

## Decision

For any source-qualified entity (`AssetPrice`, `CurrencyRate`, future entities of the same shape):

1. **Latest write wins per `(business_key, date)`**, regardless of source. Repository upserts unconditionally — no source-based skip, no precedence check at write time.
2. **The `source` enum field is retained as metadata**: it lets the price-history UI render a per-row badge ("entered by you" vs "from Stooq"), supports debugging, and is available for future audit or filtering features. It does NOT influence which row wins on read or write.
3. **Repository read** returns the row at `(business_key, date)` via the primary-key lookup (no `ORDER BY source` expression). For "most recent value for an asset," reads pick the row with the latest `date` — date is the dimension that orders price history, not source.
4. **No "lock" / "pin" flag in v1**. If the rare "I want this Manual entry to survive auto-fetch" case becomes a recurring user pain, a future ADR can introduce an explicit pin mechanism. Until then, the user re-typing is the documented workflow.

This decision **simplifies** the model — it removes the source-precedence read query, the write-time existence check, and the matrix of test cases ADR-010 mandated. The MKT spec's existing MKT-025 upsert rule (last-write-wins per `(asset_id, date)`) now stands unmodified.

Alternatives considered:

- **Keep ADR-010 as-is (Manual wins)** — rejected on the common-case argument above. Optimises for a rare scenario at the cost of ergonomic onboarding and auto-fetch.
- **Add a separate `pinned: bool` flag distinct from `source`** — rejected for v1 as YAGNI. The source enum + a future "pin" affordance can be added later without breaking the simpler model defined here.
- **Latest-write-wins only between user-source writes; auto-fetch still blocked by Manual** — rejected as a half-measure. Splits the rule into two cases with subtle differences; same complexity as ADR-010 with a thinner rationale.

## Consequences

- **Pros**: schema and read path simpler than ADR-010 (no source-aware ORDER BY); auto-fetch logic is a single unconditional upsert; onboarding and "Stooq fills in what I placeholder-typed" works naturally; no surprising "my new manual entry didn't persist" reports because every entry persists at write time; the `source` field still does real work (history badges, debugging); test surface shrinks (no precedence matrix).
- **Cons**: a user who intentionally overrode an auto-fetched value must re-override after each subsequent fetch on the same date (typically once per day at most, often zero times because auto-fetch only writes today); when a pin mechanism is later needed, the data model has to gain a flag and the write path has to gain a check — but that change is contained and additive, not a model reversal.

## Migration impact

ADR-010 landed on `main` minutes before this ADR was written; **no implementation existed yet** that depended on the precedence rule. Migration impact is therefore zero — the MKT amendment work that was about to start is now simpler, not retroactively broken.

ADR-008 and ADR-009 (asset price provider chain; FX rate provider chain) reference "Manual wins" inline; their text is corrected in-place in the same branch as this ADR. The README index is updated to flip ADR-010's status to `Superseded by ADR-012`.
