import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  Asset,
  AssetApplicationError,
  AssetCrudError,
  AssetDomainError,
  AssetLookupResult,
  CategoryApplicationError,
  CreateAssetDTO,
  UpdateAssetDTO,
  WebLookupCommandError,
} from "@/bindings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockInvoke = vi.mocked(invoke);

// Import after mock is registered so bindings.ts picks up the mock
const { assetGateway } = await import("./gateway");

const makeAsset = (): Asset => ({
  id: "asset-1",
  name: "Apple Inc.",
  reference: "AAPL",
  class: "Stocks",
  category: { id: "cat-1", name: "Equities" },
  currency: "USD",
  risk_level: 3,
  is_archived: false,
});

const baseCreateDto: CreateAssetDTO = {
  name: "Apple Inc.",
  reference: "AAPL",
  class: "Stocks",
  currency: "USD",
  risk_level: 3,
  category_id: "cat-1",
};

const baseUpdateDto: UpdateAssetDTO = {
  asset_id: "asset-1",
  name: "Apple Inc.",
  reference: "AAPL",
  class: "Stocks",
  currency: "USD",
  risk_level: 3,
  category_id: "cat-1",
};

describe("asset gateway — CRUD", () => {
  beforeEach(() => vi.clearAllMocks());

  // ── getAssets / getAssetsWithArchived ──────────────────────────────────────

  it("getAssets returns list on success", async () => {
    const assets = [makeAsset()];
    mockInvoke.mockResolvedValue(assets);
    const result = await assetGateway.getAssets();
    expect(result).toEqual({ status: "ok", data: assets });
    expect(mockInvoke).toHaveBeenCalledWith("get_assets");
  });

  it("getAssets surfaces DatabaseError on repo failure", async () => {
    const err: AssetApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.getAssets();
    expect(result).toEqual({ status: "error", error: err });
  });

  it("getAssetsWithArchived surfaces DatabaseError on repo failure", async () => {
    const err: AssetApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.getAssetsWithArchived();
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── createAsset ────────────────────────────────────────────────────────────

  it("createAsset returns Asset on success", async () => {
    const asset = makeAsset();
    mockInvoke.mockResolvedValue(asset);
    const result = await assetGateway.createAsset(baseCreateDto);
    expect(result).toEqual({ status: "ok", data: asset });
    expect(mockInvoke).toHaveBeenCalledWith("add_asset", { dto: baseCreateDto });
  });

  it("createAsset surfaces NameEmpty domain leaf", async () => {
    const err: AssetDomainError = { code: "NameEmpty" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.createAsset({ ...baseCreateDto, name: "" });
    expect(result).toEqual({ status: "error", error: err });
  });

  it("createAsset surfaces ReferenceEmpty domain leaf", async () => {
    const err: AssetDomainError = { code: "ReferenceEmpty" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.createAsset({ ...baseCreateDto, reference: "" });
    expect(result).toEqual({ status: "error", error: err });
  });

  it("createAsset surfaces InvalidCurrency with currency payload", async () => {
    const err: AssetDomainError = { code: "InvalidCurrency", currency: "XYZ" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.createAsset({ ...baseCreateDto, currency: "XYZ" });
    expect(result).toEqual({ status: "error", error: err });
  });

  it("createAsset surfaces InvalidRiskLevel with received payload", async () => {
    const err: AssetDomainError = { code: "InvalidRiskLevel", received: 9 };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.createAsset({ ...baseCreateDto, risk_level: 9 });
    expect(result).toEqual({ status: "error", error: err });
  });

  it("createAsset surfaces CategoryApplicationError NotFound from cross-aggregate lookup", async () => {
    const err: CategoryApplicationError = { code: "NotFound", id: "missing-cat" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.createAsset({ ...baseCreateDto, category_id: "missing-cat" });
    expect(result).toEqual({ status: "error", error: err });
  });

  it("createAsset surfaces DatabaseError on repo write failure", async () => {
    const err: AssetApplicationError = { code: "DatabaseError" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.createAsset(baseCreateDto);
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── updateAsset ────────────────────────────────────────────────────────────

  it("updateAsset returns Asset on success", async () => {
    const asset = makeAsset();
    mockInvoke.mockResolvedValue(asset);
    const result = await assetGateway.updateAsset(baseUpdateDto);
    expect(result).toEqual({ status: "ok", data: asset });
    expect(mockInvoke).toHaveBeenCalledWith("update_asset", { dto: baseUpdateDto });
  });

  it("updateAsset surfaces NotFound with asset id payload", async () => {
    const err: AssetApplicationError = { code: "NotFound", id: "missing-id" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.updateAsset({ ...baseUpdateDto, asset_id: "missing-id" });
    expect(result).toEqual({ status: "error", error: err });
  });

  it("updateAsset surfaces Archived domain leaf", async () => {
    const err: AssetDomainError = { code: "Archived" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.updateAsset(baseUpdateDto);
    expect(result).toEqual({ status: "error", error: err });
  });

  it("updateAsset surfaces CashAssetNotEditable for system Cash Asset", async () => {
    const err: AssetDomainError = { code: "CashAssetNotEditable" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.updateAsset({
      ...baseUpdateDto,
      asset_id: "system-cash-eur",
    });
    expect(result).toEqual({ status: "error", error: err });
  });

  // ── unarchiveAsset ─────────────────────────────────────────────────────────

  it("unarchiveAsset returns null on success", async () => {
    mockInvoke.mockResolvedValue(null);
    const result = await assetGateway.unarchiveAsset("asset-1");
    expect(result).toEqual({ status: "ok", data: null });
    expect(mockInvoke).toHaveBeenCalledWith("unarchive_asset", { id: "asset-1" });
  });

  it("unarchiveAsset surfaces CashAssetNotEditable for system Cash Asset", async () => {
    const err: AssetCrudError = { code: "CashAssetNotEditable" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.unarchiveAsset("system-cash-eur");
    expect(result).toEqual({ status: "error", error: err });
  });

  it("unarchiveAsset surfaces NotFound with id payload", async () => {
    const err: AssetCrudError = { code: "NotFound", id: "missing" };
    mockInvoke.mockRejectedValue(err);
    const result = await assetGateway.unarchiveAsset("missing");
    expect(result).toEqual({ status: "error", error: err });
  });
});

describe("asset gateway — lookupAsset", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // WEB-020 — success path returns AssetLookupResult[]
  it("lookupAsset returns result list on success", async () => {
    const results: AssetLookupResult[] = [
      {
        name: "Apple Inc.",
        reference: "AAPL",
        currency: "USD",
        asset_class: "Stocks",
        exchange: null,
      },
      {
        name: "iShares Core S&P 500",
        reference: "IVV",
        currency: "USD",
        asset_class: "ETF",
        exchange: null,
      },
    ];
    // bindings.ts wraps the TAURI_INVOKE result in { status: "ok", data: ... }
    mockInvoke.mockResolvedValue(results);

    const res = await assetGateway.lookupAsset("AAPL");

    expect(res).toEqual({ status: "ok", data: results });
    expect(mockInvoke).toHaveBeenCalledWith("lookup_asset", { query: "AAPL" });
  });

  // WEB-020 — ISIN query (12 alphanumeric chars) is forwarded as-is
  it("lookupAsset forwards 12-char ISIN query verbatim", async () => {
    const results: AssetLookupResult[] = [
      {
        name: "Apple Inc.",
        reference: "US0378331005",
        currency: "USD",
        asset_class: "Stocks",
        exchange: null,
      },
    ];
    mockInvoke.mockResolvedValue(results);

    const res = await assetGateway.lookupAsset("US0378331005");

    expect(res).toEqual({ status: "ok", data: results });
    expect(mockInvoke).toHaveBeenCalledWith("lookup_asset", {
      query: "US0378331005",
    });
  });

  // WEB-020 — empty list is a valid success (WEB-032 handled by UI layer)
  it("lookupAsset returns empty list when no instruments found", async () => {
    mockInvoke.mockResolvedValue([]);

    const res = await assetGateway.lookupAsset("xyzzy-not-a-real-ticker");

    expect(res).toEqual({ status: "ok", data: [] });
    expect(mockInvoke).toHaveBeenCalledWith("lookup_asset", {
      query: "xyzzy-not-a-real-ticker",
    });
  });

  // WEB-025 — NetworkError is surfaced as { status: "error", error: { code: "NetworkError" } }
  it("lookupAsset returns NetworkError on network failure", async () => {
    const err: WebLookupCommandError = { code: "NetworkError" };
    // bindings.ts catches the rejection and returns { status: "error", error: e }
    mockInvoke.mockRejectedValue(err);

    const res = await assetGateway.lookupAsset("AAPL");

    expect(res).toEqual({ status: "error", error: err });
    expect(mockInvoke).toHaveBeenCalledWith("lookup_asset", { query: "AAPL" });
  });

  // WEB-023/WEB-024/WEB-046 — optional fields may be null
  it("lookupAsset preserves null optional fields from result", async () => {
    const results: AssetLookupResult[] = [
      {
        name: "Obscure Fund",
        reference: null,
        currency: null,
        asset_class: null,
        exchange: null,
      },
    ];
    mockInvoke.mockResolvedValue(results);

    const res = await assetGateway.lookupAsset("obscure fund");

    expect(res).toEqual({ status: "ok", data: results });
  });
});
