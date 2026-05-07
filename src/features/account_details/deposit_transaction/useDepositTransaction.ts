import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { logger } from "@/lib/logger";
import { decimalToMicro } from "@/lib/microUnits";
import { useSnackbar } from "@/lib/snackbarStore";
import { accountDetailsGateway } from "../gateway";
import { validateAmount, validateDate } from "../shared/validateCashForm";

interface UseDepositTransactionProps {
  accountId: string;
  onSubmitSuccess?: () => void;
}

interface DepositFormData {
  date: string;
  amount: string;
  note: string;
}

const today = () => new Date().toISOString().slice(0, 10);

export function useDepositTransaction({ accountId, onSubmitSuccess }: UseDepositTransactionProps) {
  const { t } = useTranslation();
  const showSnackbar = useSnackbar();

  const [formData, setFormData] = useState<DepositFormData>(() => ({
    date: today(),
    amount: "",
    note: "",
  }));
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const isFormValid = useMemo(
    () => validateAmount(formData.amount) === null && validateDate(formData.date) === null,
    [formData.amount, formData.date],
  );

  const handleChange = useCallback((field: keyof DepositFormData, value: string) => {
    setFormData((prev) => ({ ...prev, [field]: value }));
  }, []);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      const amountErr = validateAmount(formData.amount);
      const dateErr = validateDate(formData.date);
      const validationError = amountErr ?? dateErr;
      if (validationError) {
        setError(t(validationError, { defaultValue: validationError }));
        return;
      }

      setError(null);
      setIsSubmitting(true);
      try {
        const result = await accountDetailsGateway.recordDeposit({
          account_id: accountId,
          date: formData.date,
          amount_micros: decimalToMicro(formData.amount),
          note: formData.note || null,
        });
        if (result.status === "error") {
          logger.error("[useDepositTransaction] recordDeposit failed", { error: result.error });
          const code = result.error.code;
          setError(t(`error.${code}`, { defaultValue: code }));
          return;
        }
        showSnackbar(t("cash.deposit_recorded"), "success");
        onSubmitSuccess?.();
      } catch (e) {
        logger.error("Failed to record deposit", { error: e });
        setError(String(e));
      } finally {
        setIsSubmitting(false);
      }
    },
    [accountId, formData, t, showSnackbar, onSubmitSuccess],
  );

  return {
    formData,
    error,
    isSubmitting,
    isFormValid,
    handleChange,
    handleSubmit,
  };
}
