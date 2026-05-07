import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Account, Asset, Transaction } from "@/bindings";
import { useAppStore } from "@/lib/store";
import { useEditTransactionModal } from "./useEditTransactionModal";

const { mockCorrectTransaction, mockRecordAssetPrice } = vi.hoisted(() => ({
  mockCorrectTransaction: vi.fn(),
  mockRecordAssetPrice: vi.fn(),
}));

vi.mock("../useTransactions", () => ({
  useTransactions: () => ({
    buyHolding: vi.fn(),
    sellHolding: vi.fn(),
    correctTransaction: mockCorrectTransaction,
    cancelTransaction: vi.fn(),
    getTransactions: vi.fn(),
  }),
}));

vi.mock("../gateway", () => ({
  transactionGateway: {
    recordAssetPrice: mockRecordAssetPrice,
  },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "fr" },
  }),
}));

const MICRO = 1_000_000;

// 2 units @ 50.0 each, rate=1.0, fees=0 → total=100_000_000
const baseTransaction: Transaction = {
  id: "tx-existing",
  account_id: "account-1",
  asset_id: "asset-1",
  transaction_type: "Purchase",
  date: "2024-01-10",
  quantity: 2 * MICRO,
  unit_price: 50 * MICRO,
  exchange_rate: 1 * MICRO,
  fees: 0,
  total_amount: 100 * MICRO,
  note: "initial note",
  realized_pnl: null,
  created_at: "2024-01-10T00:00:00Z",
};

// unit_price=0, fees=5 → totalMicro=5*MICRO > 0, passes validation; used for MKT-061
const zeroUnitPriceTransaction: Transaction = {
  ...baseTransaction,
  id: "tx-zero-price",
  unit_price: 0,
  fees: 5 * MICRO,
  total_amount: 5 * MICRO,
};

// TRX-051: 2 units @ total_cost=100 → stored unit_price=50 (computed by backend, TRX-047)
const openingBalanceTransaction: Transaction = {
  ...baseTransaction,
  id: "tx-ob",
  transaction_type: "OpeningBalance",
  unit_price: 50 * MICRO,
  total_amount: 100 * MICRO,
  fees: 0,
  note: null,
};

