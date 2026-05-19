//! Asset Web Lookup orchestrator — issues HTTP calls to OpenFIGI and delegates
//! ranking, dedup, and exchange-name resolution to
//! [`primary_listing_processor`] (WEB-014, WEB-022, WEB-049, WEB-050).

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::result::Result as StdResult;
use std::sync::Arc;

use super::error::WebLookupApplicationError;
use super::primary_listing_processor::{self, AssetLookupResult, QueryContext, RawFigiHit};
use crate::core::logger::BACKEND;

// ---------------------------------------------------------------------------
// OpenFigiClient trait (allows test mocking per B26)
// ---------------------------------------------------------------------------

/// Abstraction over the OpenFIGI HTTP API. Concrete production implementation
/// is [`ReqwestOpenFigiClient`]; tests use the `mockall`-generated mock.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait OpenFigiClient: Send + Sync {
    /// `/v3/mapping` with `idType=ID_ISIN` (WEB-014).
    async fn map_isin(&self, isin: &str) -> Result<Vec<RawFigiHit>>;

    /// `/v3/search` keyword endpoint, with `securityType: "Common Stock"` filter
    /// applied (WEB-050 step 1).
    async fn search_keyword(&self, query: &str) -> Result<Vec<RawFigiHit>>;

    /// Batched `/v3/mapping` call with `idType=ID_BB_GLOBAL_SHARE_CLASS_LEVEL`,
    /// returning all known listings for each share class (WEB-050 step 4).
    /// Result order matches the input id order.
    async fn map_share_classes(&self, ids: &[String]) -> Result<Vec<Vec<RawFigiHit>>>;
}

// ---------------------------------------------------------------------------
// AssetWebLookupUseCase
// ---------------------------------------------------------------------------

/// Orchestrates the OpenFIGI lookup: routes the query (WEB-014), fans out to
/// the share-class-mapping endpoint on the keyword path (WEB-050), then hands
/// the raw hits to [`primary_listing_processor::process_hits`] for dedup and
/// primary-listing pick. The final result is truncated to 10 entries (WEB-022).
pub struct AssetWebLookupUseCase {
    client: Arc<dyn OpenFigiClient>,
}

impl AssetWebLookupUseCase {
    /// Creates a new use case backed by the provided [`OpenFigiClient`].
    pub fn new(client: Arc<dyn OpenFigiClient>) -> Self {
        Self { client }
    }

    /// Searches OpenFIGI for instruments matching `query`.
    ///
    /// Routing rule (WEB-014): exactly 12 ASCII-alphanumeric chars → ISIN path;
    /// everything else → keyword path. The keyword path issues a second HTTP
    /// call (WEB-050) to enrich each unique share class with its full set of
    /// listings, so primary venues missing from `/v3/search` (notably Euronext
    /// Paris for European stocks) are surfaced.
    ///
    /// Any client error is surfaced as `WebLookupApplicationError::NetworkError`
    /// (WEB-025); the full diagnostic chain is preserved server-side via
    /// `tracing::warn!` at the translation site.
    pub async fn search(
        &self,
        query: String,
    ) -> StdResult<Vec<AssetLookupResult>, WebLookupApplicationError> {
        let trimmed = query.trim();
        let is_isin = trimmed.len() == 12 && trimmed.chars().all(|c| c.is_ascii_alphanumeric());

        let ctx = QueryContext {
            isin: if is_isin {
                Some(trimmed.to_string())
            } else {
                None
            },
        };

        let raw_hits = if is_isin {
            self.client.map_isin(trimmed).await.map_err(|e| {
                tracing::warn!(target: BACKEND, query = %trimmed, err = ?e, "search: ISIN lookup failed (WEB-025)");
                WebLookupApplicationError::NetworkError
            })?
        } else {
            self.collect_keyword_hits(trimmed).await?
        };

        let mut results = primary_listing_processor::process_hits(raw_hits, &ctx);
        results.truncate(10);
        Ok(results)
    }

