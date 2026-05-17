use crate::context::account::{AccountApplicationError, AccountService};
use crate::context::asset::{AssetError, AssetService};
use std::collections::HashSet;
use std::sync::Arc;

use super::all::build_scope;
use super::dispatcher::Dispatcher;
use super::error::FetchPriceTask;
use super::guard::FetchGuard;

/// Wire-facing error composite for `fetch_account_asset_prices` (MKT-113, MKT-111, MKT-132).
///
/// See `FetchAllAssetPricesError` for the shared shape rationale.
#[derive(Debug, thiserror::Error, serde::Serialize, specta::Type)]
#[serde(untagged)]
pub enum FetchAccountAssetPricesError {
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

/// Orchestrates the per-account price-fetch task (MKT-132, MKT-131).
pub struct FetchAccountAssetPricesUseCase {
    account_service: Arc<AccountService>,
    asset_service: Arc<AssetService>,
    fetch_guard: Arc<FetchGuard>,
    dispatcher: Arc<Dispatcher>,
}

impl FetchAccountAssetPricesUseCase {
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

    /// Runs the per-account fetch task:
    /// (a) check account exists via `account_service.get_by_id`, else `AccountNotFound` (MKT-132);
    /// (b) acquire guard or return `FetchAlreadyRunning` (MKT-113);
    /// (c) load holdings for the account;
    /// (d) filter system cash assets (MKT-116);
    /// (e) derive Stooq symbols, discard non-derivable entries;
    /// (f) if empty scope → `NoFetchableHoldings` (MKT-111);
    /// (g) dispatch background task and return `Ok(())`.
    pub async fn run(&self, account_id: &str) -> Result<(), FetchAccountAssetPricesError> {
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

        let scope = build_scope(&self.asset_service, asset_ids).await?;
        if scope.is_empty() {
            return Err(FetchPriceTask::NoFetchableHoldings.into());
        }

        Arc::clone(&self.dispatcher).spawn(scope, lease);
        Ok(())
    }
}
