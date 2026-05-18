import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSnackbar } from "@/lib/snackbarStore";
import { accountDetailsGateway } from "../gateway";

/**
 * MKT-115 / MKT-131 / MKT-132 / MKT-133 — Per-account "Refresh prices" hook for the
 * AccountDetailsView header.
 *
 * Calls `accountDetailsGateway.fetchAccountAssetPrices(accountId)` and surfaces the result
 * via the global snackbar:
 * - success → `mkt.fetch_dispatched`
 * - `FetchAlreadyRunning` → `mkt.fetch_already_running`
 * - `NoFetchableHoldings` → `mkt.fetch_no_holdings`
 * - `AccountNotFound` → `error.AccountNotFound`
 * - `DatabaseError` / `UnknownError` → `error.DatabaseError`
 */
export function useRefreshAccountPrices(accountId: string): {
  isPending: boolean;
  refresh: () => Promise<void>;
} {
  const [isPending, setIsPending] = useState(false);
  const showSnackbar = useSnackbar();
  const { t } = useTranslation();

  const refresh = useCallback(async () => {
    setIsPending(true);
    try {
      const result = await accountDetailsGateway.fetchAccountAssetPrices(accountId);
      if (result.status === "ok") {
        showSnackbar(t("mkt.fetch_dispatched"), "info");
        return;
      }
      switch (result.error.code) {
        case "FetchAlreadyRunning":
          showSnackbar(t("mkt.fetch_already_running"), "info");
          return;
        case "NoFetchableHoldings":
          showSnackbar(t("mkt.fetch_no_holdings"), "info");
          return;
        case "AccountNotFound":
          showSnackbar(t("error.AccountNotFound"), "error");
          return;
        default:
          showSnackbar(t("error.DatabaseError"), "error");
      }
    } finally {
      setIsPending(false);
    }
  }, [accountId, showSnackbar, t]);

  return { isPending, refresh };
}
