import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountDetailsResponse, ClosedHoldingDetail, HoldingDetail } from "@/bindings";
import { useAccountDetails } from "./useAccountDetails";

const mockGetAccountDetails = vi.fn();

vi.mock("../gateway", () => ({
  accountDetailsGateway: {
    getAccountDetails: (...args: unknown[]) => mockGetAccountDetails(...args),
    subscribeToEvents: vi.fn(() => Promise.resolve(() => {})),
  },
}));

vi.mock("@/lib/logger", () => ({
  logger: { error: vi.fn(), info: vi.fn() },
}));

const makeClosedHolding = (overrides: Partial<ClosedHoldingDetail> = {}): ClosedHoldingDetail => ({
  asset_id: "closed-1",
  asset_name: "Closed Corp",
  asset_reference: "CLSD",
  realized_pnl: 5_000_000,
  last_sold_date: "2025-12-01",
  ...overrides,
});

const makeResponse = (overrides: Partial<AccountDetailsResponse> = {}): AccountDetailsResponse => ({
  account_name: "My Portfolio",
  holdings: [],
  closed_holdings: [],
  total_holding_count: 0,
  total_cost_basis: 0,
  total_realized_pnl: 0,
  total_unrealized_pnl: null,
  total_global_value: 0,
  ...overrides,
});

describe("useAccountDetails — closed holdings (ACD-044–ACD-050)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ACD-044 — closedHoldings array populated when response contains closed_holdings
  it("returns closedHoldings mapped from response.closed_holdings (ACD-044)", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({
        closed_holdings: [makeClosedHolding()],
      }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.closedHoldings).toHaveLength(1);
    expect(result.current.closedHoldings.at(0)?.assetId).toBe("closed-1");
    expect(result.current.closedHoldings.at(0)?.assetName).toBe("Closed Corp");
  });

  // ACD-046 — closed holdings order preserved from backend (already sorted by asset_name asc)
  it("preserves closed_holdings order from backend response (ACD-046)", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({
        closed_holdings: [
          makeClosedHolding({ asset_id: "z", asset_name: "Zebra" }),
          makeClosedHolding({ asset_id: "a", asset_name: "Alpha" }),
        ],
      }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.closedHoldings.at(0)?.assetName).toBe("Zebra");
    expect(result.current.closedHoldings.at(1)?.assetName).toBe("Alpha");
  });

  // ACD-047 — summary.totalRealizedPnl reflects combined active + closed pnl from backend
  it("summary totalRealizedPnl reflects total_realized_pnl from response (ACD-047)", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({ total_realized_pnl: 35_000_000 }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.summary?.totalRealizedPnl).toBe("35,00");
    expect(result.current.summary?.totalRealizedPnlRaw).toBe(35_000_000);
  });

  // ACD-050 — closedHoldings is empty array when response.closed_holdings is empty
  it("returns empty closedHoldings when response.closed_holdings is empty (ACD-050)", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({ closed_holdings: [] }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.closedHoldings).toEqual([]);
  });

  // ACD-048 — summary.hasClosedHoldings is true when closed_holdings is non-empty
  it("summary hasClosedHoldings true when closed_holdings non-empty (ACD-048)", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({ closed_holdings: [makeClosedHolding()] }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.summary?.hasClosedHoldings).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Market-price hook stubs (MKT-NNN)
// Assertion below is intentionally failing — implement useAccountDetails.ts to fix.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Cash tracking — CSH-019 / CSH-092 / CSH-095
// ---------------------------------------------------------------------------

const makeHolding = (overrides: Partial<HoldingDetail> = {}): HoldingDetail => ({
  asset_id: "asset-1",
  asset_name: "Apple",
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

const makeCashHolding = (overrides: Partial<HoldingDetail> = {}): HoldingDetail =>
  makeHolding({
    asset_id: "system-cash-eur",
    asset_name: "Cash EUR",
    asset_reference: "EUR",
    quantity: 500_000_000,
    average_price: 1_000_000,
    cost_basis: 500_000_000,
    realized_pnl: 0,
    ...overrides,
  });

describe("useAccountDetails — cash row (CSH-092 / CSH-019 / CSH-095)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // CSH-092 — Cash row sorts to top of the active holdings list
  it("sorts cash row to the top of holdings (CSH-092)", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({
        holdings: [
          makeHolding({ asset_id: "z" }),
          makeCashHolding(),
          makeHolding({ asset_id: "a" }),
        ],
        total_holding_count: 3,
      }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.holdings.at(0)?.isCash).toBe(true);
    expect(result.current.holdings.at(0)?.assetId).toBe("system-cash-eur");
  });

  // CSH-019/095 — hasVisibleCashRow is true when the response contains a cash holding
  it("hasVisibleCashRow true when response contains a cash holding", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({
        holdings: [makeCashHolding()],
        total_holding_count: 1,
      }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.hasVisibleCashRow).toBe(true);
  });

  // CSH-095 — hasVisibleCashRow is false when no cash holding present
  it("hasVisibleCashRow false when response has no cash holding", async () => {
    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse({ holdings: [makeHolding()], total_holding_count: 1 }),
    });
    const { result } = renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});
    expect(result.current.hasVisibleCashRow).toBe(false);
  });
});

describe("useAccountDetails — market price events (MKT)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // MKT-036 — useAccountDetails re-fetches on AssetPriceUpdated event
  it("MKT-036 — re-fetches when AssetPriceUpdated event is received", async () => {
    // subscribeToEvents captures the callback so the test can fire it
    let capturedCallback: ((type: string) => void) | null = null;
    const mockSubscribe = vi.fn((cb: (type: string) => void) => {
      capturedCallback = cb;
      return Promise.resolve(() => {});
    });

    // Override the module-level mock for this test
    const { accountDetailsGateway } = await import("../gateway");
    (accountDetailsGateway.subscribeToEvents as ReturnType<typeof vi.fn>).mockImplementation(
      mockSubscribe,
    );

    mockGetAccountDetails.mockResolvedValue({
      status: "ok",
      data: makeResponse(),
    });

    renderHook(() => useAccountDetails("account-1"));
    await act(async () => {});

    const firstCallCount = mockGetAccountDetails.mock.calls.length;

    // Fire the AssetPriceUpdated event
    await act(async () => {
      capturedCallback?.("AssetPriceUpdated");
    });

    expect(mockGetAccountDetails.mock.calls.length).toBeGreaterThan(firstCallCount);
  });
});
