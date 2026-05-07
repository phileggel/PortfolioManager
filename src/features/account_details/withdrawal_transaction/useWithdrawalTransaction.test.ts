import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { logger } from "@/lib/logger";
import { useWithdrawalTransaction } from "./useWithdrawalTransaction";

const { mockRecordWithdrawal, mockShowSnackbar } = vi.hoisted(() => ({
  mockRecordWithdrawal: vi.fn(),
  mockShowSnackbar: vi.fn(),
}));

vi.mock("../gateway", () => ({
  accountDetailsGateway: {
    recordWithdrawal: mockRecordWithdrawal,
  },
}));

vi.mock("@/lib/snackbarStore", () => ({
  useSnackbar: () => mockShowSnackbar,
}));

vi.mock("@/lib/logger", () => ({
  logger: { error: vi.fn(), info: vi.fn(), warn: vi.fn() },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) =>
      opts ? `${key}::${JSON.stringify(opts)}` : key,
    i18n: { language: "en" },
  }),
}));

const fakeSubmit = { preventDefault: vi.fn() } as unknown as React.FormEvent;

describe("useWithdrawalTransaction (CSH-030/031/032/035/081)", () => {
  beforeEach(() => {
    mockRecordWithdrawal.mockReset();
    mockShowSnackbar.mockReset();
    vi.mocked(logger.error).mockClear();
  });

  // CSH-030 — initial state
  it("initial state has today's date and blank amount/note", () => {
    const { result } = renderHook(() => useWithdrawalTransaction({ accountId: "account-1" }));
    expect(result.current.formData.date).toBe(new Date().toISOString().slice(0, 10));
    expect(result.current.formData.amount).toBe("");
  });

  // CSH-031 — empty amount → invalid
  it("isFormValid false when amount blank", () => {
    const { result } = renderHook(() => useWithdrawalTransaction({ accountId: "account-1" }));
    expect(result.current.isFormValid).toBe(false);
  });

  // CSH-032 / CSH-035 — happy path
  it("submits and fires success snackbar on success", async () => {
    mockRecordWithdrawal.mockResolvedValue({ status: "ok", data: { id: "tx-1" } });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() =>
      useWithdrawalTransaction({ accountId: "account-1", onSubmitSuccess }),
    );

    act(() => result.current.handleChange("amount", "75.25"));
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockRecordWithdrawal).toHaveBeenCalledWith(
      expect.objectContaining({
        account_id: "account-1",
        amount_micros: 75_250_000,
      }),
    );
    expect(mockShowSnackbar).toHaveBeenCalledWith("cash.withdrawal_recorded", "success");
    expect(onSubmitSuccess).toHaveBeenCalled();
  });

  // CSH-081 — InsufficientCash includes balance + currency in inline error
  it("renders InsufficientCash inline error with balance + currency interpolation", async () => {
    mockRecordWithdrawal.mockResolvedValue({
      status: "error",
      error: {
        code: "InsufficientCash",
        current_balance_micros: 50_000_000,
        currency: "EUR",
      },
    });
    const { result } = renderHook(() => useWithdrawalTransaction({ accountId: "account-1" }));

    act(() => result.current.handleChange("amount", "999"));
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toContain("cash.insufficient_cash_inline");
    expect(result.current.error).toContain("50,00");
    expect(result.current.error).toContain("EUR");
  });

  // CSH-031 — generic backend error code surfaced as error.<code>
  it("surfaces generic backend error code", async () => {
    mockRecordWithdrawal.mockResolvedValue({
      status: "error",
      error: { code: "AmountNotPositive" },
    });
    const { result } = renderHook(() => useWithdrawalTransaction({ accountId: "account-1" }));

    act(() => result.current.handleChange("amount", "100"));
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toContain("error.AmountNotPositive");
  });

  // Unknown error path — diagnostic hint flows through to logger.error so
  // support reports retain the developer-only triage info that error.Unknown hides.
  it("logs full error including hint on Unknown backend error", async () => {
    mockRecordWithdrawal.mockResolvedValue({
      status: "error",
      error: { code: "Unknown", hint: "test diagnostic" },
    });
    const { result } = renderHook(() => useWithdrawalTransaction({ accountId: "account-1" }));

    act(() => result.current.handleChange("amount", "100"));
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toContain("error.Unknown");
    expect(logger.error).toHaveBeenCalledWith(
      "[useWithdrawalTransaction] recordWithdrawal failed",
      { error: { code: "Unknown", hint: "test diagnostic" } },
    );
    expect(mockShowSnackbar).not.toHaveBeenCalled();
  });
});
