import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  Account,
  AccountApplicationError,
  AccountCrudError,
  AccountDeletionSummary,
  AccountDomainError,
  CreateAccountDTO,
  UpdateAccountDTO,
} from "@/bindings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke);
const { accountGateway } = await import("./gateway");

const makeAccount = (): Account => ({
  id: "acc-1",
  name: "My Account",
  currency: "EUR",
  update_frequency: "ManualMonth",
});

describe("accountGateway", () => {
  beforeEach(() => vi.clearAllMocks());

  // ── getAccounts ──────────────────────────────────────────────────────────────

  it("getAccounts returns list on success", async () => {
    const accounts = [makeAccount()];
    mockInvoke.mockResolvedValue(accounts);
    const result = await accountGateway.getAccounts();
    expect(result).toEqual({ status: "ok", data: accounts });
    expect(mockInvoke).toHaveBeenCalledWith("get_accounts");
  });

  it("getAccounts surfaces DatabaseError on repo failure", async () => {
    const err: AccountApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.getAccounts();
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── addAccount ───────────────────────────────────────────────────────────────

  it("addAccount returns Account on success", async () => {
    const dto: CreateAccountDTO = {
      name: "New Account",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    const account = makeAccount();
    mockInvoke.mockResolvedValue(account);
    const result = await accountGateway.addAccount(dto);
    expect(result).toEqual({ status: "ok", data: account });
    expect(mockInvoke).toHaveBeenCalledWith("add_account", { dto });
  });

  it("addAccount returns NameAlreadyExists error", async () => {
    const dto: CreateAccountDTO = {
      name: "Duplicate",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    const err: AccountCrudError = { code: "NameAlreadyExists" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.addAccount(dto);
    expect(result).toEqual({ status: "error", error: err });
  });

  it("addAccount surfaces InvalidCurrency with currency payload", async () => {
    const dto: CreateAccountDTO = {
      name: "Test",
      currency: "XYZ",
      update_frequency: "ManualMonth",
    };
    const err: AccountDomainError = { code: "InvalidCurrency", currency: "XYZ" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.addAccount(dto);
    expect(result).toEqual({ status: "error", error: err });
  });

  it("addAccount surfaces NameEmpty domain error", async () => {
    const dto: CreateAccountDTO = {
      name: "  ",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    const err: AccountDomainError = { code: "NameEmpty" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.addAccount(dto);
    expect(result).toEqual({ status: "error", error: err });
  });

  it("addAccount surfaces DatabaseError on repo failure", async () => {
    const dto: CreateAccountDTO = {
      name: "Test",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    const err: AccountApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.addAccount(dto);
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── updateAccount ─────────────────────────────────────────────────────────────

  it("updateAccount returns updated Account on success", async () => {
    const dto: UpdateAccountDTO = {
      id: "acc-1",
      name: "Renamed",
      currency: "USD",
      update_frequency: "ManualMonth",
    };
    const account = { ...makeAccount(), name: "Renamed" };
    mockInvoke.mockResolvedValue(account);
    const result = await accountGateway.updateAccount(dto);
    expect(result).toEqual({ status: "ok", data: account });
    expect(mockInvoke).toHaveBeenCalledWith("update_account", { dto });
  });

  it("updateAccount returns error on failure", async () => {
    const dto: UpdateAccountDTO = {
      id: "acc-1",
      name: "X",
      currency: "EUR",
      update_frequency: "ManualMonth",
    };
    const err: AccountCrudError = { code: "NameAlreadyExists" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.updateAccount(dto);
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── deleteAccount ─────────────────────────────────────────────────────────────

  it("deleteAccount returns null on success", async () => {
    mockInvoke.mockResolvedValue(null);
    const result = await accountGateway.deleteAccount("acc-1");
    expect(result).toEqual({ status: "ok", data: null });
    expect(mockInvoke).toHaveBeenCalledWith("delete_account", { id: "acc-1" });
  });

  it("deleteAccount surfaces DatabaseError on repo failure", async () => {
    const err: AccountApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.deleteAccount("acc-1");
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── getAccountDeletionSummary ─────────────────────────────────────────────────

  it("getAccountDeletionSummary returns summary on success", async () => {
    const summary: AccountDeletionSummary = {
      holding_count: 2,
      transaction_count: 5,
    };
    mockInvoke.mockResolvedValue(summary);
    const result = await accountGateway.getAccountDeletionSummary("acc-1");
    expect(result).toEqual({ status: "ok", data: summary });
    expect(mockInvoke).toHaveBeenCalledWith("get_account_deletion_summary", {
      accountId: "acc-1",
    });
  });

  it("getAccountDeletionSummary surfaces DatabaseError on repo failure", async () => {
    const err: AccountApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await accountGateway.getAccountDeletionSummary("missing");
    expect(result).toEqual({ status: "error", error: err });
  });
});