    /// Keyword path (WEB-050): does the initial `/v3/search` call, collects the
    /// unique non-null `share_class_figi` values, then batches them into a
    /// single `/v3/mapping` call. Listings returned by mapping replace the
    /// initial keyword hits for each share class (mapping is the authoritative
    /// list of all venues for a share class). Hits with a null share class
    /// pass through to the processor, which drops them.
    async fn collect_keyword_hits(
        &self,
        query: &str,
    ) -> StdResult<Vec<RawFigiHit>, WebLookupApplicationError> {
        let initial = self.client.search_keyword(query).await.map_err(|e| {
            tracing::warn!(target: BACKEND, query = %query, err = ?e, "collect_keyword_hits: search_keyword failed (WEB-025)");
            WebLookupApplicationError::NetworkError
        })?;
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut share_class_ids: Vec<String> = Vec::new();
        for hit in &initial {
            if let Some(id) = hit.share_class_figi.as_deref() {
                if seen.insert(id) {
                    share_class_ids.push(id.to_string());
                }
            }
        }
        if share_class_ids.is_empty() {
            return Ok(initial);
        }
        let enriched = self
            .client
            .map_share_classes(&share_class_ids)
            .await
            .map_err(|e| {
                tracing::warn!(target: BACKEND, query = %query, err = ?e, "collect_keyword_hits: map_share_classes failed (WEB-025)");
                WebLookupApplicationError::NetworkError
            })?;
        Ok(enriched.into_iter().flatten().collect())
    }
}

// ---------------------------------------------------------------------------
// OpenFIGI HTTP response types (private deserialization structs)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OpenFigiHit {
    name: String,
    ticker: Option<String>,
    #[serde(rename = "securityType")]
    security_type: Option<String>,
    currency: Option<String>,
    #[serde(rename = "exchCode")]
    exchange_code: Option<String>,
    #[serde(rename = "micCode")]
    mic_code: Option<String>,
    #[serde(rename = "shareClassFIGI")]
    share_class_figi: Option<String>,
    #[serde(rename = "compositeFIGI")]
    composite_figi: Option<String>,
}

/// One item in the `/v3/mapping` response array.
#[derive(Deserialize)]
struct MappingResultItem {
    data: Option<Vec<OpenFigiHit>>,
}

/// The `/v3/search` response envelope.
#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<OpenFigiHit>,
}

// ---------------------------------------------------------------------------
// ReqwestOpenFigiClient — production HTTP implementation
// ---------------------------------------------------------------------------

const MAP_URL: &str = "https://api.openfigi.com/v3/mapping";
const SEARCH_URL: &str = "https://api.openfigi.com/v3/search";

/// Production [`OpenFigiClient`] backed by `reqwest` with rustls (WEB-021 — no
/// API key).
pub struct ReqwestOpenFigiClient {
    client: reqwest::Client,
}

impl Default for ReqwestOpenFigiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestOpenFigiClient {
    /// Creates a new client using the system's default TLS configuration.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OpenFigiClient for ReqwestOpenFigiClient {
    async fn map_isin(&self, isin: &str) -> Result<Vec<RawFigiHit>> {
        let body = serde_json::json!([{"idType": "ID_ISIN", "idValue": isin}]);
        let resp = self
            .client
            .post(MAP_URL)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("OpenFIGI ISIN mapping request failed for ISIN: {isin}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("OpenFIGI mapping returned {}", resp.status());
        }

        let items: Vec<MappingResultItem> = resp
            .json()
            .await
            .context("failed to deserialize OpenFIGI mapping response")?;
        Ok(items
            .into_iter()
            .flat_map(|item| item.data.unwrap_or_default())
            .map(hit_to_raw)
            .collect())
    }

