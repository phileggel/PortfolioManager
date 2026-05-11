/// Integration tests for AssetService category CRUD, update_asset error paths,
/// and event bus emission on write operations.
///
/// Uses real SQLite repos against an in-memory DB (B27).
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use std::time::Duration;
use vault_compass_lib::context::asset::{
    AssetClass, AssetService, CreateAssetDTO, SqliteAssetCategoryRepository,
    SqliteAssetPriceRepository, SqliteAssetRepository, UpdateAssetDTO, SYSTEM_CATEGORY_ID,
};
use vault_compass_lib::core::{Event, SideEffectEventBus};

async fn make_pool() -> sqlx::Pool<sqlx::Sqlite> {
    let pool = SqlitePoolOptions::new()
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

async fn setup() -> (AssetService, Arc<SideEffectEventBus>) {
    let pool = make_pool().await;
    let bus = Arc::new(SideEffectEventBus::new());
    let svc = AssetService::new(
        Box::new(SqliteAssetRepository::new(pool.clone())),
        Box::new(SqliteAssetCategoryRepository::new(pool.clone())),
        Box::new(SqliteAssetPriceRepository::new(pool)),
    )
    .with_event_bus(Arc::clone(&bus));
    (svc, bus)
}

fn base_create_dto(name: &str) -> CreateAssetDTO {
    CreateAssetDTO {
        name: name.to_string(),
        reference: "REF".to_string(),
        class: AssetClass::Stocks,
        currency: "USD".to_string(),
        risk_level: 1,
        category_id: SYSTEM_CATEGORY_ID.to_string(),
    }
}

// ── Category CRUD ─────────────────────────────────────────────────────────────

/// create_category() persists and get_category_by_id() retrieves it.
#[tokio::test]
async fn test_create_category_and_retrieve_by_id() {
    let (svc, _bus) = setup().await;

    let cat = svc.create_category("Bonds").await.expect("seed category");
    assert_eq!(cat.name, "Bonds");

    let found = svc.get_category_by_id(&cat.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(
        found.expect("category should exist after create").name,
        "Bonds"
    );
}

/// update_category() persists the new label.
#[tokio::test]
async fn test_update_category_changes_label() {
    let (svc, _bus) = setup().await;

    let cat = svc.create_category("OldLabel").await.unwrap();
    let updated = svc.update_category(&cat.id, "NewLabel").await.unwrap();
    assert_eq!(updated.name, "NewLabel");

    let found = svc
        .get_category_by_id(&cat.id)
        .await
        .expect("DB read should succeed")
        .expect("category should exist after update");
    assert_eq!(found.name, "NewLabel");
}

/// delete_category() removes it from get_all_categories().
#[tokio::test]
async fn test_delete_category_removes_it() {
    let (svc, _bus) = setup().await;

    let cat = svc
        .create_category("ToRemove")
        .await
        .expect("seed category");
    svc.delete_category(&cat.id).await.unwrap();

    let all = svc.get_all_categories().await.unwrap();
    assert!(!all.iter().any(|c| c.id == cat.id));
}

// ── update_asset error paths ──────────────────────────────────────────────────

/// update_asset() returns Archived error when the asset is archived.
#[tokio::test]
async fn test_update_asset_rejected_when_archived() {
    let (svc, _bus) = setup().await;

    let asset = svc
        .create_asset(base_create_dto("ArchiveMe"))
        .await
        .expect("seed asset");
    svc.archive_asset(&asset.id).await.unwrap();

    let err = svc
        .update_asset(UpdateAssetDTO {
            asset_id: asset.id,
            name: "NewName".to_string(),
            reference: "REF".to_string(),
            class: AssetClass::Stocks,
            currency: "USD".to_string(),
            risk_level: 1,
            category_id: SYSTEM_CATEGORY_ID.to_string(),
        })
        .await
        .unwrap_err();

    use vault_compass_lib::context::asset::{AssetCrudError, AssetDomainError};
    assert!(
        matches!(&err, AssetCrudError::Validation(AssetDomainError::Archived)),
        "expected Archived, got: {err:?}"
    );
}

/// update_asset() returns CategoryNotFound when the given category_id does not exist.
#[tokio::test]
async fn test_update_asset_rejected_when_category_not_found() {
    let (svc, _bus) = setup().await;

    let asset = svc.create_asset(base_create_dto("CatCheck")).await.unwrap();

    let err = svc
        .update_asset(UpdateAssetDTO {
            asset_id: asset.id,
            name: "CatCheck".to_string(),
            reference: "REF".to_string(),
            class: AssetClass::Stocks,
            currency: "USD".to_string(),
            risk_level: 1,
            category_id: "nonexistent-category-id".to_string(),
        })
        .await
        .unwrap_err();

    use vault_compass_lib::context::asset::{AssetCrudError, CategoryApplicationError};
    assert!(
        matches!(
            &err,
            AssetCrudError::CategoryApplication(CategoryApplicationError::NotFound { .. })
        ),
        "expected CategoryApplicationError::NotFound, got: {err:?}"
    );
}

// ── Event bus emission ────────────────────────────────────────────────────────

/// create_asset() fires AssetUpdated on the event bus.
#[tokio::test]
async fn test_create_asset_publishes_asset_updated_event() {
    let (svc, bus) = setup().await;
    let mut rx = bus.subscribe();

    svc.create_asset(base_create_dto("EventAsset"))
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_millis(200), rx.changed())
        .await
        .expect("event not received within 200ms")
        .expect("watch sender dropped before event fired");
    assert_eq!(*rx.borrow(), Event::AssetUpdated);
}

/// archive_asset() fires AssetUpdated on the event bus.
#[tokio::test]
async fn test_archive_asset_publishes_asset_updated_event() {
    let (svc, bus) = setup().await;

    let asset = svc
        .create_asset(base_create_dto("ToArchive"))
        .await
        .unwrap();
    let mut rx = bus.subscribe();

    svc.archive_asset(&asset.id).await.unwrap();

    tokio::time::timeout(Duration::from_millis(200), rx.changed())
        .await
        .expect("event not received within 200ms")
        .expect("watch sender dropped before event fired");
    assert_eq!(*rx.borrow(), Event::AssetUpdated);
}

/// unarchive_asset() fires AssetUpdated on the event bus.
#[tokio::test]
async fn test_unarchive_asset_publishes_asset_updated_event() {
    let (svc, bus) = setup().await;

    let asset = svc
        .create_asset(base_create_dto("ToUnarchive"))
        .await
        .unwrap();
    svc.archive_asset(&asset.id).await.unwrap();
    let mut rx = bus.subscribe();

    svc.unarchive_asset(&asset.id).await.unwrap();

    tokio::time::timeout(Duration::from_millis(200), rx.changed())
        .await
        .expect("event not received within 200ms")
        .expect("watch sender dropped before event fired");
    assert_eq!(*rx.borrow(), Event::AssetUpdated);
}

/// delete_asset() fires AssetUpdated on the event bus.
#[tokio::test]
async fn test_delete_asset_publishes_asset_updated_event() {
    let (svc, bus) = setup().await;

    let asset = svc.create_asset(base_create_dto("ToDelete")).await.unwrap();
    let mut rx = bus.subscribe();

    svc.delete_asset(&asset.id).await.unwrap();

    tokio::time::timeout(Duration::from_millis(200), rx.changed())
        .await
        .expect("event not received within 200ms")
        .expect("watch sender dropped before event fired");
    assert_eq!(*rx.borrow(), Event::AssetUpdated);
}

/// create_category() fires CategoryUpdated on the event bus.
#[tokio::test]
async fn test_create_category_publishes_category_updated_event() {
    let (svc, bus) = setup().await;
    let mut rx = bus.subscribe();

    svc.create_category("EventCat").await.unwrap();

    tokio::time::timeout(Duration::from_millis(200), rx.changed())
        .await
        .expect("event not received within 200ms")
        .expect("watch sender dropped before event fired");
    assert_eq!(*rx.borrow(), Event::CategoryUpdated);
}
