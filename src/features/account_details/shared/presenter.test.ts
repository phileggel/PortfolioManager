import { describe, expect, it } from "vitest";
import type {
  AccountDetailsResponse,
  AssetPriceSource,
  ClosedHoldingDetail,
  HoldingDetail,
} from "@/bindings";
import {
  formatSource,
  formatStaleness,
  toAccountSummary,
  toClosedHoldingRow,
  toHoldingRow,
} from "./presenter";

const makeHolding = (overrides: Partial<HoldingDetail> = {}): HoldingDetail => ({
  asset_id: "asset-1",
  asset_name: "Apple Inc",
  asset_reference: "AAPL",
  quantity: 2_000_000,
  average_price: 100_000_000,
  cost_basis: 200_000_000,
  realized_pnl: 0,
  asset_currency: "EUR",
  current_price: null,
  current_price_date: null,
  current_price_source: null,
  unrealized_pnl: null,
  performance_pct: null,
  ...overrides,
});

const makeClosedHolding = (overrides: Partial<ClosedHoldingDetail> = {}): ClosedHoldingDetail => ({
  asset_id: "asset-2",
  asset_name: "Closed Corp",
  asset_reference: "CLSD",
  realized_pnl: 0,
  last_sold_date: "2024-12-31",
  ...overrides,
});

const makeResponse = (overrides: Partial<AccountDetailsResponse> = {}): AccountDetailsResponse => ({
  account_name: "My Portfolio",
  holdings: [makeHolding()],
  closed_holdings: [],
  total_holding_count: 1,
  total_cost_basis: 200_000_000,
  total_realized_pnl: 0,
  total_unrealized_pnl: null,
  total_global_value: 0,
  ...overrides,
});

describe("toHoldingRow", () => {
  it("formats quantity with 6 decimals", () => {
    const row = toHoldingRow(makeHolding({ quantity: 1_500_000 }));
    expect(row.quantity).toBe("1,500000");
  });

  it("formats averagePrice with 2 decimals", () => {
    const row = toHoldingRow(makeHolding({ average_price: 150_000_000 }));
    expect(row.averagePrice).toBe("150,00");
  });

  it("formats costBasis with 2 decimals", () => {
    const row = toHoldingRow(makeHolding({ cost_basis: 300_000_000 }));
    expect(row.costBasis).toBe("300,00");
  });

  it("maps asset metadata fields correctly", () => {
    const row = toHoldingRow(makeHolding());
    expect(row.assetId).toBe("asset-1");
    expect(row.assetName).toBe("Apple Inc");
    expect(row.assetReference).toBe("AAPL");
  });

  it("formats realizedPnl with 2 decimals (SEL-042)", () => {
    const row = toHoldingRow(makeHolding({ realized_pnl: 5_000_000 }));
    expect(row.realizedPnl).toBe("5,00");
    expect(row.realizedPnlRaw).toBe(5_000_000);
  });

  it("passes quantityMicro as raw value for sell modal (SEL-010)", () => {
    const row = toHoldingRow(makeHolding({ quantity: 3_500_000 }));
    expect(row.quantityMicro).toBe(3_500_000);
  });
});

describe("toAccountSummary", () => {
  it("formats totalCostBasis with 2 decimals", () => {
    const summary = toAccountSummary(makeResponse({ total_cost_basis: 250_000_000 }));
    expect(summary.totalCostBasis).toBe("250,00");
  });

  it("formats totalRealizedPnl with 2 decimals (SEL-042)", () => {
    const summary = toAccountSummary(makeResponse({ total_realized_pnl: 12_500_000 }));
    expect(summary.totalRealizedPnl).toBe("12,50");
  });

  it("isEmpty true when total_holding_count is 0", () => {
    const summary = toAccountSummary(makeResponse({ total_holding_count: 0, holdings: [] }));
    expect(summary.isEmpty).toBe(true);
    expect(summary.isAllClosed).toBe(false);
  });

  it("isAllClosed true when holdings exist but active list is empty (ACD-034)", () => {
    const summary = toAccountSummary(
      makeResponse({
        total_holding_count: 2,
        holdings: [],
        closed_holdings: [makeClosedHolding()],
      }),
    );
    expect(summary.isEmpty).toBe(false);
    expect(summary.isAllClosed).toBe(true);
  });

  it("neither isEmpty nor isAllClosed when active holdings present", () => {
    const summary = toAccountSummary(makeResponse());
    expect(summary.isEmpty).toBe(false);
    expect(summary.isAllClosed).toBe(false);
  });
});