    async fn search_keyword(&self, query: &str) -> Result<Vec<RawFigiHit>> {
        // WEB-050 step 1: filter to Common Stock so bonds, futures, structured
        // products and warrants don't crowd out the equity rows we care about.
        let body = serde_json::json!({
            "query": query,
            "securityType": "Common Stock",
        });
        let resp = self
            .client
            .post(SEARCH_URL)
            .json(&body)
            .send()
            .await
            .context("OpenFIGI keyword search request failed")?;

        if !resp.status().is_success() {
            anyhow::bail!("OpenFIGI search returned {}", resp.status());
        }

        let search_resp: SearchResponse = resp
            .json()
            .await
            .context("failed to deserialize OpenFIGI search response")?;
        Ok(search_resp.data.into_iter().map(hit_to_raw).collect())
    }

    async fn map_share_classes(&self, ids: &[String]) -> Result<Vec<Vec<RawFigiHit>>> {
        let body: Vec<_> = ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "idType": "ID_BB_GLOBAL_SHARE_CLASS_LEVEL",
                    "idValue": id,
                })
            })
            .collect();
        let resp = self
            .client
            .post(MAP_URL)
            .json(&body)
            .send()
            .await
            .context("OpenFIGI share-class mapping request failed")?;

        if !resp.status().is_success() {
            anyhow::bail!("OpenFIGI share-class mapping returned {}", resp.status());
        }

        let items: Vec<MappingResultItem> = resp
            .json()
            .await
            .context("failed to deserialize OpenFIGI share-class mapping response")?;
        Ok(items
            .into_iter()
            .map(|item| {
                item.data
                    .unwrap_or_default()
                    .into_iter()
                    .map(hit_to_raw)
                    .collect()
            })
            .collect())
    }
}

