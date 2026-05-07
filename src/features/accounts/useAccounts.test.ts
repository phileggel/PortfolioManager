import { renderHook } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  Account,
  AccountDeletionSummary,
  CreateAccountDTO,
  UpdateAccountDTO,
} from "@/bindings";
import { useAppStore } from "@/lib/store";

const { mockAddAccount, mockUpdateAccount, mockDeleteAccount, mockGetSummary } = vi.hoisted(() => ({
  mockAddAccount: vi.fn(),
  mockUpdateAccount: vi.fn(),
  mockDeleteAccount: vi.fn(),
  mockGetSummary: vi.fn(),
}));

vi.mock("./gateway", () => ({
  accountGateway: {
    addAccount: mockAddAccount,
    updateAccount: mockUpdateAccount,
    deleteAccount: mockDeleteAccount,
    getAccountDeletionSummary: mockGetSummary,
    getAccounts: vi.fn(),
  },
}));

vi.mock("@/lib/logger", () => ({
  logger: { error: vi.fn(), info: vi.fn() },
}));

const { useAccounts } = await import("./useAccounts");

const makeAccount = (): Account => ({
  id: "acc-1",
  name: "My Account",
  currency: "EUR",
  update_frequency: "ManualMonth",
});

describe("useAccounts", () => {
  beforeEach(() => {
    mockAddAccount.mockReset();
    mockUpdateAccount.mockReset();
    mockDeleteAccount.mockReset();
    mockGetSummary.mockReset();
    // Override store fetchAccounts so mutations don't hit the gateway.
    useAppStore.setState({
      accounts: [] as Account[],
      isLoadingAccounts: false,
      accountsError: null,
      fetchAccounts: vi.fn(),
    });
  });

  // ── addAccount ────────────────────────────────────────────────────────────────

  it("addAccount returns data on success", async () => {
    const account = makeAccount();
    mockAddAccount.mockResolvedValue({ status: "ok", data: account });
    const { result } = renderHook(() => useAccounts());
    let ret: { data: Account | null; error: string | null } = {
      data: null,
      error: null,
    };
    const dto: CreateAccountDTO = {
      name: "My Account",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    await act(async () => {
      ret = await result.current.addAccount(dto);
    });
    expect(mockAddAccount).toHaveBeenCalledWith(dto);
    expect(ret.data).toEqual(account);
    expect(ret.error).toBeNull();
  });

  it("addAccount returns NameAlreadyExists error code on conflict", async () => {
    mockAddAccount.mockResolvedValue({
      status: "error",
      error: { code: "NameAlreadyExists" },
    });
    const { result } = renderHook(() => useAccounts());
    let ret: { data: Account | null; error: string | null } = {
      data: null,
      error: null,
    };
    await act(async () => {
      ret = await result.current.addAccount({
        name: "Dup",
        currency: "EUR",
        update_frequency: "ManualMonth",
      });
    });
    expect(ret.error).toBe("error.NameAlreadyExists");
  });

  // ── updateAccount ─────────────────────────────────────────────────────────────

  it("updateAccount returns data on success", async () => {
    const account = makeAccount();
    mockUpdateAccount.mockResolvedValue({ status: "ok", data: account });
    const { result } = renderHook(() => useAccounts());
    let ret: { data: Account | null; error: string | null } = {
      data: null,
      error: null,
    };
    const dto: UpdateAccountDTO = {
      id: "acc-1",
      name: "Renamed",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    await act(async () => {
      ret = await result.current.updateAccount(dto);
    });
    expect(mockUpdateAccount).toHaveBeenCalledWith(dto);
    expect(ret.data).toEqual(account);
  });

  it("updateAccount returns NameAlreadyExists error code on conflict", async () => {
    mockUpdateAccount.mockResolvedValue({
      status: "error",
      error: { code: "NameAlreadyExists" },
    });
    const { result } = renderHook(() => useAccounts());
    let ret: { data: Account | null; error: string | null } = {
      data: null,
      error: null,
    };
    await act(async () => {
      ret = await result.current.updateAccount({
        id: "acc-1",
        name: "Dup",
        currency: "EUR",
        update_frequency: "ManualMonth",
      });
    });
    expect(ret.error).toBe("error.NameAlreadyExists");
  });

  // ── deleteAccount ─────────────────────────────────────────────────────────────

  it("deleteAccount returns null error on success", async () => {
    mockDeleteAccount.mockResolvedValue({ status: "ok", data: null });
    const { result } = renderHook(() => useAccounts());
    let ret: { error: string | null } = { error: "sentinel" };
    await act(async () => {
      ret = await result.current.deleteAccount("acc-1");
    });
    expect(mockDeleteAccount).toHaveBeenCalledWith("acc-1");
    expect(ret.error).toBeNull();
  });

  it("deleteAccount returns error code on failure", async () => {
    mockDeleteAccount.mockResolvedValue({
      status: "error",
      error: { code: "Unknown" },
    });
    const { result } = renderHook(() => useAccounts());
    let ret: { error: string | null } = { error: null };
    await act(async () => {
      ret = await result.current.deleteAccount("acc-1");
    });
    expect(ret.error).toBe("error.Unknown");
  });

  // ── getAccountDeletionSummary ─────────────────────────────────────────────────

  it("getAccountDeletionSummary returns summary on success", async () => {
    const summary: AccountDeletionSummary = {
      holding_count: 2,
      transaction_count: 5,
    };
    mockGetSummary.mockResolvedValue({ status: "ok", data: summary });
    const { result } = renderHook(() => useAccounts());
    let ret: { data: AccountDeletionSummary | null; error: string | null } = {
      data: null,
      error: null,
    };
    await act(async () => {
      ret = await result.current.getAccountDeletionSummary("acc-1");
    });
    expect(mockGetSummary).toHaveBeenCalledWith("acc-1");
    expect(ret.data).toEqual(summary);
    expect(ret.error).toBeNull();
  });

  it("getAccountDeletionSummary returns error code on failure", async () => {
    mockGetSummary.mockResolvedValue({
      status: "error",
      error: { code: "Unknown" },
    });
    const { result } = renderHook(() => useAccounts());
    let ret: { data: AccountDeletionSummary | null; error: string | null } = {
      data: null,
      error: null,
    };
    await act(async () => {
      ret = await result.current.getAccountDeletionSummary("missing");
    });
    expect(ret.error).toBe("error.Unknown");
  });
});
