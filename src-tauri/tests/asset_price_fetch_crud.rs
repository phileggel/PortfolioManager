/// Integration tests for the asset-price fetch orchestrator (MKT-122, MKT-132, MKT-111, MKT-113).
///
/// These tests exercise the full stack through the public API: orchestrator constructor →
/// AccountService / AssetService → real in-memory SQLite. No mocks — per test_convention.md
/// Tier 3 constraint.
use std::sync::Arc;
use vault_compass_lib::context::account::{
    AccountService, SqliteAccountRepository, SqliteHoldingRepository, SqliteTransactionRepository,
};
use vault_compass_lib::context::asset::{
    AssetService, SqliteAssetCategoryRepository, SqliteAssetPriceRepository, SqliteAssetRepository,
};
use vault_compass_lib::core::SideEffectEventBus;
use vault_compass_lib::use_cases::asset_price_fetch::{AssetPriceFetchUseCase, FetchGuard};

async fn make_pool() -> sqlx::Pool<sqlx::Sqlite> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");
    pool
}

struct Ctx {
    use_case: AssetPriceFetchUseCase,
    fetch_guard: Arc<FetchGuard>,
}

async fn build_ctx() -> Ctx {
    let pool = make_pool().await;
    let bus = Arc::new(SideEffectEventBus::new());

    let account_service = Arc::new(
        AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        )
        .with_event_bus(Arc::clone(&bus)),
    );
    let asset_service = Arc::new(
        AssetService::new(
            Box::new(SqliteAssetRepository::new(pool.clone())),
            Box::new(SqliteAssetCategoryRepository::new(pool.clone())),
            Box::new(SqliteAssetPriceRepository::new(pool.clone())),
        )
        .with_event_bus(Arc::clone(&bus)),
    );
    let fetch_guard = Arc::new(FetchGuard::new());

    let use_case = {
        use vault_compass_lib::use_cases::asset_price_fetch::dispatcher::Dispatcher;

        struct NoOpProvider;
        #[async_trait::async_trait]
        impl vault_compass_lib::context::asset::PriceProvider for NoOpProvider {
            async fn fetch_price(&self, _symbol: &str) -> anyhow::Result<i64> {
                Ok(100_000_000)
            }
        }

        let price_repo: Arc<dyn vault_compass_lib::context::asset::AssetPriceRepository> =
            Arc::new(SqliteAssetPriceRepository::new(pool.clone()));

        let dispatcher = Arc::new(Dispatcher::new(
            Arc::new(NoOpProvider),
            price_repo,
            Arc::clone(&bus),
            Arc::new(|| chrono::Local::now().date_naive()),
        ));

        AssetPriceFetchUseCase::new(
            Arc::clone(&account_service),
            Arc::clone(&asset_service),
            Arc::clone(&fetch_guard),
            dispatcher,
        )
    };

    Ctx {
        use_case,
        fetch_guard,
    }
}

/// MKT-111 — fetch_all returns NoFetchableHoldings when no non-cash derivable holdings exist
/// (empty database). Exercises the full stack end-to-end.
#[tokio::test]
async fn fetch_all_returns_no_fetchable_holdings_on_empty_db() {
    use vault_compass_lib::use_cases::asset_price_fetch::{
        FetchAllAssetPricesError, FetchPriceTask,
    };

    let ctx = build_ctx().await;
    let result = ctx.use_case.fetch_all().await;

    assert!(
        matches!(
            result,
            Err(FetchAllAssetPricesError::Failure(
                FetchPriceTask::NoFetchableHoldings
            ))
        ),
        "expected NoFetchableHoldings on empty DB, got: {result:?}"
    );
}

/// MKT-132 — fetch_for_account returns AccountNotFound for an unknown account_id.
/// Exercises the full existence-check stack.
#[tokio::test]
async fn fetch_for_account_returns_account_not_found_for_unknown_id() {
    use vault_compass_lib::context::account::AccountApplicationError;
    use vault_compass_lib::use_cases::asset_price_fetch::FetchAccountAssetPricesError;

    let ctx = build_ctx().await;
    let result = ctx.use_case.fetch_for_account("does-not-exist").await;

    assert!(
        matches!(
            result,
            Err(FetchAccountAssetPricesError::Account(
                AccountApplicationError::AccountNotFound { .. }
            ))
        ),
        "expected Account(AccountNotFound), got: {result:?}"
    );
}

/// MKT-113 — fetch_all returns FetchAlreadyRunning when the guard is held externally.
/// Verifies the in-flight guard propagates through the public use-case API.
#[tokio::test]
async fn fetch_all_returns_fetch_already_running_while_guard_held() {
    use vault_compass_lib::use_cases::asset_price_fetch::{
        FetchAllAssetPricesError, FetchPriceTask,
    };

    let ctx = build_ctx().await;
    let _lease = ctx
        .fetch_guard
        .try_acquire()
        .expect("guard must be free at test start");

    let result = ctx.use_case.fetch_all().await;
    assert!(
        matches!(
            result,
            Err(FetchAllAssetPricesError::Failure(
                FetchPriceTask::FetchAlreadyRunning
            ))
        ),
        "expected FetchAlreadyRunning, got: {result:?}"
    );
}
