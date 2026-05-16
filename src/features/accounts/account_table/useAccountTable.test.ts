import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Account, AccountDeletionSummary } from "@/bindings";
import { useAccountTable } from "./useAccountTable";

function makeAccount(id: string, name: string, freq: Account["update_frequency"]): Account {
  return { id, name, currency: "EUR", update_frequency: freq };
}

const accounts: [Account, Account, Account, Account] = [
  makeAccount("1", "Alpha", "ManualYear"),
  makeAccount("2", "Beta", "Automatic"),
  makeAccount("3", "Gamma", "ManualDay"),
  makeAccount("4", "Delta", "ManualMonth"),
];

const noopDelete = vi.fn().mockResolvedValue({ error: null });
const noopAccountClick = vi.fn();

function makeEmptySummary(): AccountDeletionSummary {
  return { holding_count: 0, transaction_count: 0 };
}

function makeNonEmptySummary(holdings = 2, transactions = 5): AccountDeletionSummary {
  return { holding_count: holdings, transaction_count: transactions };
}

const noopSummary = vi.fn().mockResolvedValue({ data: makeEmptySummary(), error: null });

function makeKeyEvent(key: string): React.KeyboardEvent {
  return { key, preventDefault: vi.fn() } as unknown as React.KeyboardEvent;
}

function makeMouseEvent(): React.MouseEvent {
  return { stopPropagation: vi.fn() } as unknown as React.MouseEvent;
}

