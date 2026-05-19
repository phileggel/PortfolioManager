import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset, AssetLookupResult, Exchange } from "@/bindings";
import { DEFAULT_RISK_BY_CLASS, SYSTEM_CATEGORY_ID } from "../shared/constants";
import { useAddAsset } from "./useAddAsset";

const mockAddAsset = vi.fn();

vi.mock("../useAssets", () => ({
  useAssets: () => ({
    addAsset: mockAddAsset,
    assets: [] as Asset[],
    activeCount: 0,
    loading: false,
    fetchError: null,
    fetchAssets: vi.fn(),
    updateAsset: vi.fn(),
    archiveAsset: vi.fn(),
    unarchiveAsset: vi.fn(),
    deleteAsset: vi.fn(),
  }),
}));

vi.mock("@/features/categories/useCategories", () => ({
  useCategories: () => ({
    categories: [{ id: "cat-1", name: "Bonds", is_system: false }],
    loading: false,
  }),
}));

const fakeSubmit = { preventDefault: vi.fn() } as unknown as React.FormEvent;

describe("useAddAsset", () => {
  beforeEach(() => {
    mockAddAsset.mockReset();
  });

  // R2 — category pre-selected on SYSTEM_CATEGORY_ID
  it("pre-selects SYSTEM_CATEGORY_ID as default category", () => {
    const { result } = renderHook(() => useAddAsset());
    expect(result.current.formData.category_id).toBe(SYSTEM_CATEGORY_ID);
  });

  // R10 — risk_level auto-filled when class changes
  it("auto-fills risk_level when class changes", () => {
    const { result } = renderHook(() => useAddAsset());

    act(() => {
      result.current.handleClassChange("DigitalAsset");
    });

    expect(result.current.formData.class).toBe("DigitalAsset");
    expect(result.current.formData.risk_level).toBe(5);
  });

  // R10 — risk default for Stocks is 4
  it("sets risk_level to 4 when class is Stocks", () => {
    const { result } = renderHook(() => useAddAsset());

    act(() => {
      result.current.handleClassChange("Stocks");
    });

    expect(result.current.formData.risk_level).toBe(4);
  });

  // R9 — hasDuplicateReference is tested exhaustively in validateAsset.test.ts
  // Here we verify the hook integrates it without crashing (assets mock is [] in this scope)
  it("exposes duplicateWarning computed from assets", () => {
    const { result } = renderHook(() => useAddAsset());

    act(() => {
      result.current.handleChange({
        target: { name: "reference", value: "AAPL" },
      } as React.ChangeEvent<HTMLInputElement>);
    });

    expect(typeof result.current.duplicateWarning).toBe("boolean");
  });

  // R14 — modal stays open on backend error, exposes error message
  it("does not call onSubmitSuccess and exposes error on backend failure", async () => {
    mockAddAsset.mockResolvedValue({ data: null, error: "Backend error" });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() => useAddAsset({ onSubmitSuccess }));

    act(() => {
      result.current.handleChange({
        target: { name: "name", value: "Apple" },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleChange({
        target: { name: "reference", value: "AAPL" },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleChange({
        target: { name: "currency", value: "USD" },
      } as React.ChangeEvent<HTMLInputElement>);
    });

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toBe("Backend error");
    expect(onSubmitSuccess).not.toHaveBeenCalled();
  });

  // R14 — success closes modal
  it("calls onSubmitSuccess and clears error on success", async () => {
    mockAddAsset.mockResolvedValue({ data: { id: "new" }, error: null });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() => useAddAsset({ onSubmitSuccess }));

    act(() => {
      result.current.handleChange({
        target: { name: "name", value: "Apple" },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleChange({
        target: { name: "reference", value: "AAPL" },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleChange({
        target: { name: "currency", value: "USD" },
      } as React.ChangeEvent<HTMLInputElement>);
    });

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toBeNull();
    expect(onSubmitSuccess).toHaveBeenCalledTimes(1);
  });

  // WEB-041 — prefill seeds name, reference, currency, and asset class from AssetLookupResult
  it("seeds name, reference, currency, and class from prefill prop", () => {
    const prefill: AssetLookupResult = {
      name: "Apple Inc.",
      reference: "AAPL",
      currency: "USD",
      asset_class: "Stocks",
      exchange: null,
    };

    const { result } = renderHook(() => useAddAsset({ prefill }));

    expect(result.current.formData.name).toBe("Apple Inc.");
    expect(result.current.formData.reference).toBe("AAPL");
    expect(result.current.formData.currency).toBe("USD");
    expect(result.current.formData.class).toBe("Stocks");
  });

  // WEB-042 — prefilling class auto-sets risk_level via DEFAULT_RISK_BY_CLASS
  it("auto-sets risk_level to class default when class is prefilled", () => {
    const prefill: AssetLookupResult = {
      name: "iShares Core S&P 500",
      reference: "IVV",
      currency: "USD",
      asset_class: "ETF",
      exchange: null,
    };

    const { result } = renderHook(() => useAddAsset({ prefill }));

    expect(result.current.formData.class).toBe("ETF");
    expect(result.current.formData.risk_level).toBe(DEFAULT_RISK_BY_CLASS.ETF);
  });

  // WEB-042 — when asset_class is absent in prefill, risk_level stays at form default
  it("leaves risk_level at form default when prefill has no asset_class", () => {
    const prefill: AssetLookupResult = {
      name: "Obscure Fund",
      reference: null,
      currency: null,
      asset_class: null,
      exchange: null,
    };

    const { result } = renderHook(() => useAddAsset({ prefill }));

    // risk_level should remain the Stocks default (initial form default — CSH-015 made
    // Cash unavailable as a user-pickable class).
    expect(result.current.formData.risk_level).toBe(DEFAULT_RISK_BY_CLASS.Stocks);
  });

  // WEB-043 — all prefilled fields remain editable
  it("allows editing prefilled name after prefill", () => {
    const prefill: AssetLookupResult = {
      name: "Apple Inc.",
      reference: "AAPL",
      currency: "USD",
      asset_class: "Stocks",
      exchange: null,
    };

    const { result } = renderHook(() => useAddAsset({ prefill }));

    act(() => {
      result.current.handleChange({
        target: { name: "name", value: "Apple Edited" },
      } as React.ChangeEvent<HTMLInputElement>);
    });

    expect(result.current.formData.name).toBe("Apple Edited");
  });

  // WEB-043 — prefilled class remains changeable via handleClassChange
  it("allows changing prefilled class via handleClassChange", () => {
    const prefill: AssetLookupResult = {
      name: "Apple Inc.",
      reference: "AAPL",
      currency: "USD",
      asset_class: "Stocks",
      exchange: null,
    };

    const { result } = renderHook(() => useAddAsset({ prefill }));

    act(() => {
      result.current.handleClassChange("ETF");
    });

    expect(result.current.formData.class).toBe("ETF");
  });

  // WEB-044 — category_id always defaults to SYSTEM_CATEGORY_ID even when prefill is provided
  it("always defaults category_id to SYSTEM_CATEGORY_ID regardless of prefill", () => {
    const prefill: AssetLookupResult = {
      name: "Apple Inc.",
      reference: "AAPL",
      currency: "USD",
      asset_class: "Stocks",
      exchange: null,
    };
    const { result } = renderHook(() => useAddAsset({ prefill }));
    expect(result.current.formData.category_id).toBe(SYSTEM_CATEGORY_ID);
  });

  // Regression — no prefill defaults to Stocks / EUR / risk 4 (CSH-015 — Cash
  // is not user-selectable, so the form defaults to the first dropdown entry).
  it("uses form defaults when no prefill is provided", () => {
    const { result } = renderHook(() => useAddAsset());

    expect(result.current.formData.name).toBe("");
    expect(result.current.formData.reference).toBe("");
    expect(result.current.formData.currency).toBe("EUR");
    expect(result.current.formData.class).toBe("Stocks");
    expect(result.current.formData.risk_level).toBe(DEFAULT_RISK_BY_CLASS.Stocks);
    expect(result.current.formData.category_id).toBe(SYSTEM_CATEGORY_ID);
  });

  // AST-021 — exchange defaults to null when no prefill
  it("initialises exchange as null when no prefill is provided", () => {
    const { result } = renderHook(() => useAddAsset());
    expect(result.current.formData.exchange).toBeNull();
  });

  // WEB-041 — prefill.exchange seeds the exchange picker (AST-021)
  it("seeds exchange from prefill when prefill carries an exchange", () => {
    const exchange: Exchange = { code: "XPAR", label: "Euronext Paris" };
    const prefill: AssetLookupResult = {
      name: "Total SE",
      reference: "TTE",
      currency: "EUR",
      asset_class: "Stocks",
      exchange,
    };
    const { result } = renderHook(() => useAddAsset({ prefill }));
    expect(result.current.formData.exchange).toEqual(exchange);
  });

  // WEB-041 — prefill with null exchange keeps form exchange null
  it("leaves exchange null when prefill exchange is null", () => {
    const prefill: AssetLookupResult = {
      name: "No Exchange Fund",
      reference: "NEF",
      currency: "EUR",
      asset_class: "ETF",
      exchange: null,
    };
    const { result } = renderHook(() => useAddAsset({ prefill }));
    expect(result.current.formData.exchange).toBeNull();
  });

  // AST-021 — exchange is forwarded to the gateway on submit
  it("forwards formData.exchange to the gateway call on submit", async () => {
    const exchange: Exchange = { code: "XNAS", label: "NASDAQ" };
    mockAddAsset.mockResolvedValue({ data: { id: "new-1" }, error: null });

    const { result } = renderHook(() => useAddAsset());

    act(() => {
      result.current.handleChange({
        target: { name: "name", value: "Apple Inc." },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleChange({
        target: { name: "reference", value: "AAPL" },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleExchangeChange(exchange);
    });

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockAddAsset).toHaveBeenCalledWith(expect.objectContaining({ exchange }));
  });

  // AST-021 — exchange resets to null after successful submit
  it("resets exchange to null after successful submit", async () => {
    const exchange: Exchange = { code: "XNAS", label: "NASDAQ" };
    mockAddAsset.mockResolvedValue({ data: { id: "new-2" }, error: null });

    const { result } = renderHook(() => useAddAsset());

    act(() => {
      result.current.handleChange({
        target: { name: "name", value: "Apple Inc." },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleChange({
        target: { name: "reference", value: "AAPL" },
      } as React.ChangeEvent<HTMLInputElement>);
      result.current.handleExchangeChange(exchange);
    });

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.formData.exchange).toBeNull();
  });
});
