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
            async fn fetch_price(&self, _symbol: &str) -> anyhow::Result<Option<i64>> {
                Ok(Some(100_000_000))
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

/// MKT-110 — fetch_for_account uses derive_stooq_symbol_with_exchange so an asset
/// carrying `exchange = Some(XPAR)` resolves to `<ref>.fr` (exchange-qualified)
/// rather than the bare-ticker legacy form. Guards the wiring of the picker-driven
/// exchange field into the actual Stooq fetch URL.
#[tokio::test]
async fn fetch_for_account_passes_exchange_qualified_symbol_to_provider() {
    use std::sync::Mutex;
    use vault_compass_lib::context::account::{
        AccountService, SqliteAccountRepository, SqliteHoldingRepository,
        SqliteTransactionRepository, UpdateFrequency,
    };
    use vault_compass_lib::context::asset::{
        AssetPriceRepository, AssetService, CreateAssetDTO, PriceProvider,
        SqliteAssetCategoryRepository, SqliteAssetPriceRepository, SqliteAssetRepository,
        SYSTEM_CATEGORY_ID,
    };
    use vault_compass_lib::core::SideEffectEventBus;
    use vault_compass_lib::use_cases::asset_price_fetch::dispatcher::Dispatcher;

    struct CapturingProvider {
        seen: Arc<Mutex<Vec<String>>>,
    }
    #[async_trait::async_trait]
    impl PriceProvider for CapturingProvider {
        async fn fetch_price(&self, symbol: &str) -> anyhow::Result<Option<i64>> {
            self.seen.lock().unwrap().push(symbol.to_string());
            Ok(Some(100_000_000))
        }
    }

    let pool = make_pool().await;
    let bus = Arc::new(SideEffectEventBus::new());
    let account_service = Arc::new(AccountService::new(
        Box::new(SqliteAccountRepository::new(pool.clone())),
        Box::new(SqliteHoldingRepository::new(pool.clone())),
        Box::new(SqliteTransactionRepository::new(pool.clone())),
    ));
    let asset_service = Arc::new(AssetService::new(
        Box::new(SqliteAssetRepository::new(pool.clone())),
        Box::new(SqliteAssetCategoryRepository::new(pool.clone())),
        Box::new(SqliteAssetPriceRepository::new(pool.clone())),
    ));

    let asset = asset_service
        .create_asset(CreateAssetDTO {
            name: "Air Liquide".to_string(),
            reference: "AI".to_string(),
            class: vault_compass_lib::context::asset::AssetClass::Stocks,
            currency: "EUR".to_string(),
            risk_level: 4,
            category_id: SYSTEM_CATEGORY_ID.to_string(),
            exchange: Some(vault_compass_lib::context::asset::Exchange {
                code: "XPAR".to_string(),
                label: "Euronext Paris".to_string(),
            }),
        })
        .await
        .expect("seed asset with XPAR exchange");

    let account = account_service
        .create(
            "Test".to_string(),
            "EUR".to_string(),
            UpdateFrequency::ManualMonth,
        )
        .await
        .expect("seed account");
    account_service
        .open_holding(
            &account.id,
            asset.id.clone(),
            "2024-01-01".to_string(),
            1_000_000,
            100_000_000,
        )
        .await
        .expect("seed holding");

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let provider = Arc::new(CapturingProvider {
        seen: Arc::clone(&seen),
    });
    let price_repo: Arc<dyn AssetPriceRepository> =
        Arc::new(SqliteAssetPriceRepository::new(pool.clone()));
    let dispatcher = Arc::new(Dispatcher::new(
        provider,
        price_repo,
        Arc::clone(&bus),
        Arc::new(|| chrono::Local::now().date_naive()),
    ));
    let use_case = AssetPriceFetchUseCase::new(
        Arc::clone(&account_service),
        Arc::clone(&asset_service),
        Arc::new(FetchGuard::new()),
        dispatcher,
    );

    use_case
        .fetch_for_account(&account.id)
        .await
        .expect("fetch_for_account dispatch");

    // Dispatcher::spawn launches an async task — give it a moment to call the provider.
    for _ in 0..50 {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let symbols = seen.lock().unwrap().clone();
    assert_eq!(
        symbols,
        vec!["ai.fr".to_string()],
        "MKT-110: orchestrator must derive `ai.fr` (XPAR → .fr suffix) for an asset carrying `exchange = Some(XPAR)`, not the bare `ai` legacy form"
    );
}
