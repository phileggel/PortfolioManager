# ADR 008 — Asset price provider chain: Stooq primary, Finnhub fallback (BYOK), Manual override

**Date**: 2026-05-16
**Status**: Accepted

## Context

VaultCompass is an indie EU/FR-focused portfolio tracker. The forthcoming Portfolio Dashboard (PFD) needs current asset prices to compute unrealized P&L and aggregate KPIs across accounts. Today MKT only supports manual price entry, which puts a daily-update burden on the user; without an automated path the dashboard's "current value" stays stale.

The pattern required is **on-app-launch fetch**: a small burst (~5–50 requests, one per holding) at startup, cached per-day, with a manual refresh button. Per-minute throttling on a key-less provider hurts this pattern — the OpenFIGI WEB feature already hit this wall (5 req/min on `/v3/search` → 429 cascades during a typical search session).

The decision is which external price provider(s) to bind to, and how user-entered prices interact with auto-fetched ones. A 2026 review of provider reliability also drove the fallback choice: Yahoo Finance's unofficial endpoints are increasingly blocked by Yahoo (IP throttling, CAPTCHA gates, endpoint rotation), making them a poor reliability bet despite the no-key convenience.

## Decision

Use a three-tier source chain for `AssetPrice`, captured by a new `source: AssetPriceSource` enum (`Manual | Stooq | Finnhub`):

1. **Stooq CSV** (`https://stooq.com/q/?s=...&f=...&e=csv`) — primary auto-fetch source. No API key, no documented rate limit, EU+US coverage including Euronext Paris (`tte.fr`).
2. **Finnhub** (`https://finnhub.io/api/v1/quote?symbol=...&token=...`) — fallback when Stooq returns no data for a symbol. Requires a free user-supplied API key (60 req/min on the free tier). Key storage and lifecycle live in ADR-011 / the KEY spec; consumers here only call the gateway.
3. **Manual** — user-entered prices via MKT; always available.

Write semantics per `(asset_id, date)`: latest write wins regardless of source (per ADR-012). The auto-fetch flow unconditionally upserts; no source-based skip. The `source` enum is metadata for the price-history UI and debugging — it does not influence read or write precedence. The `source` column is persisted as a SQLite text discriminant (matching the enum variant name), not an integer — text keeps migrations and ad-hoc database inspection trivial.

**Stooq symbol resolution**: derive the Stooq symbol from `(ticker, exchange_code)` initially — most cases collapse to lowercasing the ticker and appending an exchange suffix (`TTE` + `PA` → `tte.fr`). Add an explicit `stooq_symbol: Option<String>` field on `Asset` later only if derivation proves brittle on real assets. Avoids a schema change for the common case.

Behavior when the user has not supplied a Finnhub key: the chain degrades to Stooq-only. Assets uncovered by Stooq fall through to "no auto-fetched price; user can enter manually." No silent fallback to an unauthenticated provider — that path is what Yahoo would have given us and is rejected (see Alternatives).

Alternatives considered:

- **Yahoo Finance v7 unofficial** — rejected as fallback in 2026. Yahoo has been actively blocking unofficial endpoint consumers: rotating URLs, requiring CAPTCHAs, and IP-throttling aggressive callers. Multiple maintained wrappers (yfinance, yahoo-finance2) publish defensive patches as their primary release cadence. The "no-key convenience" no longer outweighs the operational fragility for a portfolio app that must produce consistent numbers across launches.
- **Finnhub as primary** — rejected. The free-key requirement adds onboarding friction; Stooq's no-key path lets the app produce useful results immediately for the typical user. Finnhub as fallback respects the indie default while giving power users a reliability lift.
- **Alpha Vantage** — rejected. Free tier 25 req/day. Mathematically incompatible with portfolios above a handful of holdings.
- **Twelve Data** — viable third-tier fallback (800 req/day on free key, multi-asset). Not adopted in v1 to keep the chain short; can be added later as a fourth tier without breaking the source-qualifier model.
- **Polygon.io** — rejected. Free tier is end-of-day only and US-only.
- **IEX Cloud** — rejected. Discontinued at end of 2024.

## Consequences

- **Pros**: zero-friction default onboarding (Stooq works without setup); reliability lift available to power users via free Finnhub key (60 req/min, 20-min delay, includes Euronext on free tier); user-entered corrections survive the next launch; per-row source provenance enables debugging and audit; fallback chain remains short and reasoned about per-tier.
- **Cons**: Stooq symbol derivation from `(ticker, exchange_code)` will miss some edge cases (mid-name venue renames, dual-listed share classes) — the deferred `stooq_symbol` field is the escape hatch; Finnhub fallback only helps users who set up a key (the Connections panel in KEY spec mitigates this with link-out + test-connection UX); Finnhub free-tier EU coverage is documented as Euronext-inclusive but uneven for some smaller venues — coverage gaps will need permanent Manual entry; no real-time pricing — Stooq is ~15 min delayed for many venues, which is correct for portfolio valuation but disqualifies the system for trading use.
