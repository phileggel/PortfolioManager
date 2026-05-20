/// Application-layer errors raised by the asset web-lookup use case (WEB-025).
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum WebLookupApplicationError {
    /// OpenFIGI returned HTTP 429 Too Many Requests — transient, recoverable
    /// after a short wait. Surfaced distinctly so the frontend can render
    /// retry-after-wait copy (WEB-033).
    #[error("Lookup service rate limit reached — wait a moment and retry")]
    RateLimited,
    /// Network unreachable, connection timeout, or any non-2xx HTTP status
    /// other than 429.
    #[error("Network error while contacting the lookup service")]
    NetworkError,
}
