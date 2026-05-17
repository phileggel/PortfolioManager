use crate::context::account::{AccountApplicationError, AccountService};
use crate::context::asset::{Asset, AssetApplicationError, AssetError, AssetService};
use crate::core::cash::system_cash_asset_id;
use crate::core::logger::BACKEND;
use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use std::sync::Arc;

use super::dispatcher::Dispatcher;
use super::error::FetchPriceTask;
use super::guard::FetchGuard;

/// Wire-facing error composite for `fetch_all_asset_prices` (MKT-113, MKT-111, MKT-122).
///
/// `#[serde(untagged)]` lets every arm surface its inner `{ "code": "..." }` payload
/// directly on the wire. Each arm carries a tagged inner type (BC enum or
/// `FetchPriceTask`) so the discriminator survives the untagging.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(untagged)]
pub enum FetchAllAssetPricesError {
    /// Propagates asset-BC failures (e.g. `DatabaseError`) via `?`.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Propagates account-BC failures (`AccountNotFound`, `DatabaseError`) via `?`.
    #[error(transparent)]
    Account(#[from] AccountApplicationError),
    /// Use-case-specific failures (`FetchAlreadyRunning`, `NoFetchableHoldings`, `UnknownError`).
    #[error(transparent)]
    Failure(#[from] FetchPriceTask),
}

/// Orchestrates the all-accounts auto-fetch task (MKT-122, MKT-130).
pub struct FetchAllAssetPricesUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
    fetch_guard: Arc<FetchGuard>,
    dispatcher: Arc<Dispatcher>,
}

impl FetchAllAssetPricesUseCase {
    /// Creates a new use case instance.
    pub fn new(
        account_service: Arc<AccountService>,
        asset_service: Arc<AssetService>,
        fetch_guard: Arc<FetchGuard>,
        dispatcher: Arc<Dispatcher>,
    ) -> Self {
        Self {
            account_service,
            asset_service,
            fetch_guard,
            dispatcher,
        }
    }

    /// Runs the all-accounts fetch task:
    /// (a) acquire guard or return `FetchAlreadyRunning`;
    /// (b) load all active holdings across all accounts;
    /// (c) filter system cash assets (MKT-116);
    /// (d) derive Stooq symbols, discard non-derivable entries;
    /// (e) if empty scope → `NoFetchableHoldings` (MKT-111);
    /// (f) dispatch background task and return `Ok(())`.
    pub async fn run(&self) -> Result<(), FetchAllAssetPricesError> {
        let lease = self
            .fetch_guard
            .try_acquire()
            .ok_or(FetchPriceTask::FetchAlreadyRunning)?;

        let accounts = self.account_service.get_all().await?;

        let mut asset_ids: HashSet<String> = HashSet::new();
        for account in &accounts {
            let holdings = self
                .account_service
                .get_holdings_for_account(&account.id)
                .await?;
            for holding in holdings {
                if holding.quantity > 0 {
                    asset_ids.insert(holding.asset_id);
                }
            }
        }

        let scope = build_scope(&self.asset_service, asset_ids).await?;
        if scope.is_empty() {
            return Err(FetchPriceTask::NoFetchableHoldings.into());
        }

        Arc::clone(&self.dispatcher).spawn(scope, lease);
        Ok(())
    }
}

pub(super) async fn build_scope(
    asset_service: &Arc<AssetService>,
    asset_ids: HashSet<String>,
) -> Result<Vec<(Asset, String)>, AssetError> {
    use crate::context::asset::derive_stooq_symbol;

    let cash_prefix = system_cash_asset_id("");
    let mut scope: Vec<(Asset, String)> = Vec::new();
    for asset_id in asset_ids {
        if asset_id.starts_with(&cash_prefix) {
            continue;
        }
        let asset = match asset_service.get_asset_by_id(&asset_id).await {
            Ok(Some(asset)) => asset,
            Ok(None) => continue,
            Err(application_error) => {
                tracing::error!(
                    target: BACKEND,
                    asset_id = %asset_id,
                    err = ?application_error,
                    "fetch_scope: get_asset_by_id failed"
                );
                return Err(translate_asset_application_error(application_error));
            }
        };
        let Some(symbol) = derive_stooq_symbol(&asset.reference) else {
            continue;
        };
        scope.push((asset, symbol));
    }
    Ok(scope)
}

fn translate_asset_application_error(error: AssetApplicationError) -> AssetError {
    match error {
        AssetApplicationError::DatabaseError => AssetError::DatabaseError,
        _ => AssetError::DatabaseError,
    }
}
