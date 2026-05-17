/// Integration tests for the auto-fetch use case (MKT-122, MKT-132, MKT-111, MKT-113).
///
/// These tests exercise the full stack through the public API: use case constructors →
/// AccountService / AssetService → real in-memory SQLite. No mocks — per test_convention.md
/// Tier 3 constraint.
///
/// All tests will fail to compile or panic with `unimplemented!` until the use cases,
/// the migration (source column), and the AssetPriceSource enum are implemented.
use std::sync::Arc;
use vault_compass_lib::context::account::{
    AccountService, SqliteAccountRepository, SqliteHoldingRepository, SqliteTransactionRepository,
};
use vault_compass_lib::context::asset::{
    AssetService, SqliteAssetCategoryRepository, SqliteAssetPriceRepository, SqliteAssetRepository,
};
use vault_compass_lib::core::SideEffectEventBus;
use vault_compass_lib::use_cases::asset_price_fetch::{
    FetchAccountAssetPricesUseCase, FetchAllAssetPricesUseCase, FetchGuard,
};

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
    fetch_all_uc: FetchAllAssetPricesUseCase,
    fetch_account_uc: FetchAccountAssetPricesUseCase,
    fetch_guard: Arc<FetchGuard>,
}

async fn build_ctx() -> Ctx {
    let pool = make_pool().await;
    let bus = Arc::new(SideEffectEventBus::new());

    let account_svc = Arc::new(
        AccountService::new(
            Box::new(SqliteAccountRepository::new(pool.clone())),
            Box::new(SqliteHoldingRepository::new(pool.clone())),
            Box::new(SqliteTransactionRepository::new(pool.clone())),
        )
        .with_event_bus(Arc::clone(&bus)),
    );
    let asset_svc = Arc::new(
        AssetService::new(
            Box::new(SqliteAssetRepository::new(pool.clone())),
            Box::new(SqliteAssetCategoryRepository::new(pool.clone())),
            Box::new(SqliteAssetPriceRepository::new(pool.clone())),
        )
        .with_event_bus(Arc::clone(&bus)),
    );
    let fetch_guard = Arc::new(FetchGuard::new());

    // Dispatcher construction requires PriceProvider which is not part of the
    // integration test public API yet — use a test-only no-op provider.
    // This will fail to compile until `vault_compass_lib::use_cases::asset_price_fetch`
    // exports the necessary types — that is the intended red baseline.
    let (fetch_all_uc, fetch_account_uc) = {
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

        let all_uc = FetchAllAssetPricesUseCase::new(
            Arc::clone(&account_svc),
            Arc::clone(&asset_svc),
            Arc::clone(&fetch_guard),
            Arc::clone(&dispatcher),
        );
        let account_uc = FetchAccountAssetPricesUseCase::new(
            Arc::clone(&account_svc),
            Arc::clone(&asset_svc),
            Arc::clone(&fetch_guard),
            Arc::clone(&dispatcher),
        );
        (all_uc, account_uc)
    };

    Ctx {
        fetch_all_uc,
        fetch_account_uc,
        fetch_guard,
    }
}

/// MKT-111 — fetch_all_asset_prices returns NoFetchableHoldings when no non-cash
/// derivable holdings exist (empty database). Exercises the full stack end-to-end.
#[tokio::test]
async fn fetch_all_returns_no_fetchable_holdings_on_empty_db() {
    use vault_compass_lib::use_cases::asset_price_fetch::{
        FetchAllAssetPricesError, FetchPriceTask,
    };

    let ctx = build_ctx().await;
    let result = ctx.fetch_all_uc.run().await;

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

/// MKT-132 — fetch_account_asset_prices returns AccountNotFound for an unknown account_id.
/// Exercises the full existence-check stack.
#[tokio::test]
async fn fetch_account_returns_account_not_found_for_unknown_id() {
    use vault_compass_lib::context::account::AccountApplicationError;
    use vault_compass_lib::use_cases::asset_price_fetch::FetchAccountAssetPricesError;

    let ctx = build_ctx().await;
    let result = ctx.fetch_account_uc.run("does-not-exist").await;

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

/// MKT-113 — fetch_all_asset_prices returns FetchAlreadyRunning when the guard is held.
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

    let result = ctx.fetch_all_uc.run().await;
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
