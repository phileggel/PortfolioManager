# Business Rules — Asset Web Lookup (WEB)

## Context

The Asset Web Lookup feature allows users to search for financial instrument metadata from the OpenFIGI API (maintained by Bloomberg) before creating a new asset. The user types a name, ticker, or ISIN into a search box, selects an instrument from the returned list, and the Add Asset form is pre-filled with the retrieved metadata (name, reference, currency, and asset class). All pre-filled fields remain editable; the user saves via the existing `add_asset` command.

This is a **feature spec** extending the asset creation flow. The new Tauri command issues an outbound HTTP request to the OpenFIGI API and lives in `use_cases/asset_web_lookup/` — consistent with the `update_checker` use case, which is also an external HTTP concern. No new persisted entity is introduced; lookup results are transient.

---

## Value Object Definition

### AssetLookupResult

A transient value object returned by the OpenFIGI API. Not persisted; used only to pre-fill the Add Asset form.

| Field         | Business meaning                                                                                                                                        |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`        | Full name of the financial instrument (e.g. "Apple Inc.").                                                                                              |
| `reference`   | ISIN or ticker symbol; pre-fills the Add Asset `reference` field. Absent when the keyword search path finds no ticker for the result (WEB-046).         |
| `currency`    | ISO 4217 trading currency of the instrument (e.g. "USD"). Absent if OpenFIGI does not return one for the result.                                        |
| `asset_class` | Classification of the instrument mapped from the OpenFIGI `securityType`. Absent if the type is unrecognised (WEB-023).                                 |
| `exchange`    | Human-readable market name derived from the OpenFIGI `exchCode` (e.g. "Euronext Paris", "NYSE"). Absent if OpenFIGI returns no exchange code (WEB-049). |

---

## Business Rules

### Entry Point and Initiation (010–019)

**WEB-010 — Asset creation entry point (frontend)**: Initiating the creation of a new asset opens the web lookup step instead of going directly to the blank Add Asset form.

**WEB-011 — Minimum query length (frontend)**: The search action requires at least 1 character in the query field. An empty query disables the search action.

**WEB-012 — Single query input (frontend)**: The lookup step exposes a single query input field that accepts any text — ISIN, ticker symbol, or instrument name. No mode selector or query type hint is shown; the routing decision is made transparently by the backend (WEB-014).

**WEB-013 — Fill manually bypass (frontend)**: A "Fill manually" action is always visible in the lookup step. Activating it skips the web lookup entirely and opens the blank Add Asset form, preserving the pre-existing creation path.

**WEB-014 — Query routing (backend)**: When `lookup_asset` receives a query, it applies the following routing rule: if the trimmed query is exactly 12 alphanumeric characters it is sent to the OpenFIGI ISIN mapping endpoint; all other queries — including queries that contain non-alphanumeric characters or are shorter or longer than 12 characters — are sent to the OpenFIGI keyword search endpoint.

### Lookup Command (020–029)

**WEB-020 — Backend command (backend)**: A new Tauri command `lookup_asset(query: String) -> Result<Vec<AssetLookupResult>, WebLookupCommandError>` issues an HTTP request to the OpenFIGI API using the routing logic defined in WEB-014. The command returns a (possibly empty) ordered list of results on success.

**WEB-021 — No API key required (backend)**: The OpenFIGI API is accessed without authentication. No credential is stored or transmitted.

**WEB-022 — Result limit (backend)**: The command returns at most 10 results. If the OpenFIGI response contains more, only the first 10 are forwarded.

**WEB-023 — Asset class mapping (backend)**: The OpenFIGI `securityType` field is mapped to `AssetClass` as follows: `"Common Stock"` → `Stocks`; `"ETF"` → `ETF`; `"Mutual Fund"` → `MutualFunds`; `"Corporate Bond"` / `"Government Bond"` → `Bonds`; `"Cryptocurrency"` / `"Digital Currency"` → `DigitalAsset`; `"REIT"` / `"Real Estate Investment Trust"` → `RealEstate`; `"Cash"` → `Cash`; `"Warrant"` / `"Option"` / `"Future"` / `"Rights"` → `Derivatives`. Any unrecognised `securityType` (including `"Structured Product"`, `"Certificate"`, and others) results in `asset_class` being absent from the result.

**WEB-024 — Currency passthrough (backend)**: The ISO 4217 currency code returned by OpenFIGI is forwarded unchanged. If OpenFIGI does not return a currency for a result, the `currency` field is absent.

**WEB-025 — Error handling (backend)**: `WebLookupCommandError` has a single variant: `NetworkError`. It covers all failure modes: network unreachable, connection timeout, and any non-2xx HTTP status returned by OpenFIGI (including rate-limiting responses). No partial result list is returned on error. The enum is intentionally closed; rate-limiting is not surfaced as a distinct variant — the client receives `NetworkError` in all failure cases.

### Search UX (030–039)

**WEB-030 — Loading state (frontend)**: While `lookup_asset` is in progress, a loading indicator is shown and the search action is disabled to prevent duplicate requests.

**WEB-031 — Results display (frontend)**: Each `AssetLookupResult` is shown as a two-line selectable row. First line: reference code (if present, displayed as a muted prefix) followed by the instrument name. Second line: the asset class label if present, otherwise a localised "unknown type" fallback label; if `exchange` is also present it is shown alongside, separated by a visual separator. When `exchange` is absent the second line shows only the type label (or the fallback). When both `asset_class` and `exchange` are absent the second line shows only the fallback label. The currency field is not shown in the results list; it is pre-filled silently into the form on selection (WEB-041).

**WEB-032 — Empty results state (frontend)**: When the command returns an empty list, a message indicates no instruments were found. The user can modify the query and search again, or use the "Fill manually" bypass (WEB-013).

**WEB-033 — Error state (frontend)**: When the command returns a `NetworkError`, an inline error message is shown with a retry affordance. The "Fill manually" bypass (WEB-013) remains accessible. No navigation away from the search step occurs on error.

### Selection and Pre-fill (040–049)

**WEB-040 — Result selection (frontend)**: Selecting a result from the list transitions to the Add Asset form with fields pre-filled from the selected `AssetLookupResult`.

**WEB-041 — Pre-filled fields (frontend)**: The following Add Asset form fields are pre-filled from the selected result: `name` ← `AssetLookupResult.name`; `reference` ← `AssetLookupResult.reference` (blank if absent); `currency` ← `AssetLookupResult.currency` (blank if absent); `asset_class` ← `AssetLookupResult.asset_class` (no selection if absent).

**WEB-042 — Risk level default from asset class (frontend)**: When opening the Add Asset form from the web lookup path (creation only), if `asset_class` is pre-filled, `risk_level` is automatically set to the class default, consistent with the `AssetClass::default_risk()` behaviour defined in AST-010. When `asset_class` is absent, `risk_level` is left at its form default. This rule applies exclusively to the creation flow; it does not affect the edit form.

**WEB-043 — All pre-filled fields are editable (frontend)**: Every pre-filled field in the Add Asset form can be changed by the user before saving. The lookup result is a suggestion, not a locked value.

**WEB-044 — Category default (frontend)**: The `category` field is not provided by the OpenFIGI lookup and defaults to the system default category, consistent with the existing manual form behaviour.

**WEB-045 — Save via existing add_asset command (frontend + backend)**: Saving the pre-filled form uses the existing `add_asset` command. All existing Asset creation rules apply — reference uniqueness check, field validation, and `AssetUpdated` event publication — as defined in the AST spec. The web lookup path introduces no new save rules.

**WEB-046 — Reference field source (backend)**: When the lookup path is ISIN (WEB-014), `AssetLookupResult.reference` is the ISIN string from the query. When the lookup path is keyword search, `reference` is the ticker symbol returned by OpenFIGI when available; when OpenFIGI does not return a ticker for a result, `reference` is absent.

**WEB-047 — Back navigation from form to search results (frontend)**: When the form is in the pre-filled state (WEB-040), a back action is available that returns the user to the search step. The previous query and results list are retained in memory; the user does not need to retype the query. Selecting a different result replaces all pre-filled values.

**WEB-048 — Result ordering (backend)**: Results are sorted by instrument type priority before the 10-item truncation (WEB-022). Priority is determined by the resolved `asset_class` value (WEB-023): Priority 1 (top) — `asset_class` ∈ {`Stocks`, `ETF`, `MutualFunds`, `Bonds`, `DigitalAsset`, `RealEstate`, `Cash`}; Priority 2 — `asset_class` = `Derivatives`; Priority 3 — `asset_class` absent (unrecognised `securityType`, including structured products and certificates). Within each priority group, the original OpenFIGI response order is preserved.

**WEB-049 — Exchange field (backend)**: The OpenFIGI `exchCode` field is resolved to a human-readable market name via a static, hardcoded in-code lookup table (e.g. `"FP"` → `"Euronext Paris Stock Exchange"`, `"UN"` → `"New York Stock Exchange"`, `"UW"` → `"Nasdaq Global Select Market"`). The table mirrors OpenFIGI's official exchange-codes dictionary (published as a CSV at `openfigi.com`). Unknown codes fall back to the raw `exchCode` string. The resolved name is forwarded as `exchange` in `AssetLookupResult`. If OpenFIGI returns no `exchCode`, `exchange` is absent.

**WEB-050 — Primary listing surfacing (backend)**: The keyword search path applies a deduplication-and-enrichment pipeline so that the user is shown the asset's primary listing(s), not the dozens of secondary OTC/MTF listings OpenFIGI returns by default. The pipeline:

1. **Common Stock filter on the initial keyword search**: the `/v3/search` request includes `securityType: "Common Stock"` so bonds, futures, structured products and warrants are excluded at source.
2. **Drop noise**: results whose `shareClassFIGI` is `null` are discarded (these are pure trade-reporting venue rows such as `X1` "TradEcho APA EU"; they carry no canonical share class).
3. **Dedup by share class**: remaining results are grouped by `shareClassFIGI`. Multiple keyword-search hits for the same share class collapse into one group.
4. **Share-class enrichment**: for the unique `shareClassFIGI` values from step 3, a single batched call is made to `/v3/mapping` with `idType: "ID_BB_GLOBAL_SHARE_CLASS_LEVEL"`. The response — all known listings for that share class globally — replaces the original keyword-search hits for the group. This is the step that uncovers primary listings (e.g. `FP AI`) that the keyword search alone never returns.
5. **Primary pick per group**: the entries of each group are filtered against `GLOBAL_VENUE_PRIORITY` — a hardcoded ordered list of primary venue `exchCode` values (e.g. `UN`, `UW`, `LO`, `JT`, `FP`, `GY`, `HK`, `SE`, `AT`, `CT`, `IM`, `NA`, …). All entries whose `exchCode` is on the list are kept, in priority order. Up to 3 entries per share class are kept (cap chosen so dual-listed names like TotalEnergies surface both their NYSE and Euronext rows). If no entry on the list matches, the first entry from OpenFIGI's order is kept as a fallback so the share class is not lost.
6. **Final cap (WEB-022)**: the combined result list across all share classes is truncated to 10.

The ISIN search path (WEB-014) calls `/v3/mapping` directly and skips steps 1–4; only steps 5–6 apply, ensuring consistent primary-pick behaviour regardless of entry path. The opinionated tables and pipeline live in a single `primary_listing_processor` module so they can be audited and tested in isolation.

---

## Workflow

```
Add Asset FAB / button
    → Web Lookup step (WEB-010)
        user types ISIN / ticker / name → search (WEB-011, WEB-012)
            backend: route query (WEB-014)
            backend: HTTP to OpenFIGI (WEB-020)
            → returns up to 10 AssetLookupResult items (WEB-022)
        → results list shown (WEB-031)
        → user selects a result (WEB-040)
        → Add Asset form opens pre-filled (WEB-041–WEB-046)
            user reviews / edits fields (WEB-043)
            ← back action available to return to results (WEB-047)
            → save → existing add_asset command (WEB-045)
            → AssetUpdated published; asset appears in list

