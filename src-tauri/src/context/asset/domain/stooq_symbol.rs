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
}
