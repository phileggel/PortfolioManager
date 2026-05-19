use crate::context::account::{AccountApplicationError, AccountService};
use crate::context::asset::{
    derive_stooq_symbol_with_exchange, Asset, AssetApplicationError, AssetError, AssetService,
};
use crate::core::cash::system_cash_asset_id;
use crate::core::logger::BACKEND;
use std::collections::HashSet;
use std::sync::Arc;

use super::dispatcher::Dispatcher;
use super::error::{FetchAccountAssetPricesError, FetchAllAssetPricesError, FetchPriceTask};
use super::guard::FetchGuard;

/// Orchestrates the asset-price fetch tasks — `fetch_all` (MKT-122 / MKT-130) and
/// `fetch_for_account` (MKT-132 / MKT-131). Both methods share the same in-flight
/// guard, dispatcher, and scope-building logic.
pub struct AssetPriceFetchUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
    fetch_guard: Arc<FetchGuard>,
    dispatcher: Arc<Dispatcher>,
}

impl AssetPriceFetchUseCase {
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
    pub async fn fetch_all(&self) -> Result<(), FetchAllAssetPricesError> {
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

        let scope = self.build_scope(asset_ids).await?;
        if scope.is_empty() {
            return Err(FetchPriceTask::NoFetchableHoldings.into());
        }

        Arc::clone(&self.dispatcher).spawn(scope, lease);
        Ok(())
    }

    /// Runs the per-account fetch task:
    /// (a) check account exists via `account_service.get_by_id`, else `AccountNotFound` (MKT-132);
    /// (b) acquire guard or return `FetchAlreadyRunning` (MKT-113);
    /// (c) load holdings for the account;
    /// (d) filter system cash assets (MKT-116);
    /// (e) derive Stooq symbols, discard non-derivable entries;
    /// (f) if empty scope → `NoFetchableHoldings` (MKT-111);
    /// (g) dispatch background task and return `Ok(())`.
    pub async fn fetch_for_account(
        &self,
        account_id: &str,
    ) -> Result<(), FetchAccountAssetPricesError> {
        let account = self
            .account_service
            .get_by_id(account_id)
            .await?
            .ok_or_else(|| {
                FetchAccountAssetPricesError::Account(AccountApplicationError::AccountNotFound {
                    account_id: account_id.to_string(),
                })
            })?;

        let lease = self
            .fetch_guard
            .try_acquire()
            .ok_or(FetchPriceTask::FetchAlreadyRunning)?;

        let holdings = self
            .account_service
            .get_holdings_for_account(&account.id)
            .await?;
        let asset_ids: HashSet<String> = holdings
            .into_iter()
            .filter(|holding| holding.quantity > 0)
            .map(|holding| holding.asset_id)
            .collect();

        let scope = self.build_scope(asset_ids).await?;
        if scope.is_empty() {
            return Err(FetchPriceTask::NoFetchableHoldings.into());
        }

        Arc::clone(&self.dispatcher).spawn(scope, lease);
        Ok(())
    }

    async fn build_scope(
        &self,
        asset_ids: HashSet<String>,
    ) -> Result<Vec<(Asset, String)>, AssetError> {
        let cash_prefix = system_cash_asset_id("");
        let mut scope: Vec<(Asset, String)> = Vec::new();
        for asset_id in asset_ids {
            if asset_id.starts_with(&cash_prefix) {
                continue;
            }
            let asset = match self.asset_service.get_asset_by_id(&asset_id).await {
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
            let Some(symbol) =
                derive_stooq_symbol_with_exchange(&asset.reference, asset.exchange.as_ref())
            else {
                continue;
            };
            scope.push((asset, symbol));
        }
        Ok(scope)
    }
}

fn translate_asset_application_error(error: AssetApplicationError) -> AssetError {
    match error {
        AssetApplicationError::DatabaseError => AssetError::DatabaseError,
        _ => AssetError::DatabaseError,
    }
}
