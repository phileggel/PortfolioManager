/// Account application-layer types (errors classified per Rule B':
/// raised at the service/use-case layer, not by an aggregate method).
pub mod error;

pub use error::{AccountApplicationError, CashRecordingError};