fn hit_to_raw(h: OpenFigiHit) -> RawFigiHit {
    RawFigiHit {
        name: h.name,
        ticker: h.ticker,
        security_type: h.security_type,
        currency: h.currency,
        exchange_code: h.exchange_code,
        mic_code: h.mic_code,
        share_class_figi: h.share_class_figi,
        composite_figi: h.composite_figi,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::eq;

    fn raw_hit(
        name: &str,
        exchange: Option<&str>,
        share_class: Option<&str>,
        ticker: Option<&str>,
        currency: Option<&str>,
        security_type: Option<&str>,
    ) -> RawFigiHit {
        RawFigiHit {
            name: name.to_string(),
            ticker: ticker.map(str::to_string),
            security_type: security_type.map(str::to_string),
            currency: currency.map(str::to_string),
            exchange_code: exchange.map(str::to_string),
            mic_code: None,
            share_class_figi: share_class.map(str::to_string),
            composite_figi: None,
        }
    }

    // ------------------------------------------------------------------
    // WEB-014 routing
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn routes_12_alphanumeric_query_to_map_isin() {
        let isin = "US0378331005";
        let mut mock = MockOpenFigiClient::new();
        mock.expect_map_isin()
            .with(eq(isin))
            .times(1)
            .returning(|_| Ok(vec![]));
        mock.expect_search_keyword().times(0);
        mock.expect_map_share_classes().times(0);

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search(isin.to_string()).await.is_ok());
    }

    #[tokio::test]
    async fn routes_short_query_to_search_keyword() {
        let query = "AAPL";
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .with(eq(query))
            .times(1)
            .returning(|_| Ok(vec![]));
        mock.expect_map_isin().times(0);
        mock.expect_map_share_classes().times(0); // empty share-class set
        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search(query.to_string()).await.is_ok());
    }

    #[tokio::test]
    async fn routes_13_char_alphanumeric_to_search_keyword() {
        let query = "US03783310051";
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .with(eq(query))
            .times(1)
            .returning(|_| Ok(vec![]));
        mock.expect_map_isin().times(0);
        mock.expect_map_share_classes().times(0);
        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search(query.to_string()).await.is_ok());
    }

    #[tokio::test]
    async fn routes_query_with_dash_to_search_keyword() {
        let query = "US037833-005";
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .with(eq(query))
            .times(1)
            .returning(|_| Ok(vec![]));
        mock.expect_map_isin().times(0);
        mock.expect_map_share_classes().times(0);
        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search(query.to_string()).await.is_ok());
    }

    // ------------------------------------------------------------------
    // WEB-050 — keyword path triggers share-class enrichment
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn keyword_path_calls_map_share_classes_with_unique_ids() {
        let initial_hits = vec![
            raw_hit(
                "X",
                Some("UV"),
                Some("SC1"),
                None,
                None,
                Some("Common Stock"),
            ),
            raw_hit(
                "X",
                Some("XT"),
                Some("SC1"),
                None,
                None,
                Some("Common Stock"),
            ),
            raw_hit(
                "Y",
                Some("UN"),
                Some("SC2"),
                None,
                None,
                Some("Common Stock"),
            ),
            raw_hit("Z", Some("X1"), None, None, None, Some("Common Stock")), // null SC dropped
        ];

        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(move |_| Ok(initial_hits.clone()));
        mock.expect_map_share_classes()
            .withf(|ids| ids == ["SC1".to_string(), "SC2".to_string()])
            .times(1)
            .returning(|_| Ok(vec![vec![], vec![]]));
        mock.expect_map_isin().times(0);

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search("anything".to_string()).await.is_ok());
    }

    #[tokio::test]
    async fn keyword_path_skips_enrichment_when_no_share_class_present() {
        let initial_hits = vec![raw_hit(
            "X",
            Some("X1"),
            None,
            None,
            None,
            Some("Common Stock"),
        )];

        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(move |_| Ok(initial_hits.clone()));
        mock.expect_map_share_classes().times(0);

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search("anything".to_string()).await.is_ok());
    }

    #[tokio::test]
    async fn isin_path_does_not_call_map_share_classes() {
        let mut mock = MockOpenFigiClient::new();
        mock.expect_map_isin().times(1).returning(|_| {
            Ok(vec![raw_hit(
                "X",
                Some("FP"),
                Some("SC1"),
                Some("AI"),
                Some("EUR"),
                Some("Common Stock"),
            )])
        });
        mock.expect_search_keyword().times(0);
        mock.expect_map_share_classes().times(0);

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        assert!(uc.search("FR0000120073".to_string()).await.is_ok());
    }

    // ------------------------------------------------------------------
    // WEB-022 — final 10-row truncation
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn truncates_results_to_ten() {
        // Initial keyword response carries 15 distinct share classes; mapping
        // returns one populated hit per share class so the dedup pipeline
        // produces 15 candidate result rows. The final truncation must cap
        // them to 10.
        let initial: Vec<RawFigiHit> = (0..15)
            .map(|i| {
                raw_hit(
                    &format!("Fund {i}"),
                    Some("FP"),
                    Some(&format!("SC{i}")),
                    None,
                    None,
                    Some("Common Stock"),
                )
            })
            .collect();
        let enriched: Vec<Vec<RawFigiHit>> = initial
            .iter()
            .map(|h| {
                vec![raw_hit(
                    &h.name,
                    Some("FP"),
                    h.share_class_figi.as_deref(),
                    Some("TICK"),
                    Some("EUR"),
                    Some("Common Stock"),
                )]
            })
            .collect();

        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(move |_| Ok(initial.clone()));
        mock.expect_map_share_classes()
            .times(1)
            .returning(move |_| Ok(enriched.clone()));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let results = uc.search("fund".to_string()).await.unwrap();
        assert_eq!(results.len(), 10);
    }

    #[tokio::test]
    async fn empty_share_class_mapping_yields_empty_result() {
        let initial = vec![raw_hit(
            "X",
            Some("UV"),
            Some("SC1"),
            None,
            None,
            Some("Common Stock"),
        )];
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(move |_| Ok(initial.clone()));
        mock.expect_map_share_classes()
            .times(1)
            .returning(|_| Ok(vec![vec![]]));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let results = uc.search("anything".to_string()).await.unwrap();
        assert!(results.is_empty());
    }

    // ------------------------------------------------------------------
    // WEB-049 — exchange code resolution end-to-end
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn resolves_known_exchange_code_to_readable_name() {
        let isin = "FR0000120073";
        let hit = raw_hit(
            "AIR LIQUIDE SA",
            Some("FP"),
            Some("SC1"),
            Some("AI"),
            Some("EUR"),
            Some("Common Stock"),
        );
        let mut mock = MockOpenFigiClient::new();
        mock.expect_map_isin()
            .with(eq(isin))
            .times(1)
            .returning(move |_| Ok(vec![hit.clone()]));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let results = uc.search(isin.to_string()).await.unwrap();
        // exchange is now Option<Exchange>; verify the canonical code is resolved
        let exchange = results[0]
            .exchange
            .as_ref()
            .expect("exchange should be resolved for FP → XPAR");
        assert_eq!(exchange.code, "XPAR");
    }

    // ------------------------------------------------------------------
    // WEB-046 — reference field source
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn reference_is_input_isin_on_isin_path() {
        let isin = "US0378331005";
        let hit = raw_hit(
            "Apple Inc.",
            Some("UN"),
            Some("SC1"),
            Some("AAPL"),
            Some("USD"),
            Some("Common Stock"),
        );
        let mut mock = MockOpenFigiClient::new();
        mock.expect_map_isin()
            .with(eq(isin))
            .times(1)
            .returning(move |_| Ok(vec![hit.clone()]));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let results = uc.search(isin.to_string()).await.unwrap();
        assert_eq!(results[0].reference.as_deref(), Some(isin));
    }

    #[tokio::test]
    async fn reference_is_ticker_on_keyword_path_when_present() {
        let initial = vec![raw_hit(
            "Apple Inc.",
            Some("UN"),
            Some("SC1"),
            Some("AAPL"),
            Some("USD"),
            Some("Common Stock"),
        )];
        let enriched = vec![vec![raw_hit(
            "Apple Inc.",
            Some("UN"),
            Some("SC1"),
            Some("AAPL"),
            Some("USD"),
            Some("Common Stock"),
        )]];
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(move |_| Ok(initial.clone()));
        mock.expect_map_share_classes()
            .times(1)
            .returning(move |_| Ok(enriched.clone()));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let results = uc.search("apple".to_string()).await.unwrap();
        assert_eq!(results[0].reference.as_deref(), Some("AAPL"));
    }

    // ------------------------------------------------------------------
    // WEB-025 — error propagation
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn search_keyword_failure_translates_to_network_error() {
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("connection refused")));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let err = uc.search("AAPL".to_string()).await.unwrap_err();
        assert!(
            matches!(err, WebLookupApplicationError::NetworkError),
            "got: {err:?}"
        );
    }

    #[tokio::test]
    async fn map_share_classes_failure_translates_to_network_error() {
        let initial = vec![raw_hit(
            "X",
            Some("UV"),
            Some("SC1"),
            None,
            None,
            Some("Common Stock"),
        )];
        let mut mock = MockOpenFigiClient::new();
        mock.expect_search_keyword()
            .times(1)
            .returning(move |_| Ok(initial.clone()));
        mock.expect_map_share_classes()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("rate limited")));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let err = uc.search("anything".to_string()).await.unwrap_err();
        assert!(
            matches!(err, WebLookupApplicationError::NetworkError),
            "got: {err:?}"
        );
    }

    #[tokio::test]
    async fn map_isin_failure_translates_to_network_error() {
        let mut mock = MockOpenFigiClient::new();
        mock.expect_map_isin()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("HTTP 500")));

        let uc = AssetWebLookupUseCase::new(Arc::new(mock));
        let err = uc.search("FR0000120073".to_string()).await.unwrap_err();
        assert!(
            matches!(err, WebLookupApplicationError::NetworkError),
            "got: {err:?}"
        );
    }
}
