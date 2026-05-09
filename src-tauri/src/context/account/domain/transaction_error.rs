/// Typed errors for transaction domain validation (TRX-020).
///
/// Derives `Serialize + specta::Type + #[serde(tag = "code")]` so it can be exposed
/// at the Tauri boundary verbatim — boundary error types compose this enum via
/// untagged unions instead of redefining its variants (PR #5 review feedback).
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum TransactionDomainError {
    /// Date string could not be parsed as YYYY-MM-DD.
    #[error("Invalid date format — expected YYYY-MM-DD")]
    InvalidDate,
    /// Transaction date is in the future.
    #[error("Transaction date cannot be in the future")]
    DateInFuture,
    /// Transaction date is before 1900-01-01.
    #[error("Transaction date cannot be before 1900-01-01")]
    DateTooOld,
    /// Quantity is zero or negative.
    #[error("Quantity must be strictly positive")]
    QuantityNotPositive,
    /// Cash deposit/withdrawal amount was zero or negative (CSH-021/CSH-031).
    /// Cash-specific framing of the same TRX-020 constraint that surfaces as
    /// `QuantityNotPositive` for non-cash transactions; raised by the cash
    /// factories (`Transaction::new_deposit` / `new_withdrawal`) BEFORE the
    /// generic check, so the FE sees the cash-specific error code.
    #[error("Amount must be greater than 0")]
    AmountNotPositive,
    /// Unit price is negative.
    #[error("Unit price cannot be negative")]
    UnitPriceNegative,
    /// Fees amount is negative.
    #[error("Fees cannot be negative")]
    FeesNegative,
    /// Exchange rate is zero or negative.
    #[error("Exchange rate must be strictly positive")]
    ExchangeRateNotPositive,
    /// Total amount is zero or negative.
    #[error("Total amount must be strictly positive")]
    TotalAmountNotPositive,
}
