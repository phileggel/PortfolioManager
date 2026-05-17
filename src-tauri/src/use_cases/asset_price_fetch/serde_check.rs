#[cfg(test)]
mod tests {
    use super::super::error::FetchPriceTask;
    use super::super::FetchAllAssetPricesError;
    use crate::context::account::AccountApplicationError;
    use crate::context::asset::AssetError;

    fn json(error: &FetchAllAssetPricesError) -> serde_json::Value {
        serde_json::to_value(error).expect("serialize")
    }

    #[test]
    fn use_case_variants_serialize_with_code() {
        let already = FetchAllAssetPricesError::Failure(FetchPriceTask::FetchAlreadyRunning);
        let empty = FetchAllAssetPricesError::Failure(FetchPriceTask::NoFetchableHoldings);
        let unknown = FetchAllAssetPricesError::Failure(FetchPriceTask::UnknownError);
        assert_eq!(
            json(&already),
            serde_json::json!({ "code": "FetchAlreadyRunning" })
        );
        assert_eq!(
            json(&empty),
            serde_json::json!({ "code": "NoFetchableHoldings" })
        );
        assert_eq!(
            json(&unknown),
            serde_json::json!({ "code": "UnknownError" })
        );
    }

    #[test]
    fn asset_wrapper_flattens_bc_code() {
        let wrapped = FetchAllAssetPricesError::Asset(AssetError::DatabaseError);
        assert_eq!(
            json(&wrapped),
            serde_json::json!({ "code": "DatabaseError" })
        );
    }

    #[test]
    fn account_wrapper_flattens_bc_code_with_payload() {
        let wrapped = FetchAllAssetPricesError::Account(AccountApplicationError::AccountNotFound {
            account_id: "abc".into(),
        });
        assert_eq!(
            json(&wrapped),
            serde_json::json!({ "code": "AccountNotFound", "account_id": "abc" })
        );
    }
}
