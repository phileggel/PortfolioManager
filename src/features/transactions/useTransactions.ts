import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import type {
  BuyHoldingDTO,
  CorrectTransactionDTO,
  HoldingTransactionError,
  SellHoldingDTO,
  Transaction,
} from "@/bindings";
import { logger } from "@/lib/logger";
import { microToFormatted } from "@/lib/microUnits";
import { transactionGateway } from "./gateway";

/**
 * Hook providing CRUD callbacks for the transaction feature.
 * Does not hold domain state — holdings are managed via the transaction store.
 */
export function useTransactions() {
  const { t } = useTranslation();

  // CSH-081 — InsufficientCash carries a balance + currency payload that must be
  // surfaced inline. All other errors round-trip via their legacy translation key.
  const formatError = useCallback(
    (err: HoldingTransactionError): string => {
      if (err.code === "InsufficientCash") {
        return t("cash.insufficient_cash_inline", {
          balance: microToFormatted(err.current_balance_micros, 2),
          currency: err.currency,
        });
      }
      return `error.${err.code}`;
    },
    [t],
  );

  const buyHolding = useCallback(
    async (dto: BuyHoldingDTO) => {
      try {
        const res = await transactionGateway.buyHolding(dto);
        if (res.status === "ok") {
          return { data: res.data, error: null };
        }
        return { data: null, error: formatError(res.error) };
      } catch (e) {
        logger.error("Failed to buy holding", { error: e });
        return { data: null, error: String(e) };
      }
    },
    [formatError],
  );

  const sellHolding = useCallback(
    async (dto: SellHoldingDTO) => {
      try {
        const res = await transactionGateway.sellHolding(dto);
        if (res.status === "ok") {
          return { data: res.data, error: null };
        }
        return { data: null, error: formatError(res.error) };
      } catch (e) {
        logger.error("Failed to sell holding", { error: e });
        return { data: null, error: String(e) };
      }
    },
    [formatError],
  );

  const correctTransaction = useCallback(
    async (id: string, accountId: string, dto: CorrectTransactionDTO) => {
      try {
        const res = await transactionGateway.correctTransaction(id, accountId, dto);
        if (res.status === "ok") {
          return { data: res.data, error: null };
        }
        return { data: null, error: formatError(res.error) };
      } catch (e) {
        logger.error("Failed to correct transaction", { error: e });
        return { data: null, error: String(e) };
      }
    },
    [formatError],
  );

  const cancelTransaction = useCallback(
    async (id: string, accountId: string) => {
      try {
        const res = await transactionGateway.cancelTransaction(id, accountId);
        if (res.status === "ok") {
          return { error: null };
        }
        return { error: formatError(res.error) };
      } catch (e) {
        logger.error("Failed to cancel transaction", { error: e });
        return { error: String(e) };
      }
    },
    [formatError],
  );

  const getTransactions = useCallback(
    async (accountId: string, assetId: string): Promise<Transaction[]> => {
      try {
        const res = await transactionGateway.getTransactions(accountId, assetId);
        if (res.status === "ok") {
          return res.data;
        }
        logger.error("Failed to get transactions", { error: res.error });
        return [];
      } catch (e) {
        logger.error("Failed to get transactions", { error: e });
        return [];
      }
    },
    [],
  );

  return {
    buyHolding,
    sellHolding,
    correctTransaction,
    cancelTransaction,
    getTransactions,
  };
}
