import { useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { logger } from "@/lib/logger";
import { Button } from "@/ui/components/button/Button";
import { FAB } from "@/ui/components/fab/FAB";
import { ManagerLayout } from "@/ui/components/layout/ManagerLayout";
import { AccountTable } from "./account_table/AccountTable";
import { AddAccountModal } from "./add_account/AddAccountModal";
import { useRefreshGlobalPrices } from "./refresh_prices/useRefreshGlobalPrices";

export function AccountManager() {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const navigate = useNavigate();
  const { isPending: isRefreshPending, refresh: refreshPrices } = useRefreshGlobalPrices();
  const handleAccountClick = useCallback(
    (id: string) => navigate({ to: "/accounts/$accountId", params: { accountId: id } }),
    [navigate],
  );

  useEffect(() => {
    logger.info("[AccountManager] mounted");
  }, []);

  return (
    <>
      <ManagerLayout
        searchId="account-search"
        searchTerm={query}
        onSearchChange={setQuery}
        searchPlaceholder={t("account.search_placeholder")}
        searchExtra={
          <Button
            id="account-manager-refresh-prices"
            variant="tonal"
            size="sm"
            loading={isRefreshPending}
            onClick={() => void refreshPrices()}
            aria-label={t("account.refresh_prices")}
          >
            {t("account.refresh_prices")}
          </Button>
        }
        table={<AccountTable searchTerm={query} onAccountClick={handleAccountClick} />}
      />
      {/* R14 — FAB opens add modal */}
      <FAB onClick={() => setIsAddModalOpen(true)} label={t("account.fab_label")} />
      <AddAccountModal isOpen={isAddModalOpen} onClose={() => setIsAddModalOpen(false)} />
    </>
  );
}
