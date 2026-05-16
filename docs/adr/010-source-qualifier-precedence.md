# ADR 010 — Source-qualifier precedence: Manual overrides External per (key, date)

**Date**: 2026-05-16
**Status**: Accepted

## Context

ADR-008 introduces `AssetPrice.source ∈ {Manual, Stooq, Finnhub}` and ADR-009 introduces `CurrencyRate.source ∈ {Manual, Frankfurter, Ecb}`. Both bounded contexts face the same precedence question: when a Manual row and an External row both exist for the same natural key on the same date, which one wins on read, and what does the auto-fetch flow do on write?

Recording the answer once as a cross-cutting pattern — rather than letting each spec re-derive it — keeps the two BCs aligned and makes the rule a reference for any future source-qualified entity (e.g. a `BenchmarkValue` BC, an `EarningsEstimate` BC).

## Decision

For any source-qualified entity, the natural key is `(business_key, date)` — `(asset_id, date)` for `AssetPrice`, `(from_currency, to_currency, date)` for `CurrencyRate`. The precedence rule is:

1. **Manual always wins over any External source** per `(business_key, date)`.
2. **Auto-fetch flow MUST check for a Manual row before writing an External row.** If a Manual row exists for the date, the auto-fetch skips silently — no overwrite, no error, no log noise above debug.
3. **Two External sources for the same date**: the most recently written row wins. This case only arises during the refresh button or a cache-repair sequence; in normal operation auto-fetch runs once per day and writes only today's row.
4. **Repository read** returns the precedence-winning row via SQL `ORDER BY` on a `source` priority expression (Manual ranked above External; among External the most recent `updated_at`), with `LIMIT 1`. Application-layer post-filtering is rejected — it allows partial reads if the result set is paginated and forces every consumer to re-implement the same logic. Integration tests assert the full matrix (Manual + External, External + External, External alone, Manual alone).

Alternatives considered:

- **External wins on overwrite** — rejected. The user's intentional correction would be silently overwritten on the next launch. This is the UX failure mode the rule exists to prevent.
- **Latest-write-wins regardless of source** — rejected. Degenerates to External-wins in practice because auto-fetch runs daily; same failure mode.
- **Never overwrite anything** — rejected. Stale or wrong cached rows would be unrepairable, defeating the refresh button.
- **Separate `locked: bool` flag distinct from `source`** — rejected. Adds a second field with semantically identical purpose: a Manual entry already encodes "user pinned this value."

## Consequences

- **Pros**: a single rule reused across both AssetPrice and CurrencyRate; user-intentional edits always survive the next auto-fetch; auto-fetch logic stays trivial ("check Manual, skip if present, write External, end"); read path is deterministic and unit-testable; pattern generalises to any future source-qualified entity.
- **Cons**: per-row `source` field carries a small storage and serialization cost on multiple entities; repository tests must cover the precedence matrix per entity (4 cases each); resetting to auto requires the user to explicitly delete the Manual row — the system never silently abandons a Manual entry to "let auto take over again."
