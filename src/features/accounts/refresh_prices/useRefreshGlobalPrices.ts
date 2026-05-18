import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSnackbar } from "@/lib/snackbarStore";
import { accountGateway } from "../gateway";

/**
 * MKT-115 / MKT-133 — Global "Refresh prices" hook for the AccountManager header.
 *
 * Calls `accountGateway.fetchAllAssetPrices()` and surfaces the result via the global snackbar:
 * - success → `mkt.fetch_dispatched`
 * - `FetchAlreadyRunning` → `mkt.fetch_already_running`
 * - `NoFetchableHoldings` → `mkt.fetch_no_holdings`
 * - `DatabaseError` / `UnknownError` → `error.DatabaseError`
 *
 * `isPending` toggles for the duration of the gateway call so the button can disable itself.
 */
export function useRefreshGlobalPrices(): {
  isPending: boolean;
  refresh: () => Promise<void>;
} {
  const [isPending, setIsPending] = useState(false);
  const showSnackbar = useSnackbar();
  const { t } = useTranslation();

  const refresh = useCallback(async () => {
    setIsPending(true);
    try {
      const result = await accountGateway.fetchAllAssetPrices();
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
        default:
          showSnackbar(t("error.DatabaseError"), "error");
      }
    } finally {
      setIsPending(false);
    }
  }, [showSnackbar, t]);

  return { isPending, refresh };
}
