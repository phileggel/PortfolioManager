import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { logger } from "@/lib/logger";
import { useDepositTransaction } from "./useDepositTransaction";

const { mockRecordDeposit, mockShowSnackbar } = vi.hoisted(() => ({
  mockRecordDeposit: vi.fn(),
  mockShowSnackbar: vi.fn(),
}));

vi.mock("../gateway", () => ({
  accountDetailsGateway: {
    recordDeposit: mockRecordDeposit,
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
    t: (key: string) => key,
    i18n: { language: "en" },
  }),
}));

const fakeSubmit = { preventDefault: vi.fn() } as unknown as React.FormEvent;

describe("useDepositTransaction (CSH-020/021/022/025)", () => {
  beforeEach(() => {
    mockRecordDeposit.mockReset();
    mockShowSnackbar.mockReset();
    vi.mocked(logger.error).mockClear();
  });

  // CSH-020 — initial form has today's date and empty amount
  it("initial state has today's date and blank amount/note", () => {
    const { result } = renderHook(() => useDepositTransaction({ accountId: "account-1" }));
    expect(result.current.formData.date).toBe(new Date().toISOString().slice(0, 10));
    expect(result.current.formData.amount).toBe("");
    expect(result.current.formData.note).toBe("");
  });

  // CSH-021 — empty amount makes the form invalid
  it("isFormValid false when amount is blank", () => {
    const { result } = renderHook(() => useDepositTransaction({ accountId: "account-1" }));
    expect(result.current.isFormValid).toBe(false);
  });

  // CSH-021 — amount > 0 makes the form valid
  it("isFormValid true when amount is positive and date is valid", () => {
    const { result } = renderHook(() => useDepositTransaction({ accountId: "account-1" }));
    act(() => result.current.handleChange("amount", "100"));
    expect(result.current.isFormValid).toBe(true);
  });

  // CSH-022 / CSH-025 — successful submit calls gateway then shows success snackbar
  it("submits and fires success snackbar on success", async () => {
    mockRecordDeposit.mockResolvedValue({ status: "ok", data: { id: "tx-1" } });
    const onSubmitSuccess = vi.fn();
    const { result } = renderHook(() =>
      useDepositTransaction({ accountId: "account-1", onSubmitSuccess }),
    );

    act(() => result.current.handleChange("amount", "250.50"));

    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(mockRecordDeposit).toHaveBeenCalledWith(
      expect.objectContaining({
        account_id: "account-1",
        amount_micros: 250_500_000,
      }),
    );
    expect(mockShowSnackbar).toHaveBeenCalledWith("cash.deposit_recorded", "success");
    expect(onSubmitSuccess).toHaveBeenCalled();
  });

  // CSH-021 — backend rejects AmountNotPositive → inline error key set
  it("surfaces backend error code as inline error", async () => {
    mockRecordDeposit.mockResolvedValue({
      status: "error",
      error: { code: "AmountNotPositive" },
    });
    const { result } = renderHook(() => useDepositTransaction({ accountId: "account-1" }));

    act(() => result.current.handleChange("amount", "100"));
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toBe("error.AmountNotPositive");
    expect(mockShowSnackbar).not.toHaveBeenCalled();
  });

  // Unknown error path — diagnostic hint flows through to logger.error so
  // support reports retain the developer-only triage info that error.Unknown hides.
  it("logs full error including hint on Unknown backend error", async () => {
    mockRecordDeposit.mockResolvedValue({
      status: "error",
      error: { code: "Unknown", hint: "test diagnostic" },
    });
    const { result } = renderHook(() => useDepositTransaction({ accountId: "account-1" }));

    act(() => result.current.handleChange("amount", "100"));
    await act(async () => {
      await result.current.handleSubmit(fakeSubmit);
    });

    expect(result.current.error).toBe("error.Unknown");
    expect(logger.error).toHaveBeenCalledWith("[useDepositTransaction] recordDeposit failed", {
      error: { code: "Unknown", hint: "test diagnostic" },
    });
    expect(mockShowSnackbar).not.toHaveBeenCalled();
  });
});