describe("toClosedHoldingRow", () => {
  // ACD-044 — closed holding detail maps to a view model row
  it("maps closed holding metadata fields (ACD-044)", () => {
    const row = toClosedHoldingRow(makeClosedHolding());
    expect(row.assetId).toBe("asset-2");
    expect(row.assetName).toBe("Closed Corp");
    expect(row.assetReference).toBe("CLSD");
  });

  // ACD-049 — realized P&L formatted to 2 decimal places
  it("formats realizedPnl with 2 decimals (ACD-049)", () => {
    const row = toClosedHoldingRow(makeClosedHolding({ realized_pnl: 15_000_000 }));
    expect(row.realizedPnl).toBe("15,00");
  });

  // ACD-049 — raw realized P&L exposed for sign-based colour styling
  it("exposes realizedPnlRaw as micro-unit number (ACD-049)", () => {
    const row = toClosedHoldingRow(makeClosedHolding({ realized_pnl: -5_000_000 }));
    expect(row.realizedPnlRaw).toBe(-5_000_000);
  });

  // ACD-049 — last_sold_date passed through verbatim
  it("passes lastSoldDate through verbatim (ACD-049)", () => {
    const row = toClosedHoldingRow(makeClosedHolding({ last_sold_date: "2025-06-15" }));
    expect(row.lastSoldDate).toBe("2025-06-15");
  });

  // ACD-047 — toAccountSummary totalRealizedPnl covers active + closed (backend sums, presenter passes through)
  it("toAccountSummary totalRealizedPnl includes closed positions pnl (ACD-047)", () => {
    const summary = toAccountSummary(
      makeResponse({
        total_realized_pnl: 35_000_000,
        closed_holdings: [makeClosedHolding({ realized_pnl: 25_000_000 })],
      }),
    );
    expect(summary.totalRealizedPnl).toBe("35,00");
  });

  // ACD-050 — empty closed_holdings list → hasClosedHoldings false
  it("toAccountSummary hasClosedHoldings is false when closed_holdings is empty (ACD-050)", () => {
    const summary = toAccountSummary(makeResponse({ closed_holdings: [] }));
    expect(summary.hasClosedHoldings).toBe(false);
  });

  // ACD-044/ACD-048 — hasClosedHoldings is true when closed_holdings is non-empty
  it("toAccountSummary hasClosedHoldings is true when closed_holdings is non-empty (ACD-044)", () => {
    const summary = toAccountSummary(makeResponse({ closed_holdings: [makeClosedHolding()] }));
    expect(summary.hasClosedHoldings).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Market-price presenter stubs (MKT-NNN)
// All assertions below are intentionally failing — implement presenter.ts to fix.
// Types used: HoldingDetail, AccountDetailsResponse, HoldingRowViewModel, AccountSummaryViewModel
// ---------------------------------------------------------------------------

describe("toHoldingRow — market price fields (MKT)", () => {
  // MKT-010 — "Enter price" action available on active holding rows
  it("MKT-010 — canEnterPrice is true on active holding rows", () => {
    const row = toHoldingRow(makeHolding());
    expect(row.canEnterPrice).toBe(true);
  });

  // MKT-030 — current price column: formatted price with 2 decimals
  it("MKT-030 — currentPrice is formatted with 2 decimals when current_price is set", () => {
    const row = toHoldingRow(makeHolding({ current_price: 150_500_000 }));
    expect(row.currentPrice).toBe("150,50");
  });

  // MKT-030 — "—" sentinel when current_price is null
  it("MKT-030 — currentPrice is '—' when current_price is null", () => {
    const row = toHoldingRow(makeHolding({ current_price: null }));
    expect(row.currentPrice).toBe("—");
  });

  // MKT-030 — currentPriceDate passed through for "as of {date}" label; null when no price
  it("MKT-030 — currentPriceDate is the ISO date string when present, null otherwise", () => {
    const withDate = toHoldingRow(makeHolding({ current_price_date: "2026-04-25" }));
    expect(withDate.currentPriceDate).toBe("2026-04-25");
    const noDate = toHoldingRow(makeHolding({ current_price_date: null }));
    expect(noDate.currentPriceDate).toBeNull();
  });

  // MKT-032 — "—" in unrealized P&L column when unrealized_pnl is null
  it("MKT-032 — unrealizedPnl is '—' when unrealized_pnl is null", () => {
    const row = toHoldingRow(makeHolding({ unrealized_pnl: null }));
    expect(row.unrealizedPnl).toBe("—");
  });

  // MKT-032 — "—" in performance % column when performance_pct is null
  it("MKT-032 — performancePct is '—' when performance_pct is null", () => {
    const row = toHoldingRow(makeHolding({ performance_pct: null }));
    expect(row.performancePct).toBe("—");
  });

  // MKT-034 — currency mismatch: unrealized_pnl null but current_price non-null
  it("MKT-034 — unrealizedPnl is '—' and performancePct is '—' when unrealized_pnl is null but current_price is set", () => {
    const row = toHoldingRow(
      makeHolding({
        current_price: 110_000_000,
        unrealized_pnl: null,
        performance_pct: null,
      }),
    );
    expect(row.unrealizedPnl).toBe("—");
    expect(row.performancePct).toBe("—");
  });

  // MKT-034 — currentPrice still formatted on currency mismatch
  it("MKT-034 — currentPrice is formatted even when unrealized_pnl is null (currency mismatch)", () => {
    const row = toHoldingRow(makeHolding({ current_price: 110_000_000, unrealized_pnl: null }));
    expect(row.currentPrice).toBe("110,00");
  });
});

describe("toAccountSummary — market price fields (MKT)", () => {
  // MKT-041 — total_unrealized_pnl formatted with 2 decimals when present
  it("MKT-041 — totalUnrealizedPnl is formatted with 2 decimals when total_unrealized_pnl is set", () => {
    const summary = toAccountSummary(makeResponse({ total_unrealized_pnl: 20_000_000 }));
    expect(summary.totalUnrealizedPnl).toBe("20,00");
  });

  // MKT-041 — "—" when total_unrealized_pnl is null
  it("MKT-041 — totalUnrealizedPnl is '—' when total_unrealized_pnl is null", () => {
    const summary = toAccountSummary(makeResponse({ total_unrealized_pnl: null }));
    expect(summary.totalUnrealizedPnl).toBe("—");
  });
});

// ---------------------------------------------------------------------------
// Cash tracking — CSH-090/091/094/098
// ---------------------------------------------------------------------------

const makeCashHolding = (overrides: Partial<HoldingDetail> = {}): HoldingDetail =>
  makeHolding({
    asset_id: "system-cash-eur",
    asset_name: "Cash EUR",
    asset_reference: "EUR",
    quantity: 500_000_000,
    average_price: 1_000_000,
    cost_basis: 500_000_000,
    realized_pnl: 0,
    asset_currency: "EUR",
    current_price: null,
    current_price_date: null,
    unrealized_pnl: null,
    performance_pct: null,
    ...overrides,
  });

describe("toHoldingRow — cash variant (CSH-090/091)", () => {
  // CSH-090 — system Cash Asset id starts with "system-cash-"; presenter detects via prefix
  it("flags isCash when asset_id starts with system-cash-", () => {
    const row = toHoldingRow(makeCashHolding());
    expect(row.isCash).toBe(true);
  });

  it("non-cash holdings have isCash false", () => {
    const row = toHoldingRow(makeHolding());
    expect(row.isCash).toBe(false);
  });

  // CSH-091 — cash row has no cost basis / average price / realized P&L cells
  it("cash row leaves averagePrice / costBasis / realizedPnl blank", () => {
    const row = toHoldingRow(makeCashHolding());
    expect(row.averagePrice).toBe("");
    expect(row.costBasis).toBe("");
    expect(row.realizedPnl).toBe("");
  });

  // CSH-091 — cash row has no Buy/Sell/Inspect actions, no price entry
  it("cash row disables canEnterPrice and clears market-price cells", () => {
    const row = toHoldingRow(makeCashHolding());
    expect(row.canEnterPrice).toBe(false);
    expect(row.currentPrice).toBe("");
    expect(row.unrealizedPnl).toBe("");
    expect(row.performancePct).toBe("");
  });

  // CSH-090 — quantity rendered as a 2-decimal amount (currency), not 6-decimal qty
  it("formats cash quantity with 2 decimals", () => {
    const row = toHoldingRow(makeCashHolding({ quantity: 250_500_000 }));
    expect(row.quantity).toBe("250,50");
  });
});

describe("toAccountSummary — cash totals (CSH-094/098)", () => {
  // CSH-094 — totalGlobalValue passed through, formatted with 2 decimals
  it("formats totalGlobalValue with 2 decimals", () => {
    const summary = toAccountSummary(makeResponse({ total_global_value: 250_000_000 }));
    expect(summary.totalGlobalValue).toBe("250,00");
    expect(summary.totalGlobalValueRaw).toBe(250_000_000);
  });

  // CSH-019/095 — hasCashHolding reflects presence of a cash holding with quantity > 0
  it("hasCashHolding true when holdings include a non-zero cash row", () => {
    const summary = toAccountSummary(
      makeResponse({
        holdings: [makeHolding(), makeCashHolding()],
        total_holding_count: 2,
      }),
    );
    expect(summary.hasCashHolding).toBe(true);
  });

  it("hasCashHolding false when no cash holding present", () => {
    const summary = toAccountSummary(makeResponse());
    expect(summary.hasCashHolding).toBe(false);
  });

  // CSH-098 — cash row excluded from isEmpty / isAllClosed gating
  it("isEmpty true when only the cash holding is active and no closed holdings", () => {
    const summary = toAccountSummary(
      makeResponse({
        holdings: [makeCashHolding()],
        total_holding_count: 0,
        closed_holdings: [],
      }),
    );
    expect(summary.isEmpty).toBe(true);
  });

  it("isAllClosed true when only cash is active but closed holdings exist", () => {
    const summary = toAccountSummary(
      makeResponse({
        holdings: [makeCashHolding()],
        total_holding_count: 2,
        closed_holdings: [makeClosedHolding()],
      }),
    );
    expect(summary.isAllClosed).toBe(true);
    expect(summary.isEmpty).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// formatStaleness — pure helper (MKT-140)
// ---------------------------------------------------------------------------

describe("formatStaleness", () => {
  const today = new Date("2026-05-17");

  // MKT-140 — no date → null (caller renders no label)
  it("returns null when currentPriceDate is null", () => {
    expect(formatStaleness(null, today)).toBeNull();
  });

  // MKT-140 — same day → today i18n key
  it("returns the today i18n key when currentPriceDate equals today", () => {
    expect(formatStaleness("2026-05-17", today)).toEqual({ key: "mkt.staleness_today" });
  });

  // MKT-140 — one day ago → days_ago i18n key with days=1
  it("returns the days_ago i18n key with days=1 when currentPriceDate is one day before today", () => {
    expect(formatStaleness("2026-05-16", today)).toEqual({
      key: "mkt.staleness_days_ago",
      params: { days: 1 },
    });
  });

  // MKT-140 — multiple days ago → days_ago i18n key with days=N
  it("returns the days_ago i18n key with days=7 when currentPriceDate is seven days before today", () => {
    expect(formatStaleness("2026-05-10", today)).toEqual({
      key: "mkt.staleness_days_ago",
      params: { days: 7 },
    });
  });

  // MKT-140 — large delta (e.g. stale data after holiday)
  it("returns the days_ago i18n key with days=30 when currentPriceDate is thirty days before today", () => {
    expect(formatStaleness("2026-04-17", today)).toEqual({
      key: "mkt.staleness_days_ago",
      params: { days: 30 },
    });
  });
});

// ---------------------------------------------------------------------------
// formatSource — pure helper (MKT-141, MKT-142)
// ---------------------------------------------------------------------------

describe("formatSource", () => {
  // MKT-142 — null source (no price recorded) → null
  it("returns null when source is null", () => {
    expect(formatSource(null)).toBeNull();
  });

  // MKT-101 — Manual source → i18n key
  it("returns mkt.source_manual for Manual source", () => {
    const source: AssetPriceSource = "Manual";
    expect(formatSource(source)).toBe("mkt.source_manual");
  });

  // MKT-102 — Stooq source → i18n key
  it("returns mkt.source_stooq for Stooq source", () => {
    const source: AssetPriceSource = "Stooq";
    expect(formatSource(source)).toBe("mkt.source_stooq");
  });
});

// ---------------------------------------------------------------------------
// toHoldingRow — staleness + source label derived fields (MKT-140, MKT-142)
// ---------------------------------------------------------------------------

describe("toHoldingRow — staleness and sourceLabel fields (MKT-140, MKT-142)", () => {
  // MKT-140 — staleness field: no price → null
  it("staleness is null when current_price_date is null", () => {
    const row = toHoldingRow(makeHolding({ current_price_date: null }));
    expect(row.staleness).toBeNull();
  });

  // MKT-142 — sourceLabel field: no price → null
  it("sourceLabel is null when current_price_source is null", () => {
    const row = toHoldingRow(makeHolding({ current_price_source: null }));
    expect(row.sourceLabel).toBeNull();
  });

  // MKT-142 — sourceLabel: Manual source → i18n key
  it("sourceLabel is mkt.source_manual when current_price_source is Manual", () => {
    const row = toHoldingRow(
      makeHolding({ current_price_source: "Manual", current_price: 100_000_000 }),
    );
    expect(row.sourceLabel).toBe("mkt.source_manual");
  });

  // MKT-142 — sourceLabel: Stooq source → i18n key
  it("sourceLabel is mkt.source_stooq when current_price_source is Stooq", () => {
    const row = toHoldingRow(
      makeHolding({ current_price_source: "Stooq", current_price: 100_000_000 }),
    );
    expect(row.sourceLabel).toBe("mkt.source_stooq");
  });
});
