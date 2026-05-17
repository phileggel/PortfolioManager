import type {
  AccountDetailsResponse,
  AssetPriceSource,
  ClosedHoldingDetail,
  HoldingDetail,
} from "@/bindings";
import { microToFormatted } from "@/lib/microUnits";

const DASH = "—";
const CASH_ASSET_PREFIX = "system-cash-";

/** True when the asset_id is the deterministic system Cash Asset ID (CSH-014). */
export function isCashAsset(assetId: string): boolean {
  return assetId.startsWith(CASH_ASSET_PREFIX);
}

export interface HoldingRowViewModel {
  assetId: string;
  assetName: string;
  assetReference: string;
  /** ISO 4217 currency code of the asset (MKT-023). */
  assetCurrency: string;
  quantity: string;
  /** Holding quantity in raw micro-units — used to pass to SellTransactionModal (SEL-010). */
  quantityMicro: number;
  averagePrice: string;
  costBasis: string;
  /** Formatted realized P&L string (2 decimal places, SEL-042). */
  realizedPnl: string;
  /** Raw realized P&L in micro-units — used for sign-based color styling (SEL-043). */
  realizedPnlRaw: number;
  /** Always true — active holding rows can trigger the price entry modal (MKT-010). */
  canEnterPrice: boolean;
  /** Formatted current market price (2 decimal places) or "—" when no price recorded (MKT-030). */
  currentPrice: string;
  /** ISO date of the price observation, or null when no price recorded (MKT-030). */
  currentPriceDate: string | null;
  /** Formatted unrealized P&L (2 decimal places) or "—" when not computable (MKT-032/034). */
  unrealizedPnl: string;
  /** Raw unrealized P&L in micro-units, or null when not computable (MKT-034). */
  unrealizedPnlRaw: number | null;
  /** Formatted performance % (e.g. "5.25%") or "—" when not computable (MKT-032/035). */
  performancePct: string;
  /** True when this row is the system Cash Holding (CSH-090). Drives the cash variant in HoldingRow. */
  isCash: boolean;
  /** Staleness label for the current price (MKT-140); null when no price is recorded. */
  staleness: StalenessLabel | null;
  /** i18n key for the price source badge (MKT-142), or null when no price is recorded. */
  sourceLabel: string | null;
}

/** i18n key + optional interpolation params for the price staleness label (MKT-140). */
export type StalenessLabel = { key: string; params?: { days: number } };

export interface ClosedHoldingRowViewModel {
  assetId: string;
  assetName: string;
  assetReference: string;
  /** Formatted realized P&L string (2 decimal places, ACD-049). */
  realizedPnl: string;
  /** Raw realized P&L in micro-units — used for sign-based color styling (ACD-049). */
  realizedPnlRaw: number;
  /** ISO date of last sell "YYYY-MM-DD" (ACD-049). */
  lastSoldDate: string;
}

export interface AccountSummaryViewModel {
  accountName: string;
  totalCostBasis: string;
  /** Formatted total realized P&L string (2 decimal places, SEL-042). */
  totalRealizedPnl: string;
  /** Raw total realized P&L in micro-units — used for sign-based color styling (SEL-043). */
  totalRealizedPnlRaw: number;
  holdingCount: number;
  isEmpty: boolean;
  isAllClosed: boolean;
  /** True when there is at least one closed holding to display (ACD-048). */
  hasClosedHoldings: boolean;
  /** Formatted total unrealized P&L (2 decimals) or "—" when no qualifying holdings (MKT-041). */
  totalUnrealizedPnl: string;
  /** Formatted total Global Value (cash + priced holdings, 2 decimals, CSH-094). */
  totalGlobalValue: string;
  /** Raw total Global Value in micro-units (CSH-094). */
  totalGlobalValueRaw: number;
  /** True when the account currently holds a non-zero cash balance (CSH-019/095). */
  hasCashHolding: boolean;
}

/**
 * MKT-140 — Returns an i18n descriptor for the staleness of the current price.
 * `null` when no date is recorded; `{ key: "mkt.staleness_today" }` when the price is from today;
 * `{ key: "mkt.staleness_days_ago", params: { days } }` otherwise.
 *
 * The caller renders via `t(label.key, label.params)`.
 */
export function formatStaleness(
  currentPriceDate: string | null,
  today: Date,
): StalenessLabel | null {
  if (currentPriceDate === null) return null;
  const observed = new Date(`${currentPriceDate}T00:00:00`);
  if (Number.isNaN(observed.getTime())) return null;
  const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const millisPerDay = 24 * 60 * 60 * 1000;
  const dayDelta = Math.floor((startOfToday.getTime() - observed.getTime()) / millisPerDay);
  if (dayDelta <= 0) return { key: "mkt.staleness_today" };
  return { key: "mkt.staleness_days_ago", params: { days: dayDelta } };
}