describe("useAccountTable", () => {
  // R9 — frequency sorted by logical enum order, not alphabetical label
  it("sorts update_frequency by logical enum order ascending", () => {
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, noopAccountClick),
    );

    act(() => {
      result.current.handleSort("update_frequency");
    });

    const freqs = result.current.sortedAndFilteredAccounts.map((a) => a.update_frequency);
    expect(freqs).toEqual(["Automatic", "ManualDay", "ManualMonth", "ManualYear"]);
  });

  // R9 — descending reverses logical order
  it("sorts update_frequency by logical enum order descending on second click", () => {
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, noopAccountClick),
    );

    act(() => result.current.handleSort("update_frequency"));
    act(() => result.current.handleSort("update_frequency"));

    const freqs = result.current.sortedAndFilteredAccounts.map((a) => a.update_frequency);
    expect(freqs).toEqual(["ManualYear", "ManualMonth", "ManualDay", "Automatic"]);
  });

  // R10 — search active with no match → hasNoSearchResults true
  it("sets hasNoSearchResults when filter is active but no match", () => {
    const { result } = renderHook(() =>
      useAccountTable(accounts, "zzz", noopDelete, noopSummary, noopAccountClick),
    );
    expect(result.current.hasNoSearchResults).toBe(true);
    expect(result.current.isEmpty).toBe(false);
  });

  // R11 — empty list with no filter → isEmpty true
  it("sets isEmpty when list is empty and no search is active", () => {
    const { result } = renderHook(() =>
      useAccountTable([], "", noopDelete, noopSummary, noopAccountClick),
    );
    expect(result.current.isEmpty).toBe(true);
    expect(result.current.hasNoSearchResults).toBe(false);
  });

  // R10 / R11 — empty list with active filter → hasNoSearchResults (not isEmpty)
  it("sets hasNoSearchResults (not isEmpty) when list is empty but filter is active", () => {
    const { result } = renderHook(() =>
      useAccountTable([], "something", noopDelete, noopSummary, noopAccountClick),
    );
    expect(result.current.hasNoSearchResults).toBe(true);
    expect(result.current.isEmpty).toBe(false);
  });

  it("handleRowKeyDown calls onAccountClick on Enter", () => {
    const onClick = vi.fn();
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, onClick),
    );
    const e = makeKeyEvent("Enter");

    act(() => result.current.handleRowKeyDown(e, "2"));

    expect(e.preventDefault).toHaveBeenCalled();
    expect(onClick).toHaveBeenCalledWith("2");
  });

  it("handleRowKeyDown calls onAccountClick on Space", () => {
    const onClick = vi.fn();
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, onClick),
    );
    const e = makeKeyEvent(" ");

    act(() => result.current.handleRowKeyDown(e, "3"));

    expect(onClick).toHaveBeenCalledWith("3");
  });

  it("handleRowKeyDown ignores other keys", () => {
    const onClick = vi.fn();
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, onClick),
    );
    const e = makeKeyEvent("Tab");

    act(() => result.current.handleRowKeyDown(e, "1"));

    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(onClick).not.toHaveBeenCalled();
  });

  it("handleEditClick stops propagation and sets editData", () => {
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, noopAccountClick),
    );
    const e = makeMouseEvent();
    const account = accounts[0];

    act(() => result.current.handleEditClick(e, account));

    expect(e.stopPropagation).toHaveBeenCalled();
    expect(result.current.editData).toBe(account);
  });

  it("handleEditClose clears editData", () => {
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, noopSummary, noopAccountClick),
    );

    act(() => result.current.handleEditClick(makeMouseEvent(), accounts[0]));
    act(() => result.current.handleEditClose());

    expect(result.current.editData).toBeNull();
  });

  // ACC-018 — empty account: handleDeleteClick fetches summary and opens standard dialog
  it("handleDeleteClick fetches summary and sets deleteData (empty account)", async () => {
    const getSummary = vi.fn().mockResolvedValue({ data: makeEmptySummary(), error: null });
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, getSummary, noopAccountClick),
    );
    const e = makeMouseEvent();

    await act(async () => {
      await result.current.handleDeleteClick(e, "1", "Alpha");
    });

    expect(e.stopPropagation).toHaveBeenCalled();
    expect(getSummary).toHaveBeenCalledWith("1");
    expect(result.current.deleteData).toEqual({ id: "1", name: "Alpha" });
    expect(result.current.deleteSummary).toEqual(makeEmptySummary());
  });

  // ACC-019 — non-empty account: summary has holdings, dialog carries counts
  it("handleDeleteClick sets non-empty deleteSummary for reinforced dialog (ACC-019)", async () => {
    const summary = makeNonEmptySummary(3, 7);
    const getSummary = vi.fn().mockResolvedValue({ data: summary, error: null });
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, getSummary, noopAccountClick),
    );

    await act(async () => {
      await result.current.handleDeleteClick(makeMouseEvent(), "2", "Beta");
    });

    expect(result.current.deleteData).toEqual({ id: "2", name: "Beta" });
    expect(result.current.deleteSummary?.holding_count).toBe(3);
    expect(result.current.deleteSummary?.transaction_count).toBe(7);
  });

  // ACC-018/019 — summary fetch error: dialog does not open, actionError is set
  it("handleDeleteClick shows actionError and does not open dialog when summary fetch fails", async () => {
    const getSummary = vi.fn().mockResolvedValue({ data: null, error: "error.Unknown" });
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, getSummary, noopAccountClick),
    );

    await act(async () => {
      await result.current.handleDeleteClick(makeMouseEvent(), "1", "Alpha");
    });

    expect(result.current.deleteData).toBeNull();
    expect(result.current.actionError).toBe("error.Unknown");
  });

  it("handleDeleteCancel clears deleteData and deleteSummary", async () => {
    const getSummary = vi.fn().mockResolvedValue({ data: makeEmptySummary(), error: null });
    const { result } = renderHook(() =>
      useAccountTable(accounts, "", noopDelete, getSummary, noopAccountClick),
    );

    await act(async () => {
      await result.current.handleDeleteClick(makeMouseEvent(), "1", "Alpha");
    });
    act(() => result.current.handleDeleteCancel());

    expect(result.current.deleteData).toBeNull();
    expect(result.current.deleteSummary).toBeNull();
  });
});
