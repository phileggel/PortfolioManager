import { useParams } from "@tanstack/react-router";
import { ArrowDownToLine, ArrowUpFromLine, Plus, RefreshCw } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { logger } from "@/lib/logger";
import { Button } from "@/ui/components/button/Button";
import { BuyTransactionModal } from "../buy_transaction/BuyTransactionModal";
import { DepositTransactionModal } from "../deposit_transaction/DepositTransactionModal";
import { OpenBalanceModal } from "../open_balance/OpenBalanceModal";
import { PriceHistoryModal } from "../price_history/PriceHistoryModal";
import { useRefreshAccountPrices } from "../refresh_prices/useRefreshAccountPrices";
import { SellTransactionModal } from "../sell_transaction/SellTransactionModal";
import { WithdrawalTransactionModal } from "../withdrawal_transaction/WithdrawalTransactionModal";
import { ClosedHoldingRow } from "./ClosedHoldingRow";
import { HoldingRow } from "./HoldingRow";
import { NoCashBanner } from "./NoCashBanner";
import { PriceModal } from "./PriceModal";
import { useAccountDetailsView } from "./useAccountDetailsView";

export function AccountDetailsView() {
  const { t } = useTranslation();
  const { accountId } = useParams({ from: "/accounts/$accountId" });
  const view = useAccountDetailsView(accountId);
  const { isPending: isRefreshPending, refresh: refreshPrices } =
    useRefreshAccountPrices(accountId);

  useEffect(() => {
    logger.info("[AccountDetailsView] mounted");
  }, []);

  return (
    <div className="flex h-full flex-col gap-4 overflow-hidden py-2 px-2">
      <div className="flex-1 flex flex-col min-w-0 bg-m3-surface-container rounded-[28px] shadow-elevation-1 overflow-hidden">
        {/* Summary header */}
        <div className="px-6 py-4 bg-m3-surface-container-high">
          {view.isLoading ? (
            <div className="h-4 w-32 bg-m3-surface-variant rounded animate-pulse" />
          ) : view.summary ? (
            <div className="flex items-center justify-between">
              <div className="flex gap-6 flex-wrap">
                <p className="text-sm text-m3-on-surface-variant">
                  {t("account_details.total_cost_basis")}:{" "}
                  <span className="font-semibold text-m3-on-surface">
                    {view.summary.totalCostBasis}
                  </span>
                </p>
                {view.summary.totalRealizedPnlRaw !== 0 && (
                  <p className="text-sm text-m3-on-surface-variant">
                    {t("account_details.total_realized_pnl")}:{" "}
                    <span
                      className={`font-semibold ${
                        view.summary.totalRealizedPnlRaw < 0 ? "text-m3-error" : "text-m3-success"
                      }`}
                    >
                      {view.summary.totalRealizedPnl}
                    </span>
                  </p>
                )}
                {/* MKT-041 — total unrealized P&L */}
                {view.summary.totalUnrealizedPnl !== "—" && (
                  <p className="text-sm text-m3-on-surface-variant">
                    {t("account_details.total_unrealized_pnl")}:{" "}
                    <span className="font-semibold text-m3-on-surface">
                      {view.summary.totalUnrealizedPnl}
                    </span>
                  </p>
                )}
                {/* CSH-094 — Global Value (cash + priced holdings, account currency) */}
                <p className="text-sm text-m3-on-surface-variant">
                  {t("account_details.total_global_value")}:{" "}
                  <span className="font-semibold text-m3-on-surface">
                    {view.summary.totalGlobalValue}
                  </span>
                </p>
              </div>
              {/* TRX-055 — open balance always accessible (migration tool for any account state) */}
              {/* ACD-036 — add transaction only when active holdings exist */}
              <div className="flex gap-2">
                {/* MKT-131 — per-account "Refresh prices" entry point */}
                <Button
                  id="account-details-refresh-prices"
                  variant="secondary"
                  size="sm"
                  icon={<RefreshCw size={14} />}
                  loading={isRefreshPending}
                  onClick={() => void refreshPrices()}
                  aria-label={t("account_details.action_refresh_prices")}
                >
                  {t("account_details.action_refresh_prices")}
                </Button>
                <Button variant="secondary" size="sm" onClick={view.handleOpenBalanceOpen}>
                  {t("account_details.action_open_balance")}
                </Button>
                {/* CSH-019 — Deposit always visible */}
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<ArrowDownToLine size={14} />}
                  onClick={view.handleDepositOpen}
                >
                  {t("account_details.action_deposit")}
                </Button>
                {/* CSH-019 — Withdraw only when there is cash to withdraw */}
                {view.hasVisibleCashRow && (
                  <Button
                    variant="secondary"
                    size="sm"
                    icon={<ArrowUpFromLine size={14} />}
                    onClick={view.handleWithdrawalOpen}
                  >
                    {t("account_details.action_withdraw")}
                  </Button>
                )}
                {!view.summary.isEmpty && !view.summary.isAllClosed && (
                  <Button
                    variant="tonal"
                    size="sm"
                    icon={<Plus size={14} />}
                    onClick={view.handleAddTransaction}
                  >
                    {t("account_details.add_transaction")}
                  </Button>
                )}
              </div>
            </div>
          ) : null}
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto">
          {view.isLoading ? (
            /* ACD-037 — loading skeletons */
            <div className="animate-pulse p-4 space-y-3">
              {[1, 2, 3].map((i) => (
                <div key={i} className="h-10 bg-m3-surface-variant rounded-lg" />
              ))}
            </div>
          ) : view.error ? (
            /* ACD-038 — error state */
            <div className="flex flex-col items-center justify-center h-full gap-3 py-12">
              <span className="text-m3-error text-sm">{t("account_details.error_load")}</span>
              <Button variant="secondary" size="sm" onClick={view.retry}>
                {t("action.retry")}
              </Button>
            </div>
          ) : view.summary?.isEmpty && !view.hasVisibleCashRow ? (
            /* ACD-034 / CSH-098 — empty state only when there are no positions
               AND no cash row to display. With a cash holding present, the
               cash row renders even though `isEmpty` excludes cash from its
               own count (CSH-098 gating intent). */
            <div className="flex flex-col items-center justify-center h-full gap-4 py-12">
              <p className="text-m3-on-surface-variant italic">
                {t("account_details.empty_no_positions")}
              </p>
              {/* ACD-035 — empty state CTA */}
              <Button
                variant="primary"
                size="sm"
                icon={<Plus size={14} />}
                onClick={view.handleAddTransaction}
              >
                {t("account_details.add_transaction")}
              </Button>
            </div>
          ) : (
            <div className="flex flex-col">
              {/* CSH-095 — no-cash banner above the active holdings table */}
              {view.showNoCashBanner && <NoCashBanner onRecordDeposit={view.handleDepositOpen} />}

              {/* Active holdings table */}
              {view.hasActiveHoldings && (
                <div className="m3-table-container">
                  <table className="w-full border-collapse">
                    <thead className="sticky top-0 bg-m3-surface-container z-10">
                      <tr>
                        <th className="m3-th">{t("account_details.column_asset")}</th>
                        <th className="m3-th text-right">{t("account_details.column_quantity")}</th>
                        <th className="m3-th text-right">
                          {t("account_details.column_avg_price")}
                        </th>
                        <th className="m3-th text-right">
                          {t("account_details.column_cost_basis")}
                        </th>
                        {/* SEL-042 — Realized P&L column */}
                        <th className="m3-th text-right">
                          {t("account_details.column_realized_pnl")}
                        </th>
                        {/* MKT-030 — Current price column */}
                        <th className="m3-th text-right">
                          {t("account_details.column_current_price")}
                        </th>
                        {/* MKT-032/034 — Unrealized P&L column */}
                        <th className="m3-th text-right">
                          {t("account_details.column_unrealized_pnl")}
                        </th>
                        {/* MKT-035 — Performance % column */}
                        <th className="m3-th text-right">
                          {t("account_details.column_performance_pct")}
                        </th>
                        <th className="m3-th">{t("transaction.column_actions")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {view.holdings.map((row) => (
                        <HoldingRow
                          key={row.assetId}
                          row={row}
                          accountId={accountId}
                          onBuy={view.handleBuyOpen}
                          onSell={view.handleSellOpen}
                          onEnterPrice={view.handleEnterPrice}
                          onPriceHistory={view.handlePriceHistory}
                          onDeposit={view.handleDepositOpen}
                          onWithdraw={view.handleWithdrawalOpen}
                        />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              {/* ACD-048 — Closed positions section */}
              {view.hasClosedHoldings && (
                <div className="mt-2">
                  <div className="px-6 py-3 bg-m3-surface-container-high">
                    <h3 className="text-sm font-semibold text-m3-on-surface-variant uppercase tracking-wide">
                      {t("account_details.closed_positions_header")}
                    </h3>
                  </div>
                  <table className="w-full border-collapse">
                    <thead className="sticky top-0 bg-m3-surface-container z-10">
                      <tr>
                        <th className="m3-th">{t("account_details.column_asset")}</th>
                        {/* ACD-049 — P&L and last sold date */}
                        <th className="m3-th text-right">
                          {t("account_details.column_realized_pnl")}
                        </th>
                        <th className="m3-th text-right">
                          {t("account_details.column_last_sold_date")}
                        </th>
                        <th className="m3-th">{t("transaction.column_actions")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {view.closedHoldings.map((row) => (
                        <ClosedHoldingRow key={row.assetId} row={row} accountId={accountId} />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              {/* No active holdings but has closed — show CTA */}
              {!view.hasActiveHoldings && (
                <div className="flex flex-col items-center justify-center gap-4 py-8">
                  <p className="text-m3-on-surface-variant italic">
                    {t("account_details.empty_all_closed")}
                  </p>
                  <Button
                    variant="primary"
                    size="sm"
                    icon={<Plus size={14} />}
                    onClick={view.handleAddTransaction}
                  >
                    {t("account_details.add_transaction")}
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* TRX-041 — Buy modal from holding row */}
      {view.buyTarget && (
        <BuyTransactionModal
          isOpen
          onClose={view.handleBuyClose}
          accountId={accountId}
          accountName={view.buyTarget.accountName}
          assetId={view.buyTarget.assetId}
          assetName={view.buyTarget.assetName}
          assetCurrency={view.buyTarget.assetCurrency}
          showExchangeRate={view.buyTarget.showExchangeRate}
          onSubmitSuccess={view.handleBuySuccess}
        />
      )}

      {/* SEL-010 — Sell modal */}
      {view.sellTarget && (
        <SellTransactionModal
          isOpen
          onClose={view.handleSellClose}
          accountId={accountId}
          accountName={view.sellTarget.accountName}
          assetId={view.sellTarget.assetId}
          assetName={view.sellTarget.assetName}
          assetCurrency={view.sellTarget.assetCurrency}
          holdingQuantityMicro={view.sellTarget.holdingQuantityMicro}
          showExchangeRate={view.sellTarget.showExchangeRate}
          onSubmitSuccess={view.handleSellSuccess}
        />
      )}

      {/* MKT-010 — Enter price modal */}
      {view.priceTarget && (
        <PriceModal
          isOpen
          onClose={view.handlePriceClose}
          holding={view.priceTarget}
          onSubmitSuccess={view.handlePriceSuccess}
        />
      )}

      {/* MKT-072 — Price history modal */}
      {view.historyTarget && (
        <PriceHistoryModal isOpen onClose={view.handleHistoryClose} holding={view.historyTarget} />
      )}

      {/* TRX-055 — Open balance modal (account pre-filled, user picks asset inside) */}
      <OpenBalanceModal
        isOpen={view.openBalanceOpen}
        onClose={view.handleOpenBalanceClose}
        accountId={accountId}
        accountName={view.summary?.accountName ?? ""}
        assetId=""
        assetName=""
        onSubmitSuccess={view.handleOpenBalanceSuccess}
      />

      {/* CSH-022 — Deposit modal */}
      <DepositTransactionModal
        isOpen={view.depositOpen}
        onClose={view.handleDepositClose}
        accountId={accountId}
        accountName={view.summary?.accountName ?? ""}
        accountCurrency={view.accountCurrency}
        onSubmitSuccess={view.handleDepositSuccess}
      />

      {/* CSH-032 — Withdrawal modal */}
      <WithdrawalTransactionModal
        isOpen={view.withdrawalOpen}
        onClose={view.handleWithdrawalClose}
        accountId={accountId}
        accountName={view.summary?.accountName ?? ""}
        accountCurrency={view.accountCurrency}
        onSubmitSuccess={view.handleWithdrawalSuccess}
      />
    </div>
  );
}
