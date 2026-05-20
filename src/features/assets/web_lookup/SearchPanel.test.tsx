import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AssetLookupResult } from "@/bindings";
import { SearchPanel } from "./SearchPanel";
import type { WebLookupSearchState } from "./useWebLookupSearch";

// Translate asset.class.* keys to English labels so result-row assertions read
// naturally. opts.name covers the select_result interpolation. Everything else
// passes through as the key (sufficient for all other assertions in this file).
const CLASS_LABELS: Record<string, string> = {
  "asset.class.Cash": "Cash",
  "asset.class.Bonds": "Bonds",
  "asset.class.RealEstate": "Real Estate",
  "asset.class.MutualFunds": "Mutual Funds",
  "asset.class.ETF": "ETF",
  "asset.class.Stocks": "Stocks",
  "asset.class.DigitalAsset": "Digital Asset",
  "asset.class.Derivatives": "Derivatives",
};

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, string>) => opts?.name ?? CLASS_LABELS[key] ?? key,
  }),
}));

const noop = () => {};

function renderPanel(state: WebLookupSearchState, onSelect = noop, query = "") {
  return render(
    <SearchPanel
      query={query}
      setQuery={noop}
      state={state}
      submit={noop}
      retry={noop}
      onSelect={onSelect}
      onFillManually={noop}
    />,
  );
}

// ---------------------------------------------------------------------------
// Status states
// ---------------------------------------------------------------------------