Bypass path:
    → "Fill manually" (WEB-013) → blank Add Asset form (existing behaviour)

No results (WEB-032):
    → "No instruments found" + retry or fill manually

Error (WEB-033):
    → inline error + retry or fill manually
```

---

## UX Draft

### Entry Point

Clicking the "Add Asset" FAB opens the web lookup dialog. A "Fill manually" link/button is always visible as an escape hatch.

### Main Component

A dialog or modal with two sequential states:

1. **Search state** — query input + search button + "Fill manually" bypass + results list (or loading / empty / error state).
2. **Form state** — the existing Add Asset form, pre-filled (or blank if bypass used) + back action.

### States

- **Idle**: Empty query input, search button disabled (WEB-011). "Fill manually" visible.
- **Loading**: Spinner shown, search action disabled (WEB-030).
- **Results**: Up to 10 selectable rows (WEB-031), ordered by type priority (WEB-048). Each row: reference code + name (first line); type label + exchange (second line).
- **Empty**: "No instruments found" message; retry or fill manually (WEB-032).
- **Error**: Inline error banner with retry; fill manually always accessible (WEB-033).
- **Form (pre-filled)**: Add Asset form fields populated from selected result; all editable (WEB-041–WEB-043). Back action returns to search results (WEB-047).
- **Form (manual)**: Blank Add Asset form — identical to current behaviour.

### User Flow

1. User clicks "Add Asset".
2. Web lookup dialog opens: query input + "Fill manually" bypass.
3. User types "AAPL", a 12-char ISIN, or a fund name and clicks Search.
4. Backend fetches from OpenFIGI; results appear.
5. User clicks the matching row.
6. Add Asset form opens with name, reference, currency, asset class pre-filled.
7. User reviews, adjusts category and risk level if needed, and saves.
8. Existing `add_asset` command runs; asset appears in the list.
