import { useEffect, useMemo, useState } from "react";
import type { Asset, AssetClass } from "@/bindings";
import { logger } from "@/lib/logger";
import { useAppStore } from "@/lib/store";
import { hasDuplicateReference } from "../shared/validateAsset";
import { useAssets } from "../useAssets";

interface UseEditAssetModalProps {
  asset: Asset | null;
  onClose: () => void;
}

export function useEditAssetModal({ asset, onClose }: UseEditAssetModalProps) {
  const { updateAsset, assets } = useAssets();
  const categories = useAppStore((s) => s.categories);

  const [formData, setFormData] = useState({
    name: "",
    reference: "",
    class: "Stocks" as AssetClass,
    currency: "USD",
    risk_level: 3,
    category_id: "",
  });
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Sync form data when asset changes
  useEffect(() => {
    if (asset) {
      setFormData({
        name: asset.name,
        reference: asset.reference,
        class: asset.class,
        currency: asset.currency,
        risk_level: asset.risk_level,
        category_id: asset.category.id,
      });
      setError(null);
    }
  }, [asset]);

  // Duplicate reference warning — R9 (excludes self, includes archived)
  const duplicateWarning = useMemo(
    () => hasDuplicateReference(formData.reference, assets, asset?.id),
    [formData.reference, assets, asset?.id],
  );

  const handleChange = (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => {
    const { name, value } = e.target;
    setFormData((prev) => ({
      ...prev,
      [name]: name === "risk_level" ? parseInt(value, 10) : value,
    }));
  };

  // R12: class change in edit mode does NOT auto-fill risk_level
  const handleClassChange = (_assetClass: AssetClass) => {
    // intentionally a no-op — risk_level suggestion only applies at creation (R10)
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!asset) return;

    setError(null);
    setIsSubmitting(true);
    const result = await updateAsset({
      asset_id: asset.id,
      name: formData.name,
      reference: formData.reference,
      class: formData.class,
      currency: formData.currency,
      risk_level: formData.risk_level,
      category_id: formData.category_id,
      exchange: asset.exchange,
    });

    setIsSubmitting(false);

    if (result.error) {
      logger.error("[useEditAssetModal] update failed", {
        error: result.error,
      });
      setError(result.error);
      return;
    }

    onClose();
  };

  return {
    formData,
    error,
    isSubmitting,
    duplicateWarning,
    handleChange,
    handleClassChange,
    handleSubmit,
    categories,
  };
}
