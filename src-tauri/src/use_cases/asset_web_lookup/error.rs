/// Application-layer errors raised by the asset web-lookup use case.
///
/// Single variant — covers all failure modes: network unreachable, connection
/// timeout, and any non-2xx HTTP status (including rate-limiting responses)
/// from the OpenFIGI client (WEB-025).
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type, Clone)]
#[serde(tag = "code")]
pub enum WebLookupApplicationError {
    /// All network or HTTP-level failures.
    #[error("Network error while contacting the lookup service")]
    NetworkError,
}