describe("useEditTransactionModal", () => {
  beforeEach(() => {
    localStorage.clear();
    mockCorrectTransaction.mockReset();
    mockRecordAssetPrice.mockReset();
    useAppStore.setState({
      assets: [
        { id: "asset-1", name: "Apple", is_archived: false, currency: "USD" },
        { id: "asset-archived", name: "OldCo", is_archived: true, currency: "USD" },
      ] as Asset[],
      accounts: [{ id: "account-1", name: "My Account" }] as Account[],
    });
  });

  // Pre-fill: micro-unit values are converted to decimal strings; totalAmount is derived
  it("pre-fills formData from transaction (micro → decimal)", () => {
    const { result } = renderHook(() => useEditTransactionModal({ transaction: baseTransaction }));
    expect(result.current.formData.quantity).toBe("2.000");
    expect(result.current.formData.unitPrice).toBe("50.000");
    expect(result.current.formData.exchangeRate).toBe("1.000");
    expect(result.current.formData.note).toBe("initial note");
    // totalAmount is derived from micro values, not stored in formData
    expect(result.current.totalAmountDisplay).toBe("100,000");
  });

  // Submit calls correctTransaction with correct args: (id, accountId, dto)
  it("calls correctTransaction with correct args on submit", async () => {
    mockCorrectTransaction.mockResolvedValue({
      data: { id: "tx-existing" },
      error: null,
    });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() =>
      useEditTransactionModal({
        transaction: baseTransaction,
        onSubmitSuccess,
      }),
    );

    const fakeSubmit = {
      preventDefault: vi.fn(),
    } as unknown as React.FormEvent;

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockCorrectTransaction).toHaveBeenCalledWith(
      "tx-existing",
      "account-1",
      expect.objectContaining({
        quantity: 2 * MICRO,
        unit_price: 50 * MICRO,
        exchange_rate: 1 * MICRO,
      }),
    );
    expect(onSubmitSuccess).toHaveBeenCalledTimes(1);
  });

  // Backend error stays modal open
  it("sets error and does not call onSubmitSuccess on backend error", async () => {
    mockCorrectTransaction.mockResolvedValue({
      data: null,
      error: "Not found",
    });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() =>
      useEditTransactionModal({
        transaction: baseTransaction,
        onSubmitSuccess,
      }),
    );

    const fakeSubmit = {
      preventDefault: vi.fn(),
    } as unknown as React.FormEvent;
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toBe("Not found");
    expect(onSubmitSuccess).not.toHaveBeenCalled();
  });

  // MKT-052 — recordPrice is always false on edit regardless of localStorage
  it("recordPrice is false on edit mount even when localStorage auto_record_price is true", () => {
    localStorage.setItem("auto_record_price", "true");
    const { result } = renderHook(() => useEditTransactionModal({ transaction: baseTransaction }));
    expect(result.current.recordPrice).toBe(false);
  });

  // MKT-054 — calls recordAssetPrice when recordPrice is manually set to true
  it("calls recordAssetPrice when recordPrice is manually set to true", async () => {
    mockCorrectTransaction.mockResolvedValue({
      data: { id: "tx-existing" },
      error: null,
    });
    mockRecordAssetPrice.mockResolvedValue({ status: "ok", data: null });

    const { result } = renderHook(() => useEditTransactionModal({ transaction: baseTransaction }));

    await act(async () => {
      result.current.setRecordPrice(true);
    });

    const fakeSubmit = {
      preventDefault: vi.fn(),
    } as unknown as React.FormEvent;
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockRecordAssetPrice).toHaveBeenCalledWith("asset-1", "2024-01-10", 50);
  });

  // MKT-061 — skip recordAssetPrice when recordPrice is true but unit_price is 0
  it("does not call recordAssetPrice when recordPrice is true but unit_price is 0", async () => {
    mockCorrectTransaction.mockResolvedValue({
      data: { id: "tx-zero-price" },
      error: null,
    });

    const { result } = renderHook(() =>
      useEditTransactionModal({ transaction: zeroUnitPriceTransaction }),
    );

    await act(async () => {
      result.current.setRecordPrice(true);
    });

    const fakeSubmit = {
      preventDefault: vi.fn(),
    } as unknown as React.FormEvent;
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockCorrectTransaction).toHaveBeenCalledWith(
      "tx-zero-price",
      "account-1",
      expect.objectContaining({ unit_price: 0 }),
    );
    expect(mockRecordAssetPrice).not.toHaveBeenCalled();
  });

  // TRX-051: OpeningBalance pre-fill uses total_amount (not unit_price) as the "Total Cost" field
  it("TRX-051: pre-fills unitPrice from total_amount for OpeningBalance", () => {
    const { result } = renderHook(() =>
      useEditTransactionModal({ transaction: openingBalanceTransaction }),
    );
    expect(result.current.formData.unitPrice).toBe("100.000");
  });

  // TRX-051: submit recomputes unit_price = round(total_cost * 1M / quantity); fees=0, rate=1M, note=null
  it("TRX-051: submit sends computed unit_price, zero fees, unit exchange_rate, null note", async () => {
    mockCorrectTransaction.mockResolvedValue({
      data: { id: "tx-ob" },
      error: null,
    });
    const { result } = renderHook(() =>
      useEditTransactionModal({ transaction: openingBalanceTransaction }),
    );

    const fakeSubmit = {
      preventDefault: vi.fn(),
    } as unknown as React.FormEvent;
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockCorrectTransaction).toHaveBeenCalledWith(
      "tx-ob",
      "account-1",
      expect.objectContaining({
        unit_price: 50 * MICRO, // round(100M * 1M / 2M) = 50M
        exchange_rate: 1 * MICRO,
        fees: 0,
        note: null,
      }),
    );
  });

  // MKT-062 — recordAssetPrice failure is silent; edit commits and onSubmitSuccess fires
  it("swallows recordAssetPrice rejection and still calls onSubmitSuccess", async () => {
    mockCorrectTransaction.mockResolvedValue({
      data: { id: "tx-existing" },
      error: null,
    });
    mockRecordAssetPrice.mockRejectedValue(new Error("network error"));

    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() =>
      useEditTransactionModal({
        transaction: baseTransaction,
        onSubmitSuccess,
      }),
    );

    await act(async () => {
      result.current.setRecordPrice(true);
    });

    const fakeSubmit = {
      preventDefault: vi.fn(),
    } as unknown as React.FormEvent;
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toBeNull();
    expect(onSubmitSuccess).toHaveBeenCalledTimes(1);
  });
});