describe("SearchPanel — status states", () => {
  // WEB-011 — idle hint shown when no search has been made
  it("shows idle hint in idle state", () => {
    renderPanel({ status: "idle" });
    expect(screen.getByText("asset.web_lookup.idle_hint")).toBeInTheDocument();
  });

  // WEB-030 — loading indicator shown while search is in progress
  it("shows loading indicator in loading state", () => {
    renderPanel({ status: "loading" });
    expect(screen.getByText("asset.web_lookup.loading")).toBeInTheDocument();
  });

  // WEB-032 — empty state shown when no results returned
  it("shows no-results message in empty state", () => {
    renderPanel({ status: "empty" });
    expect(screen.getByText("asset.web_lookup.no_results")).toBeInTheDocument();
  });

  // WEB-033 — NetworkError state shows the generic network copy
  it("shows network error message and retry button when state.code is NetworkError", () => {
    const retry = vi.fn();
    render(
      <SearchPanel
        query=""
        setQuery={noop}
        state={{ status: "error", code: "NetworkError" }}
        submit={noop}
        retry={retry}
        onSelect={noop}
        onFillManually={noop}
      />,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText("asset.web_lookup.error_network")).toBeInTheDocument();
    expect(screen.queryByText("asset.web_lookup.error_rate_limit")).not.toBeInTheDocument();
    const retryBtn = screen.getByRole("button", {
      name: "asset.web_lookup.action_retry",
    });
    expect(retryBtn).toBeInTheDocument();
    fireEvent.click(retryBtn);
    expect(retry).toHaveBeenCalledTimes(1);
  });

  // WEB-011 — search button disabled when query is empty
  it("disables search button when query is empty", () => {
    renderPanel({ status: "idle" }, noop, "");
    expect(screen.getByRole("button", { name: "asset.web_lookup.action_search" })).toBeDisabled();
  });

  // WEB-011 — search button enabled when query is non-empty
  it("enables search button when query is non-empty", () => {
    renderPanel({ status: "idle" }, noop, "AAPL");
    expect(
      screen.getByRole("button", { name: "asset.web_lookup.action_search" }),
    ).not.toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Results row layout (WEB-031)
// ---------------------------------------------------------------------------

describe("SearchPanel — result row layout (WEB-031)", () => {
  const stockResult: AssetLookupResult = {
    name: "Apple Inc.",
    reference: "AAPL",
    currency: "USD",
    asset_class: "Stocks",
    exchange: { code: "XNYS", label: "New York Stock Exchange" },
  };

  // WEB-031 — first line shows reference code and instrument name
  it("shows reference code and name on the first line", () => {
    renderPanel({ status: "results", results: [stockResult] });
    expect(screen.getByText("AAPL")).toBeInTheDocument();
    expect(screen.getByText("Apple Inc.")).toBeInTheDocument();
  });

  // WEB-031 — second line shows formatted class label and exchange separated by · (U+00B7 middle dot)
  it("shows class label · exchange on the second line when both present", () => {
    renderPanel({ status: "results", results: [stockResult] });
    expect(screen.getByText("Stocks · New York Stock Exchange")).toBeInTheDocument();
  });

  // WEB-031 — multi-word class names are human-readable on the second line
  it("renders multi-word class names as readable labels", () => {
    const fund: AssetLookupResult = {
      name: "Vanguard 500",
      reference: "VFIAX",
      currency: "USD",
      asset_class: "MutualFunds",
      exchange: null,
    };
    renderPanel({ status: "results", results: [fund] });
    expect(screen.getByText("Mutual Funds")).toBeInTheDocument();
  });

  // WEB-031 — when exchange absent, second line shows type label only (no separator)
  it("shows type label only when exchange is absent", () => {
    const noExchange: AssetLookupResult = {
      name: "iShares Core ETF",
      reference: "IVV",
      currency: "USD",
      asset_class: "ETF",
      exchange: null,
    };
    renderPanel({ status: "results", results: [noExchange] });
    expect(screen.getByText("ETF")).toBeInTheDocument();
    expect(screen.queryByText(/·/)).toBeNull();
  });

  // WEB-031 — when asset_class absent but exchange present, shows fallback · exchange
  it("shows type_unknown fallback · exchange when asset_class is absent but exchange is present", () => {
    const noClass: AssetLookupResult = {
      name: "Structured Product X",
      reference: null,
      currency: null,
      asset_class: null,
      exchange: { code: "XLON", label: "London Stock Exchange" },
    };
    renderPanel({ status: "results", results: [noClass] });
    expect(
      screen.getByText("asset.web_lookup.type_unknown · London Stock Exchange"),
    ).toBeInTheDocument();
  });

  // WEB-031 — when both asset_class and exchange absent, shows fallback label only
  it("shows type_unknown fallback only when both asset_class and exchange are absent", () => {
    const unknown: AssetLookupResult = {
      name: "Mystery Instrument",
      reference: null,
      currency: null,
      asset_class: null,
      exchange: null,
    };
    renderPanel({ status: "results", results: [unknown] });
    expect(screen.getByText("asset.web_lookup.type_unknown")).toBeInTheDocument();
    expect(screen.queryByText(/·/)).toBeNull();
  });

  // WEB-046 — reference prefix omitted when reference is absent
  it("omits the reference prefix when reference is null", () => {
    const noRef: AssetLookupResult = {
      name: "No Reference Fund",
      reference: null,
      currency: "EUR",
      asset_class: "MutualFunds",
      exchange: null,
    };
    renderPanel({ status: "results", results: [noRef] });
    // The instrument name is rendered; no reference code text should appear
    expect(screen.getByText("No Reference Fund")).toBeInTheDocument();
    expect(screen.queryByText("null")).toBeNull();
  });

  // WEB-040 — clicking a result calls onSelect with the result
  it("calls onSelect with the result when a row is clicked", () => {
    const onSelect = vi.fn();
    renderPanel({ status: "results", results: [stockResult] }, onSelect);
    fireEvent.click(screen.getByRole("button", { name: "Apple Inc." }));
    expect(onSelect).toHaveBeenCalledWith(stockResult);
  });
});

// ---------------------------------------------------------------------------
// WEB-013 — Fill manually bypass
// ---------------------------------------------------------------------------

describe("SearchPanel — fill manually bypass (WEB-013)", () => {
  it("fill manually button is always visible in idle state", () => {
    renderPanel({ status: "idle" });
    expect(
      screen.getByRole("button", {
        name: "asset.web_lookup.action_fill_manually",
      }),
    ).toBeInTheDocument();
  });

  it("fill manually button is visible even in error state", () => {
    renderPanel({ status: "error", code: "NetworkError" });
    expect(
      screen.getByRole("button", {
        name: "asset.web_lookup.action_fill_manually",
      }),
    ).toBeInTheDocument();
  });

  // WEB-033 — RateLimited state shows the wait-and-retry copy (not the generic network copy)
  it("shows rate-limit copy when state.code is RateLimited", () => {
    const retry = vi.fn();
    render(
      <SearchPanel
        query=""
        setQuery={noop}
        state={{ status: "error", code: "RateLimited" }}
        submit={noop}
        retry={retry}
        onSelect={noop}
        onFillManually={noop}
      />,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText("asset.web_lookup.error_rate_limit")).toBeInTheDocument();
    expect(screen.queryByText("asset.web_lookup.error_network")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "asset.web_lookup.action_retry" }),
    ).toBeInTheDocument();
  });
});
