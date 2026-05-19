/// Stooq outbound mapper: resolves a canonical `Exchange` to its Stooq venue
/// suffix (MKT-110, ADR-008).
///
/// This is a pure infrastructure adapter that translates between the asset BC's
/// canonical `Exchange` value object and the Stooq provider's URL suffix scheme.
/// Returns `None` when the exchange has no known Stooq mapping (mapper gap),
/// which causes the asset to be skipped per MKT-114.
use crate::context::asset::Exchange;

/// Maps a canonical `Exchange` to its Stooq venue suffix.
///
/// Returns `Some(suffix)` for exchanges in the curated-to-Stooq mapping table,
/// `None` for exchanges outside it (mapper gap → asset skipped per MKT-114).
pub fn exchange_to_stooq_suffix(exchange: &Exchange) -> Option<&'static str> {
    EXCHANGE_TO_STOOQ_SUFFIX
        .iter()
        .find(|(mic, _)| *mic == exchange.code)
        .map(|(_, suffix)| *suffix)
}

/// Canonical Exchange MIC → Stooq venue suffix. Mirrors Stooq's URL scheme:
/// the symbol component appears as `{ticker}.{suffix}`, e.g. `ai.fr` for
/// Air Liquide on Euronext Paris (XPAR).
const EXCHANGE_TO_STOOQ_SUFFIX: &[(&str, &str)] = &[
    ("XPAR", "fr"),
    ("XNAS", "us"),
    ("XNYS", "us"),
    ("XLON", "uk"),
    ("XETR", "de"),
    ("XAMS", "nl"),
    ("XBRU", "be"),
    ("XMIL", "it"),
    ("XMAD", "es"),
    ("XSWX", "ch"),
    ("XTSE", "ca"),
    ("XHKG", "hk"),
    ("XTKS", "jp"),
    ("XASX", "au"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn make_exchange(code: &str) -> Exchange {
        Exchange {
            code: code.to_string(),
            label: format!("Test exchange {code}"),
        }
    }

    // XPAR maps to the French Stooq suffix
    #[test]
    fn xpar_maps_to_fr() {
        let exchange = make_exchange("XPAR");
        let suffix = exchange_to_stooq_suffix(&exchange);
        assert_eq!(
            suffix,
            Some("fr"),
            "XPAR must map to \"fr\" (Stooq's French venue suffix)"
        );
    }

    // XETR maps to the German Stooq suffix
    #[test]
    fn xetr_maps_to_de() {
        let exchange = make_exchange("XETR");
        let suffix = exchange_to_stooq_suffix(&exchange);
        assert_eq!(
            suffix,
            Some("de"),
            "XETR must map to \"de\" (Stooq's German venue suffix)"
        );
    }

    // XNYS maps to the US Stooq suffix
    #[test]
    fn xnys_maps_to_us() {
        let exchange = make_exchange("XNYS");
        let suffix = exchange_to_stooq_suffix(&exchange);
        assert_eq!(
            suffix,
            Some("us"),
            "XNYS must map to \"us\" (Stooq's US venue suffix)"
        );
    }

    // XNAS maps to the US Stooq suffix
    #[test]
    fn xnas_maps_to_us() {
        let exchange = make_exchange("XNAS");
        let suffix = exchange_to_stooq_suffix(&exchange);
        assert_eq!(
            suffix,
            Some("us"),
            "XNAS must map to \"us\" (Stooq's US venue suffix)"
        );
    }

    // XLON maps to the UK Stooq suffix
    #[test]
    fn xlon_maps_to_uk() {
        let exchange = make_exchange("XLON");
        let suffix = exchange_to_stooq_suffix(&exchange);
        assert_eq!(
            suffix,
            Some("uk"),
            "XLON must map to \"uk\" (Stooq's UK venue suffix)"
        );
    }

    // every canonical exchange in the curated set returns Some
    #[test]
    fn all_canonical_exchanges_return_a_suffix() {
        use crate::context::asset::exchange::all;
        let exchanges = all();
        for exchange in &exchanges {
            let suffix = exchange_to_stooq_suffix(exchange);
            assert!(
                suffix.is_some(),
                "canonical exchange {} has no Stooq suffix mapping",
                exchange.code
            );
        }
    }

    // every returned suffix is non-empty
    #[test]
    fn all_canonical_exchanges_return_non_empty_suffix() {
        use crate::context::asset::exchange::all;
        let exchanges = all();
        for exchange in &exchanges {
            if let Some(suffix) = exchange_to_stooq_suffix(exchange) {
                assert!(
                    !suffix.is_empty(),
                    "exchange {} returned an empty Stooq suffix",
                    exchange.code
                );
            }
        }
    }

    // an exchange not in the curated set returns None (mapper gap)
    #[test]
    fn unknown_exchange_returns_none() {
        let unknown = make_exchange("BOGUS");
        let suffix = exchange_to_stooq_suffix(&unknown);
        assert!(
            suffix.is_none(),
            "unknown exchange BOGUS must return None (mapper gap), got: {suffix:?}"
        );
    }

    // an exchange with a valid-looking but non-curated MIC returns None
    #[test]
    fn non_curated_mic_format_returns_none() {
        let non_curated = make_exchange("XBOG");
        let suffix = exchange_to_stooq_suffix(&non_curated);
        assert!(
            suffix.is_none(),
            "XBOG is not in the curated set and must return None, got: {suffix:?}"
        );
    }
}
