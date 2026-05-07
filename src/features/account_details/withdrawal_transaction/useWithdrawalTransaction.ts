import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { logger } from "@/lib/logger";
import { decimalToMicro, microToFormatted } from "@/lib/microUnits";
import { useSnackbar } from "@/lib/snackbarStore";
import { accountDetailsGateway } from "../gateway";
import { validateAmount, validateDate } from "../shared/validateCashForm";

interface UseWithdrawalTransactionProps {
  accountId: string;
  onSubmitSuccess?: () => void;
}

interface WithdrawalFormData {
  date: string;
  amount: string;
  note: string;
}

const today = () => new Date().toISOString().slice(0, 10);

export function useWithdrawalTransaction({
  accountId,
  onSubmitSuccess,
}: UseWithdrawalTransactionProps) {
  const { t } = useTranslation();
  const showSnackbar = useSnackbar();

  const [formData, setFormData] = useState<WithdrawalFormData>(() => ({
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

  const handleChange = useCallback((field: keyof WithdrawalFormData, value: string) => {
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
        const result = await accountDetailsGateway.recordWithdrawal({
          account_id: accountId,
          date: formData.date,
          amount_micros: decimalToMicro(formData.amount),
          note: formData.note || null,
        });
        if (result.status === "error") {
          logger.error("[useWithdrawalTransaction] recordWithdrawal failed", {
            error: result.error,
          });
          // CSH-081 — InsufficientCash carries balance + currency for inline display.
          if (result.error.code === "InsufficientCash") {
            setError(
              t("cash.insufficient_cash_inline", {
                balance: microToFormatted(result.error.current_balance_micros, 2),
                currency: result.error.currency,
              }),
            );
            return;
          }
          const code = result.error.code;
          setError(t(`error.${code}`, { defaultValue: code }));
          return;
        }
        showSnackbar(t("cash.withdrawal_recorded"), "success");
        onSubmitSuccess?.();
      } catch (e) {
        logger.error("Failed to record withdrawal", { error: e });
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
