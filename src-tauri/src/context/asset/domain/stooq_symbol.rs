/// Derives the Stooq provider symbol using the 3-step MKT-110 precedence rule.
///
/// Precedence:
///   1. `exchange` is `Some` AND the mapper returns a non-empty suffix
///      → `{lowercase(reference)}.{suffix}` (exchange-qualified symbol)
///   2. `exchange` is `None` → `lowercase(reference)` (legacy fallback, preserves
///      US ticker happy path per ADR-008)
///   3. Mapper returns `None` or an empty suffix → `None` (asset skipped per MKT-114)
///
/// Returns `None` for empty or non-ASCII references regardless of the exchange.
pub fn derive_stooq_symbol_with_exchange(
    reference: &str,
    exchange: Option<&super::exchange::Exchange>,
) -> Option<String> {
    let base = derive_stooq_symbol(reference)?;
    match exchange {
        None => Some(base),
        Some(exchange) => {
            let suffix = super::stooq_exchange_mapper::exchange_to_stooq_suffix(exchange)?;
            Some(format!("{base}.{suffix}"))
        }
    }
}

/// Derives the Stooq provider symbol from an asset reference string (MKT-110, ADR-008).
///
/// Returns `None` when derivation is impossible (empty input, non-ASCII).
/// Returns `Some(symbol)` with the lowercased symbol on success.
/// If the reference already contains a `.` (Stooq-style `ticker.exchange`) it is
/// passed through lowercased without modification.
pub fn derive_stooq_symbol(reference: &str) -> Option<String> {
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.is_ascii() {
        return None;
    }
    Some(trimmed.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    // MKT-110 — bare ticker lowercased (e.g. "AAPL" -> "aapl")
    #[test]
    fn bare_ticker_is_lowercased() {
        let result = derive_stooq_symbol("AAPL");
        assert_eq!(result, Some("aapl".to_string()));
    }

    // MKT-110 — already-lowercase bare ticker passes through unchanged
    #[test]
    fn already_lowercase_bare_ticker_unchanged() {
        let result = derive_stooq_symbol("msft");
        assert_eq!(result, Some("msft".to_string()));
    }

    // MKT-110 — reference containing '.' (already Stooq-style) passes through lowercased
    #[test]
    fn stooq_style_reference_passes_through_lowercased() {
        let result = derive_stooq_symbol("TTE.FR");
        assert_eq!(result, Some("tte.fr".to_string()));
    }

    // MKT-110 — already-lowercase Stooq-style reference passes through unchanged
    #[test]
    fn stooq_style_already_lowercase_unchanged() {
        let result = derive_stooq_symbol("cdp.pa");
        assert_eq!(result, Some("cdp.pa".to_string()));
    }

    // MKT-110 — empty string returns None (unmappable)
    #[test]
    fn empty_reference_returns_none() {
        let result = derive_stooq_symbol("");
        assert!(
            result.is_none(),
            "expected None for empty input, got: {result:?}"
        );
    }

    // MKT-110 — non-ASCII reference returns None (unmappable)
    #[test]
    fn non_ascii_reference_returns_none() {
        let result = derive_stooq_symbol("日本電信電話");
        assert!(
            result.is_none(),
            "expected None for non-ASCII input, got: {result:?}"
        );
    }

    // MKT-110 — whitespace-only reference returns None (unmappable)
    #[test]
    fn whitespace_only_reference_returns_none() {
        let result = derive_stooq_symbol("   ");
        assert!(
            result.is_none(),
            "expected None for whitespace-only input, got: {result:?}"
        );
    }

    // --- derive_stooq_symbol_with_exchange tests (MKT-110 3-step precedence) ---

    fn xpar_exchange() -> crate::context::asset::Exchange {
        crate::context::asset::exchange::lookup("XPAR").expect("XPAR must be in the curated set")
    }

    fn non_curated_exchange() -> crate::context::asset::Exchange {
        crate::context::asset::Exchange {
            code: "XBOG".to_string(),
            label: "Bogus Exchange".to_string(),
        }
    }

    // MKT-110 step 1 — exchange set + mapper returns suffix → qualified symbol
    #[test]
    fn with_exchange_returns_qualified_symbol_when_exchange_set() {
        let result = derive_stooq_symbol_with_exchange("AI", Some(&xpar_exchange()));
        assert_eq!(
            result,
            Some("ai.fr".to_string()),
            "XPAR should produce \".fr\" suffix → \"ai.fr\""
        );
    }

    // MKT-110 step 1 — reference is uppercased at entry but lowercased in symbol
    #[test]
    fn with_exchange_lowercases_reference() {
        let result = derive_stooq_symbol_with_exchange("MC", Some(&xpar_exchange()));
        assert_eq!(
            result,
            Some("mc.fr".to_string()),
            "reference should be lowercased in the derived Stooq symbol"
        );
    }

    // MKT-110 step 2 — exchange absent → lowercase reference (legacy fallback)
    #[test]
    fn with_exchange_none_returns_lowercase_reference_only() {
        let result = derive_stooq_symbol_with_exchange("AAPL", None);
        assert_eq!(
            result,
            Some("aapl".to_string()),
            "no exchange → legacy fallback: lowercase reference only"
        );
    }

    // MKT-110 step 3 — exchange set but mapper returns None → None (mapper gap, MKT-114)
    #[test]
    fn with_exchange_mapper_gap_returns_none() {
        // A non-curated exchange bypasses AST-001 validation at this layer
        // (validation lives in Asset::validate). The mapper returns None for
        // any code not in its table, which triggers the MKT-114 skip.
        let result = derive_stooq_symbol_with_exchange("TICK", Some(&non_curated_exchange()));
        assert!(
            result.is_none(),
            "mapper gap (unknown exchange code) must return None → asset skipped per MKT-114, got: {result:?}"
        );
    }

    // MKT-110 — empty reference returns None regardless of exchange
    #[test]
    fn with_exchange_empty_reference_returns_none() {
        let result = derive_stooq_symbol_with_exchange("", Some(&xpar_exchange()));
        assert!(
            result.is_none(),
            "empty reference must return None, got: {result:?}"
        );
    }

    // MKT-110 — non-ASCII reference returns None regardless of exchange
    #[test]
    fn with_exchange_non_ascii_reference_returns_none() {
        let result = derive_stooq_symbol_with_exchange("日本電信電話", Some(&xpar_exchange()));
        assert!(
            result.is_none(),
            "non-ASCII reference must return None, got: {result:?}"
        );
    }

    // MKT-110 step 2 — US ticker (AAPL) without exchange uses legacy path, returns Some
    #[test]
    fn us_ticker_without_exchange_returns_some() {
        let result = derive_stooq_symbol_with_exchange("AAPL", None);
        assert!(
            result.is_some(),
            "US ticker without exchange must return Some via legacy fallback"
        );
        assert_eq!(result.unwrap(), "aapl");
    }
}
