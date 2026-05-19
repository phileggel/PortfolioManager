import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { Asset } from "@/bindings";
import { logger } from "@/lib/logger";
import { Button } from "@/ui/components/button/Button";
import { Dialog } from "@/ui/components/modal/Dialog";
import { AssetForm } from "../shared/AssetForm";
import { useEditAssetModal } from "./useEditAssetModal";

interface EditAssetModalProps {
  isOpen: boolean;
  onClose: () => void;
  asset: Asset | null;
}

export function EditAssetModal({ isOpen, onClose, asset }: EditAssetModalProps) {
  const { t } = useTranslation();

  useEffect(() => {
    logger.info("[EditAssetModal] mounted");
  }, []);
  const {
    formData,
    error,
    isSubmitting,
    duplicateWarning,
    handleChange,
    handleClassChange,
    handleExchangeChange,
    handleSubmit,
    categories,
  } = useEditAssetModal({ asset, onClose });

  const isSubmitDisabled =
    !formData.name.trim() || !formData.reference.trim() || !formData.currency.trim();

  const actions = (
    <>
      <Button variant="secondary" onClick={onClose}>
        {t("action.cancel")}
      </Button>
      <Button
        type="submit"
        form="edit-asset-form"
        variant="primary"
        loading={isSubmitting}
        disabled={isSubmitDisabled || isSubmitting}
      >
        {t("action.save")}
      </Button>
    </>
  );

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onClose}
      title={t("asset.edit_modal_title")}
      actions={actions}
      maxWidth="max-w-xl"
    >
      <form id="edit-asset-form" className="py-2" onSubmit={handleSubmit}>
        <AssetForm
          formData={formData}
          handleChange={handleChange}
          onClassChange={handleClassChange}
          onExchangeChange={handleExchangeChange}
          categories={categories}
          duplicateWarning={duplicateWarning}
          idPrefix="edit-asset"
        />
        {error && (
          <p role="alert" className="mt-3 text-sm text-m3-error">
            {t(error)}
          </p>
        )}
      </form>
    </Dialog>
  );
}
