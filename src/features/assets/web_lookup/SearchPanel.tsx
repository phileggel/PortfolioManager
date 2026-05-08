import { useTranslation } from "react-i18next";
import type { AssetLookupResult } from "@/bindings";
import { formatAssetClass } from "@/features/assets/shared/presenter";
import { Button } from "@/ui/components/button/Button";
import { TextField } from "@/ui/components/field/TextField";
import type { WebLookupSearchState } from "./useWebLookupSearch";

interface SearchPanelProps {
  query: string;
  setQuery: (q: string) => void;
  state: WebLookupSearchState;
  submit: () => void;
  retry: () => void;
  onSelect: (result: AssetLookupResult) => void;
  onFillManually: () => void;
}

export function SearchPanel({
  query,
  setQuery,
  state,
  submit,
  retry,
  onSelect,
  onFillManually,
}: SearchPanelProps) {
  const { t } = useTranslation();
  const isLoading = state.status === "loading";

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    submit();
  };

  return (
    <div className="flex flex-col gap-4">
      <form id="web-lookup-search-form" onSubmit={handleSubmit} className="flex gap-2 items-end">
        <div className="flex-1">
          <TextField
            id="web-lookup-search-query"
            label={t("asset.web_lookup.query_label")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("asset.web_lookup.query_placeholder")}
            autoFocus
          />
        </div>
        <Button
          type="submit"
          form="web-lookup-search-form"
          variant="primary"
          disabled={!query.trim() || isLoading}
          loading={isLoading}
        >
          {t("asset.web_lookup.action_search")}
        </Button>
      </form>

      <div className="min-h-[80px] max-h-[60vh] overflow-y-auto">
        {state.status === "idle" && (
          <p className="text-sm text-m3-on-surface-variant">{t("asset.web_lookup.idle_hint")}</p>
        )}

        {state.status === "loading" && (
          <p aria-busy="true" className="text-sm text-m3-on-surface-variant">
            {t("asset.web_lookup.loading")}
          </p>
        )}

        {state.status === "empty" && (
          <p className="text-sm text-m3-on-surface-variant">{t("asset.web_lookup.no_results")}</p>
        )}

        {state.status === "error" && (
          <div role="alert" className="flex flex-col gap-2">
            <p className="text-sm text-m3-error">{t("asset.web_lookup.error_network")}</p>
            <Button
              variant="outline"
              size="sm"
              aria-label={t("asset.web_lookup.action_retry")}
              onClick={retry}
            >
              {t("asset.web_lookup.action_retry")}
            </Button>
          </div>
        )}

        {state.status === "results" && (
          <ul className="flex flex-col gap-1">
            {state.results.map((result) => {
              const typeLabel = result.asset_class
                ? formatAssetClass(result.asset_class, t)
                : t("asset.web_lookup.type_unknown");
              const secondLine = result.exchange ? `${typeLabel} · ${result.exchange}` : typeLabel;
              return (
                <li key={`${result.name}|${result.reference ?? ""}|${result.exchange ?? ""}`}>
                  <button
                    type="button"
                    aria-label={t("asset.web_lookup.select_result", {
                      name: result.name,
                    })}
                    onClick={() => onSelect(result)}
                    className="w-full text-left px-3 py-2 rounded-xl hover:bg-m3-surface-variant/40 transition-colors"
                  >
                    <div className="flex items-baseline gap-1.5">
                      {result.reference && (
                        <span className="text-xs font-mono text-m3-on-surface-variant shrink-0">
                          {result.reference}
                        </span>
                      )}
                      <span className="font-medium text-m3-on-surface text-sm truncate">
                        {result.name}
                      </span>
                    </div>
                    <div className="text-xs text-m3-on-surface-variant mt-0.5">{secondLine}</div>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="flex justify-start pt-1">
        <Button
          variant="ghost"
          size="sm"
          aria-label={t("asset.web_lookup.action_fill_manually")}
          onClick={onFillManually}
        >
          {t("asset.web_lookup.action_fill_manually")}
        </Button>
      </div>
    </div>
  );
}
