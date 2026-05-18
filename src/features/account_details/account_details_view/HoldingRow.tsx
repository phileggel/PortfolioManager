import { useNavigate } from "@tanstack/react-router";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  DollarSign,
  History,
  Minus,
  Plus,
  Search,
} from "lucide-react";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "@/lib/store";
import { IconButton } from "@/ui/components/button/IconButton";
import { formatIsoDate } from "../shared/formatDate";
import { PnlCell } from "../shared/PnlCell";
import type { HoldingRowViewModel } from "../shared/presenter";
import type { ModalTarget, SellTarget } from "../shared/types";

type HoldingRowProps = {
  row: HoldingRowViewModel;
  accountId: string;
  onBuy: (target: ModalTarget) => void;
  onSell: (target: SellTarget) => void;
  onEnterPrice: (assetId: string) => void;
  onPriceHistory: (assetId: string) => void;
  /** Cash variant — open Deposit modal (CSH-091). */
  onDeposit?: () => void;
  /** Cash variant — open Withdrawal modal (CSH-091). */
  onWithdraw?: () => void;
};

export function HoldingRow({
  row,
  accountId,
  onBuy,
  onSell,
  onEnterPrice,
  onPriceHistory,
  onDeposit,
  onWithdraw,
}: HoldingRowProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const assets = useAppStore((state) => state.assets);
  const accounts = useAppStore((state) => state.accounts);

  const buildTarget = useCallback((): ModalTarget => {
    const asset = assets.find((a) => a.id === row.assetId);
    const account = accounts.find((a) => a.id === accountId);
    return {
      accountName: account?.name ?? accountId,
      assetId: row.assetId,
      assetName: row.assetName,
      assetCurrency: asset?.currency ?? "",
      showExchangeRate: asset && account ? asset.currency !== account.currency : false,
    };
  }, [assets, accounts, accountId, row]);

  const handleBuy = useCallback(() => {
    onBuy(buildTarget());
  }, [onBuy, buildTarget]);

  const handleSell = useCallback(() => {
    onSell({ ...buildTarget(), holdingQuantityMicro: row.quantityMicro });
  }, [onSell, buildTarget, row.quantityMicro]);

  const handleViewTransactions = useCallback(() => {
    navigate({
      to: "/accounts/$accountId/transactions/$assetId",
      params: { accountId, assetId: row.assetId },
      search: { pendingTransactionAssetId: undefined },
    });
  }, [navigate, accountId, row.assetId]);

  const handleEnterPrice = useCallback(() => {
    onEnterPrice(row.assetId);
  }, [onEnterPrice, row.assetId]);

  const handlePriceHistory = useCallback(() => {
    onPriceHistory(row.assetId);
  }, [onPriceHistory, row.assetId]);

  const asset = assets.find((a) => a.id === row.assetId);
  const isArchived = asset?.is_archived ?? false;

  // CSH-091 — cash row variant: no Buy/Sell/Inspect, only Deposit/Withdraw.
  if (row.isCash) {
    return (
      <tr className="m3-tr">
        <td className="m3-td">
          <div className="flex flex-col">
            <span className="font-medium text-m3-on-surface">{row.assetName}</span>
            <span className="text-xs text-m3-on-surface-variant">{row.assetReference}</span>
          </div>
        </td>
        <td className="m3-td text-right tabular-nums font-medium">{row.quantity}</td>
        <td className="m3-td" />
        <td className="m3-td" />
        <td className="m3-td" />
        <td className="m3-td" />
        <td className="m3-td" />
        <td className="m3-td" />
        <td className="m3-td">
          <div className="flex items-center gap-1">
            <IconButton
              icon={<ArrowDownToLine size={16} />}
              variant="success"
              size="sm"
              aria-label={t("cash.action_record_deposit")}
              onClick={onDeposit}
            />
            <IconButton
              icon={<ArrowUpFromLine size={16} />}
              variant="error"
              size="sm"
              aria-label={t("cash.action_record_withdrawal")}
              onClick={onWithdraw}
              disabled={row.quantityMicro <= 0}
            />
          </div>
        </td>
      </tr>
    );
  }

  return (
    <tr className="m3-tr">
      <td className="m3-td">
        <div className="flex flex-col">
          <span className="font-medium text-m3-on-surface">{row.assetName}</span>
          <span className="text-xs text-m3-on-surface-variant">{row.assetReference}</span>
        </div>
      </td>
      <td className="m3-td text-right tabular-nums">{row.quantity}</td>
      <td className="m3-td text-right tabular-nums">{row.averagePrice}</td>
      <td className="m3-td text-right tabular-nums font-medium">{row.costBasis}</td>
      {/* SEL-042 — Realized P&L */}
      <td className="m3-td text-right">
        <PnlCell value={row.realizedPnl} raw={row.realizedPnlRaw} />
      </td>
      {/* MKT-030 — Current price; MKT-140 staleness; MKT-142 source badge */}
      <td className="m3-td text-right tabular-nums">
        {row.currentPrice !== "—" ? (
          <div className="flex flex-col items-end gap-0.5">
            <span>{row.currentPrice}</span>
            {row.currentPriceDate && (
              <span className="text-xs text-m3-on-surface-variant">
                {t("account_details.price_as_of", {
                  date: formatIsoDate(row.currentPriceDate ?? ""),
                })}
              </span>
            )}
            <div className="flex items-center gap-1.5">
              {row.sourceLabel && (
                <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-m3-surface-container-highest text-m3-on-surface-variant">
                  {t(row.sourceLabel)}
                </span>
              )}
              {row.staleness && (
                <span className="text-[10px] text-m3-on-surface-variant">
                  {t(row.staleness.key, row.staleness.params)}
                </span>
              )}
            </div>
          </div>
        ) : (
          <span className="text-m3-on-surface-variant">{row.currentPrice}</span>
        )}
      </td>
      {/* MKT-032/034 — Unrealized P&L */}
      <td className="m3-td text-right">
        {row.unrealizedPnl !== "—" ? (
          <PnlCell value={row.unrealizedPnl} raw={row.unrealizedPnlRaw ?? 0} />
        ) : (
          <span className="text-m3-on-surface-variant">{row.unrealizedPnl}</span>
        )}
      </td>
      {/* MKT-035 — Performance % */}
      <td className="m3-td text-right tabular-nums">
        {row.performancePct !== "—" ? (
          <span
            className={
              row.unrealizedPnlRaw !== null && row.unrealizedPnlRaw < 0
                ? "text-m3-error"
                : "text-m3-success"
            }
          >
            {row.performancePct}
          </span>
        ) : (
          <span className="text-m3-on-surface-variant">{row.performancePct}</span>
        )}
      </td>
      <td className="m3-td">
        <div className="flex items-center gap-1">
          {/* TRX-041 — Buy modal from holding row */}
          <IconButton
            icon={<Plus size={16} />}
            variant="success"
            size="sm"
            aria-label={t("transaction.action_buy")}
            onClick={handleBuy}
          />
          {/* SEL-010 — Sell button; disabled when asset is archived (SEL-037) */}
          <IconButton
            icon={<Minus size={16} />}
            variant="error"
            size="sm"
            aria-label={t("transaction.action_sell")}
            onClick={handleSell}
            disabled={isArchived}
          />
          {/* MKT-010 — Enter price button (active holdings only) */}
          {row.canEnterPrice && (
            <IconButton
              icon={<DollarSign size={16} />}
              size="sm"
              aria-label={t("account_details.action_enter_price")}
              onClick={handleEnterPrice}
            />
          )}
          {/* MKT-070 — Price history button (active holdings only) */}
          {row.canEnterPrice && (
            <IconButton
              icon={<History size={16} />}
              size="sm"
              aria-label={t("account_details.action_price_history")}
              onClick={handlePriceHistory}
            />
          )}
          <IconButton
            icon={<Search size={16} />}
            size="sm"
            aria-label={t("transaction.list_title")}
            onClick={handleViewTransactions}
          />
        </div>
      </td>
    </tr>
  );
}