/**
 * MKT-141 / MKT-142 — Maps an AssetPriceSource to its i18n label key, or null when source is null.
 */
export function formatSource(source: AssetPriceSource | null): string | null {
  if (source === null) return null;
  switch (source) {
    case "Manual":
      return "mkt.source_manual";
    case "Stooq":
      return "mkt.source_stooq";
  }
}

export function toHoldingRow(detail: HoldingDetail): HoldingRowViewModel {
  const isCash = isCashAsset(detail.asset_id);
  if (isCash) {
    // Cash row variant (CSH-090/091): no cost basis, average price, realized PnL or
    // market-price columns — those cells render blank in the table. Quantity is the
    // running cash balance, formatted to 2 decimals like an amount.
    return {
      assetId: detail.asset_id,
      assetName: detail.asset_name,
      assetReference: detail.asset_reference,
      assetCurrency: detail.asset_currency,
      quantity: microToFormatted(detail.quantity, 2),
      quantityMicro: detail.quantity,
      averagePrice: "",
      costBasis: "",
      realizedPnl: "",
      realizedPnlRaw: 0,
      canEnterPrice: false,
      currentPrice: "",
      currentPriceDate: null,
      unrealizedPnl: "",
      unrealizedPnlRaw: null,
      performancePct: "",
      isCash: true,
      staleness: null,
      sourceLabel: null,
    };
  }
  return {
    assetId: detail.asset_id,
    assetName: detail.asset_name,
    assetReference: detail.asset_reference,
    assetCurrency: detail.asset_currency,
    quantity: microToFormatted(detail.quantity, 6),
    quantityMicro: detail.quantity,
    averagePrice: microToFormatted(detail.average_price, 2),
    costBasis: microToFormatted(detail.cost_basis, 2),
    realizedPnl: microToFormatted(detail.realized_pnl, 2),
    realizedPnlRaw: detail.realized_pnl,
    canEnterPrice: true,
    currentPrice: detail.current_price !== null ? microToFormatted(detail.current_price, 2) : DASH,
    currentPriceDate: detail.current_price_date,
    unrealizedPnl:
      detail.unrealized_pnl !== null ? microToFormatted(detail.unrealized_pnl, 2) : DASH,
    unrealizedPnlRaw: detail.unrealized_pnl,
    performancePct:
      detail.performance_pct !== null ? `${microToFormatted(detail.performance_pct, 2)}%` : DASH,
    isCash: false,
    staleness: formatStaleness(detail.current_price_date, new Date()),
    sourceLabel: formatSource(detail.current_price_source),
  };
}

export function toClosedHoldingRow(detail: ClosedHoldingDetail): ClosedHoldingRowViewModel {
  return {
    assetId: detail.asset_id,
    assetName: detail.asset_name,
    assetReference: detail.asset_reference,
    realizedPnl: microToFormatted(detail.realized_pnl, 2),
    realizedPnlRaw: detail.realized_pnl,
    lastSoldDate: detail.last_sold_date,
  };
}

export function toAccountSummary(response: AccountDetailsResponse): AccountSummaryViewModel {
  // CSH-098 — exclude the cash holding from the active count used for empty/all-closed
  // gating, otherwise an account with only a cash holding would never trigger the
  // "no positions yet" empty state nor the closed-only message.
  const nonCashActive = response.holdings.filter((h) => !isCashAsset(h.asset_id));
  const hasCashHolding = response.holdings.some((h) => isCashAsset(h.asset_id) && h.quantity > 0);
  return {
    accountName: response.account_name,
    totalCostBasis: microToFormatted(response.total_cost_basis, 2),
    totalRealizedPnl: microToFormatted(response.total_realized_pnl, 2),
    totalRealizedPnlRaw: response.total_realized_pnl,
    holdingCount: response.total_holding_count,
    isEmpty:
      response.total_holding_count === 0 ||
      (nonCashActive.length === 0 && response.closed_holdings.length === 0),
    isAllClosed: response.total_holding_count > 0 && nonCashActive.length === 0,
    hasClosedHoldings: response.closed_holdings.length > 0,
    totalUnrealizedPnl:
      response.total_unrealized_pnl !== null
        ? microToFormatted(response.total_unrealized_pnl, 2)
        : DASH,
    totalGlobalValue: microToFormatted(response.total_global_value, 2),
    totalGlobalValueRaw: response.total_global_value,
    hasCashHolding,
  };
}
