import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HoldingDetail } from "@/bindings";

// Gateway mock — recordAssetPrice is the new command (Result<null, string>)
const mockRecordAssetPrice = vi.fn();

vi.mock("../gateway", () => ({
  accountDetailsGateway: {
    getAccountDetails: vi.fn(),
    subscribeToEvents: vi.fn(() => Promise.resolve(() => {})),
    recordAssetPrice: (...args: unknown[]) => mockRecordAssetPrice(...args),
  },
}));

vi.mock("@/lib/logger", () => ({
  logger: { error: vi.fn(), info: vi.fn() },
}));

vi.mock("@/lib/snackbarStore", () => ({
  useSnackbar: () => vi.fn(),
}));

// today's ISO date — used in MKT-011/012 assertions
const TODAY = new Date().toISOString().slice(0, 10);

// Minimal HoldingDetail fixture matching the full type from bindings.ts
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

// usePriceModal hook — to be implemented in
// src/features/account_details/account_details_view/usePriceModal.ts
// Props: { holding: HoldingDetail; onSubmitSuccess: () => void }
// Returns: { date, price, error, isSubmitting, isFormValid, handleChange, handleSubmit }
import { usePriceModal } from "./usePriceModal";

describe("usePriceModal", () => {
  const fakeSubmit = { preventDefault: vi.fn() } as unknown as React.FormEvent;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  // MKT-011 — Modal pre-fills asset name (read-only) and today's date (editable)
  it("MKT-011 — initialises date to today", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    expect(result.current.date).toBe(TODAY);
  });

  // MKT-012 — Price field pre-filled when current_price_date == today, empty otherwise
  it("MKT-012 — pre-fills price when current_price_date is today", () => {
    const holding = makeHolding({
      current_price: 150_000_000,
      current_price_date: TODAY,
    });
    const { result } = renderHook(() => usePriceModal({ holding, onSubmitSuccess: vi.fn() }));
    expect(result.current.price).toBe("150.00");
  });

  // MKT-012 — price is empty when current_price_date is not today
  it("MKT-012 — price is empty when current_price_date is not today", () => {
    const holding = makeHolding({
      current_price: 150_000_000,
      current_price_date: "2024-01-01",
    });
    const { result } = renderHook(() => usePriceModal({ holding, onSubmitSuccess: vi.fn() }));
    expect(result.current.price).toBe("");
  });

  // MKT-013 — No extra IPC call when opening the modal (data comes from HoldingDetail prop)
  it("MKT-013 — no gateway call on mount", () => {
    renderHook(() => usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }));
    expect(mockRecordAssetPrice).not.toHaveBeenCalled();
  });

  // MKT-020 — Submit disabled while date or price is empty
  it("MKT-020 — isFormValid is false when price is empty", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    // price starts as "" (no pre-fill), date is today → not valid
    expect(result.current.isFormValid).toBe(false);
  });

  // MKT-020 — Submit disabled while date is empty
  it("MKT-020 — isFormValid is false when date is empty", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    act(() => {
      result.current.handleChange("date", "");
    });
    expect(result.current.isFormValid).toBe(false);
  });

  // MKT-021 — Inline error + submit disabled for price ≤ 0 (frontend validation)
  it("MKT-021 — isFormValid is false and error set when price is zero", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    act(() => {
      result.current.handleChange("price", "0");
    });
    expect(result.current.isFormValid).toBe(false);
    expect(result.current.error).toBe("price_modal.error_price_not_positive");
  });

  // MKT-021 — negative price also invalid
  it("MKT-021 — isFormValid is false and error set when price is negative", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    act(() => {
      result.current.handleChange("price", "-10");
    });
    expect(result.current.isFormValid).toBe(false);
    expect(result.current.error).toBe("price_modal.error_price_not_positive");
  });

  // MKT-022 — Inline error + submit disabled for future date
  it("MKT-022 — isFormValid is false and error set for a future date", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    act(() => {
      result.current.handleChange("date", "2099-12-31");
      result.current.handleChange("price", "100");
    });
    expect(result.current.isFormValid).toBe(false);
    expect(result.current.error).toBe("price_modal.error_future_date");
  });

  // MKT-022 — Inline error + submit disabled for malformed date
  it("MKT-022 — isFormValid is false and error set for malformed date string", () => {
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    act(() => {
      result.current.handleChange("date", "not-a-date");
      result.current.handleChange("price", "100");
    });
    expect(result.current.isFormValid).toBe(false);
    expect(result.current.error).toBe("price_modal.error_invalid_date");
  });

  // MKT-027 — Submit button disabled + spinner while in-flight (isSubmitting true during call)
  it("MKT-027 — isSubmitting is true while gateway call is pending", async () => {
    let resolveCall!: () => void;
    mockRecordAssetPrice.mockReturnValue(
      new Promise<{ status: string; data: null }>((resolve) => {
        resolveCall = () => resolve({ status: "ok", data: null });
      }),
    );
    const { result } = renderHook(() =>
      usePriceModal({ holding: makeHolding(), onSubmitSuccess: vi.fn() }),
    );
    act(() => {
      result.current.handleChange("price", "100");
    });

    let submitPromise: Promise<void>;
    act(() => {
      submitPromise = result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.isSubmitting).toBe(true);
    await act(async () => {
      resolveCall();
      await submitPromise;
    });
    expect(result.current.isSubmitting).toBe(false);
  });

  // MKT-028 — Modal closes on success + snackbar shown (onSubmitSuccess called)
  it("MKT-028 — calls onSubmitSuccess on successful recordAssetPrice", async () => {
    mockRecordAssetPrice.mockResolvedValue({ status: "ok", data: null });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() => usePriceModal({ holding: makeHolding(), onSubmitSuccess }));

    await act(async () => {
      result.current.handleChange("date", TODAY);
      result.current.handleChange("price", "150.50");
    });

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(onSubmitSuccess).toHaveBeenCalledOnce();
  });

  // MKT-029 — Modal stays open on error + inline error shown
  it("MKT-029 — sets inline error and does not call onSubmitSuccess on backend error", async () => {
    mockRecordAssetPrice.mockResolvedValue({
      status: "error",
      error: { code: "NotPositive" },
    });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() => usePriceModal({ holding: makeHolding(), onSubmitSuccess }));

    await act(async () => {
      result.current.handleChange("date", TODAY);
      result.current.handleChange("price", "150.50");
    });

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(onSubmitSuccess).not.toHaveBeenCalled();
    expect(result.current.error).toBe("error.NotPositive");
  });
});
