use super::application::{CategoryApplicationError, CategoryCrudError};
use super::domain::{
    Asset, AssetCategory, AssetCategoryRepository, AssetClass, AssetDomainError, AssetPrice,
    AssetPriceDomainError, AssetPriceRepository, AssetRepository, SYSTEM_CATEGORY_ID,
};
use crate::{
    context::asset::{CreateAssetDTO, UpdateAssetDTO},
    core::{Event, SideEffectEventBus, BACKEND},
};
use anyhow::Result;
use std::sync::Arc;

/// Orchestrates business logic for assets, categories, and market prices.
pub struct AssetService {
    asset_repo: Box<dyn AssetRepository>,
    category_repo: Box<dyn AssetCategoryRepository>,
    price_repo: Box<dyn AssetPriceRepository>,
    event_bus: Option<Arc<SideEffectEventBus>>,
}

impl AssetService {
    /// Creates a new AssetService.
    pub fn new(
        asset_repo: Box<dyn AssetRepository>,
        category_repo: Box<dyn AssetCategoryRepository>,
        price_repo: Box<dyn AssetPriceRepository>,
    ) -> Self {
        Self {
            asset_repo,
            category_repo,
            price_repo,
            event_bus: None,
        }
    }

    /// Attaches an event bus for side-effect notifications.
    pub fn with_event_bus(mut self, bus: Arc<SideEffectEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    // --- Asset Methods ---

    /// Retrieves all active (non-archived) assets.
    pub async fn get_all_assets(&self) -> Result<Vec<Asset>> {
        self.asset_repo.get_all().await
    }

    /// Retrieves all assets including archived ones.
    pub async fn get_all_assets_with_archived(&self) -> Result<Vec<Asset>> {
        self.asset_repo.get_all_including_archived().await
    }

    /// Retrieves a single asset by ID.
    pub async fn get_asset_by_id(&self, asset_id: &str) -> Result<Option<Asset>> {
        self.asset_repo.get_by_id(asset_id).await
    }

    /// Idempotently seeds the system Cash Asset for `currency` and the system
    /// Cash category that hosts it (CSH-010, CSH-011, CSH-017). Safe to call from every
    /// cash-affecting use case — returns the existing asset on subsequent calls.
    ///
    /// Asset id format: `system-cash-{ccy_lower}`. Category id: `system-cash-category`.
    /// Both constants and the id format live in `core::cash` so the account context
    /// can use the same format without crossing a context boundary (B13).
    pub async fn seed_cash_asset(&self, currency: &str) -> Result<Asset> {
        let asset_id = crate::core::cash::system_cash_asset_id(currency);

        if let Some(existing) = self.asset_repo.get_by_id(&asset_id).await? {
            return Ok(existing);
        }

        let category = match self
            .category_repo
            .get_by_id(crate::core::cash::SYSTEM_CASH_CATEGORY_ID)
            .await?
        {
            Some(c) => c,
            None => {
                let cat = AssetCategory::with_id(
                    crate::core::cash::SYSTEM_CASH_CATEGORY_ID.to_string(),
                    crate::core::cash::SYSTEM_CASH_CATEGORY_LABEL.to_string(),
                )?;
                self.category_repo.create(cat).await?
            }
        };

        let asset = Asset::with_id(
            asset_id.clone(),
            format!("Cash {}", currency.to_uppercase()),
            AssetClass::Cash,
            category,
            currency.to_string(),
            1,
            currency.to_uppercase(),
            false,
        )?;
        let asset = self.asset_repo.create(asset).await?;
        tracing::info!(target: BACKEND, asset_id = %asset.id, currency = %currency, "Seeded Cash Asset");

        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetUpdated);
        }

        Ok(asset)
    }

    /// Creates a new asset and publishes an AssetUpdated event.
    pub async fn create_asset(&self, dto: CreateAssetDTO) -> Result<Asset> {
        let category = self
            .get_category_by_id(&dto.category_id)
            .await?
            .ok_or_else(|| CategoryApplicationError::NotFound {
                id: dto.category_id.clone(),
            })?;

        let asset = Asset::new(
            dto.name,
            dto.class,
            category,
            dto.currency,
            dto.risk_level,
            dto.reference,
        )?;

        let asset = self.asset_repo.create(asset).await?;
        tracing::info!(target: BACKEND, asset_id = %asset.id, name = %asset.name, "Asset created");

        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetUpdated);
        }

        Ok(asset)
    }

    /// Updates an existing asset. Rejects if the asset is the system Cash Asset
    /// (CSH-016) or archived (R6) — both invariants are enforced inside
    /// `Asset::update_from` on the loaded aggregate (single source of truth).
    pub async fn update_asset(&self, dto: UpdateAssetDTO) -> Result<Asset> {
        let existing = self
            .asset_repo
            .get_by_id(&dto.asset_id)
            .await?
            .ok_or_else(|| AssetDomainError::NotFound(dto.asset_id.clone()))?;

        let category = self
            .get_category_by_id(&dto.category_id)
            .await?
            .ok_or_else(|| CategoryApplicationError::NotFound {
                id: dto.category_id.clone(),
            })?;

        let asset = existing.update_from(
            dto.name,
            dto.class,
            category,
            dto.currency,
            dto.risk_level,
            dto.reference,
        )?;

        let asset = self.asset_repo.update(asset).await?;
        tracing::info!(target: BACKEND, asset_id = %asset.id, name = %asset.name, "Asset updated");

        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetUpdated);
        }

        Ok(asset)
    }

    /// Archives an asset (reversible — R6). The system-asset invariant
    /// (CSH-016) is enforced inside `Asset::archive`.
    pub async fn archive_asset(&self, asset_id: &str) -> Result<()> {
        let existing = self
            .asset_repo
            .get_by_id(asset_id)
            .await?
            .ok_or_else(|| AssetDomainError::NotFound(asset_id.to_string()))?;
        // Aggregate enforces the invariant; the returned mutated Asset is
        // intentionally discarded — the column-update fast path
        // `repo.archive(id)` handles persistence. PR 2 / a follow-up may
        // collapse to `repo.update(asset)` once typed-Result migration lands.
        existing.archive()?;
        self.asset_repo.archive(asset_id).await?;
        tracing::info!(target: BACKEND, asset_id = %asset_id, "Asset archived");
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetUpdated);
        }
        Ok(())
    }

    /// Unarchives an asset (R18). The system-asset invariant (CSH-016) is
    /// enforced inside `Asset::unarchive`.
    pub async fn unarchive_asset(&self, asset_id: &str) -> Result<()> {
        let existing = self
            .asset_repo
            .get_by_id(asset_id)
            .await?
            .ok_or_else(|| AssetDomainError::NotFound(asset_id.to_string()))?;
        // See `archive_asset` for the rationale on discarding the returned aggregate.
        existing.unarchive()?;
        self.asset_repo.unarchive(asset_id).await?;
        tracing::info!(target: BACKEND, asset_id = %asset_id, "Asset unarchived");
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetUpdated);
        }
        Ok(())
    }

    /// Soft-deletes an asset and publishes an AssetUpdated event. The
    /// system-asset invariant (CSH-016) is enforced inside
    /// `Asset::ensure_user_managed` on the loaded aggregate.
    pub async fn delete_asset(&self, asset_id: &str) -> Result<()> {
        let existing = self
            .asset_repo
            .get_by_id(asset_id)
            .await?
            .ok_or_else(|| AssetDomainError::NotFound(asset_id.to_string()))?;
        existing.ensure_user_managed()?;
        self.asset_repo.delete(asset_id).await?;
        tracing::info!(target: BACKEND, asset_id = %asset_id, "Asset deleted");
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetUpdated);
        }
        Ok(())
    }

    // --- Category Methods ---

    /// Retrieves all non-deleted categories.
    ///
    /// Read-only — the only failure mode is a repository error, surfaced as
    /// `CategoryApplicationError::DatabaseError` (typed, payload-free per the
    /// gold infra-translation rule). The full diagnostic stays in
    /// `tracing::error!` server-side.
    pub async fn get_all_categories(
        &self,
    ) -> std::result::Result<Vec<AssetCategory>, CategoryApplicationError> {
        self.category_repo.get_all().await.map_err(|e| {
            tracing::error!(target: BACKEND, err = ?e, "get_all_categories: repository failure");
            CategoryApplicationError::DatabaseError
        })
    }

    /// Retrieves a category by ID.
    ///
    /// Kept on `anyhow::Result` for now — used by `create_asset` / `update_asset`
    /// (Asset CRUD), which has not been migrated to typed errors yet. Will be
    /// narrowed to `Result<_, CategoryApplicationError>` when Asset CRUD migrates
    /// in a follow-up PR.
    pub async fn get_category_by_id(&self, id: &str) -> Result<Option<AssetCategory>> {
        self.category_repo.get_by_id(id).await
    }

    /// Creates a category and publishes a CategoryUpdated event.
    pub async fn create_category(
        &self,
        label: &str,
    ) -> std::result::Result<AssetCategory, CategoryCrudError> {
        if find_category_by_name(&*self.category_repo, label)
            .await?
            .is_some()
        {
            return Err(CategoryApplicationError::DuplicateName.into());
        }
        let category = AssetCategory::new(label.to_string())?;
        let category = self.category_repo.create(category).await.map_err(|e| {
            tracing::error!(target: BACKEND, err = ?e, "create_category: repository failure");
            CategoryApplicationError::DatabaseError
        })?;
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::CategoryUpdated);
        }
        Ok(category)
    }

    /// Updates a category and publishes a CategoryUpdated event. The
    /// system-category invariant (`SystemReadonly`) is enforced inside
    /// `AssetCategory::update_from` on the loaded aggregate (single source of
    /// truth). `update_from` runs before the uniqueness query so SystemReadonly
    /// takes precedence over DuplicateName when both would apply.
    pub async fn update_category(
        &self,
        id: &str,
        label: &str,
    ) -> std::result::Result<AssetCategory, CategoryCrudError> {
        let existing = load_category_for_crud(&*self.category_repo, id).await?;
        let candidate = existing.update_from(label.to_string())?;
        if let Some(other) = find_category_by_name(&*self.category_repo, &candidate.name).await? {
            if other.id != id {
                return Err(CategoryApplicationError::DuplicateName.into());
            }
        }
        let category = self.category_repo.update(candidate).await.map_err(|e| {
            tracing::error!(target: BACKEND, category_id = %id, err = ?e, "update_category: repository failure");
            CategoryApplicationError::DatabaseError
        })?;
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::CategoryUpdated);
        }
        Ok(category)
    }

    /// Reassigns assets to default category, then deletes the category. The
    /// system-category invariant (`SystemProtected`) is enforced inside
    /// `AssetCategory::ensure_deletable` on the loaded aggregate.
    pub async fn delete_category(
        &self,
        category_id: &str,
    ) -> std::result::Result<(), CategoryCrudError> {
        let existing = load_category_for_crud(&*self.category_repo, category_id).await?;
        existing.ensure_deletable()?;
        self.category_repo
            .reassign_assets_and_delete(category_id, SYSTEM_CATEGORY_ID)
            .await
            .map_err(|e| {
                tracing::error!(target: BACKEND, category_id = %category_id, err = ?e, "delete_category: repository failure");
                CategoryApplicationError::DatabaseError
            })?;
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::CategoryUpdated);
        }
        Ok(())
    }

    // --- AssetPrice Methods ---

    /// Converts a decimal f64 price to i64 micro-units at the IPC boundary (ADR-001 / MKT-024).
    /// Caller must have already checked `price_f64.is_finite()`.
    fn f64_to_micros(price_f64: f64) -> i64 {
        (price_f64 * 1_000_000.0).round() as i64
    }

    /// Records (or overwrites) a market price for an asset on a given date (MKT-025).
    /// Validates asset exists (MKT-043), price > 0 (MKT-021), date not in future (MKT-022).
    /// Publishes AssetPriceUpdated on success (MKT-026).
    pub async fn record_asset_price(
        &self,
        asset_id: &str,
        date: &str,
        price_f64: f64,
    ) -> Result<()> {
        // MKT-043 — reject unknown asset
        if self.asset_repo.get_by_id(asset_id).await?.is_none() {
            return Err(AssetDomainError::NotFound(asset_id.to_string()).into());
        }
        // MKT-024 — convert f64 decimal to i64 micros at the IPC boundary
        if !price_f64.is_finite() {
            return Err(AssetPriceDomainError::NonFinite.into());
        }
        let price_micros = Self::f64_to_micros(price_f64);
        // MKT-021, MKT-022 — validate via domain entity factory
        let price = AssetPrice::new(asset_id.to_string(), date.to_string(), price_micros)?;
        // MKT-025 — upsert
        self.price_repo.upsert(price).await?;
        tracing::info!(target: BACKEND, asset_id = %asset_id, date = %date, "Asset price recorded");
        // MKT-026 — publish bare signal event
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetPriceUpdated);
        }
        Ok(())
    }

    /// Returns the most recently dated market price for the given asset, or None (MKT-031).
    /// No asset-existence check: MKT-031 is a read-only display fallback; an unknown asset
    /// simply returns None, which is indistinguishable from "no price recorded yet".
    pub async fn get_latest_price(&self, asset_id: &str) -> Result<Option<AssetPrice>> {
        self.price_repo.get_latest(asset_id).await
    }

    /// Returns all recorded market prices for the given asset, sorted date descending (MKT-072).
    /// Rejects with AssetNotFound if the asset does not exist.
    pub async fn get_asset_prices(&self, asset_id: &str) -> Result<Vec<AssetPrice>> {
        if self.asset_repo.get_by_id(asset_id).await?.is_none() {
            return Err(AssetDomainError::NotFound(asset_id.to_string()).into());
        }
        self.price_repo.get_all_for_asset(asset_id).await
    }

    /// Updates the date and/or price of an existing price record (MKT-083/084).
    /// Same-date: in-place upsert. Different-date: atomic delete-old + upsert-new (MKT-084).
    /// Publishes AssetPriceUpdated on success (MKT-085).
    pub async fn update_asset_price(
        &self,
        asset_id: &str,
        original_date: &str,
        new_date: &str,
        price_f64: f64,
    ) -> Result<()> {
        // Input validation runs before the DB existence check (fail-fast on bad inputs, MKT-082).
        // MKT-082 — finite check before micro conversion
        if !price_f64.is_finite() {
            return Err(AssetPriceDomainError::NonFinite.into());
        }
        let price_micros = Self::f64_to_micros(price_f64);
        // MKT-082 — validate via domain factory (NotPositive, DateInFuture)
        let new_price = AssetPrice::new(asset_id.to_string(), new_date.to_string(), price_micros)?;
        // MKT-083 — reject if original record absent
        if self
            .price_repo
            .get_by_asset_and_date(asset_id, original_date)
            .await?
            .is_none()
        {
            return Err(AssetPriceDomainError::NotFound.into());
        }
        if original_date == new_date {
            // Same date: in-place upsert is atomic by primary key; replace_atomic not needed.
            self.price_repo.upsert(new_price).await?;
        } else {
            self.price_repo
                .replace_atomic(asset_id, original_date, new_price)
                .await?;
        }
        tracing::info!(target: BACKEND, asset_id = %asset_id, from = %original_date, to = %new_date, "Asset price updated");
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetPriceUpdated);
        }
        Ok(())
    }

    /// Deletes a specific price record by (asset_id, date) (MKT-090).
    /// Returns NotFound if the record does not exist.
    /// Publishes AssetPriceUpdated on success (MKT-091).
    pub async fn delete_asset_price(&self, asset_id: &str, date: &str) -> Result<()> {
        if self
            .price_repo
            .get_by_asset_and_date(asset_id, date)
            .await?
            .is_none()
        {
            return Err(AssetPriceDomainError::NotFound.into());
        }
        self.price_repo.delete(asset_id, date).await?;
        tracing::info!(target: BACKEND, asset_id = %asset_id, date = %date, "Asset price deleted");
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetPriceUpdated);
        }
        Ok(())
    }

    /// Publishes AssetPriceUpdated without performing any write.
    /// Called by the record_transaction use case after an atomic DB commit (MKT-057, B8).
    pub fn notify_asset_price_updated(&self) {
        if let Some(bus) = &self.event_bus {
            bus.publish(Event::AssetPriceUpdated);
        }
    }
}

/// Loads a category by ID for the CRUD family (update / delete). Translates
/// `Ok(None)` into `CategoryApplicationError::NotFound { id }` and any
/// repository error into `CategoryApplicationError::DatabaseError` after
/// preserving the diagnostic chain server-side via `tracing::error!`.
///
/// Parallel to PR 5's `load_account` helper in the account BC. Used by
/// `update_category` and `delete_category`.
async fn load_category_for_crud(
    repo: &dyn AssetCategoryRepository,
    id: &str,
) -> std::result::Result<AssetCategory, CategoryCrudError> {
    match repo.get_by_id(id).await {
        Ok(Some(cat)) => Ok(cat),
        Ok(None) => Err(CategoryApplicationError::NotFound { id: id.to_string() }.into()),
        Err(e) => {
            tracing::error!(target: BACKEND, category_id = %id, err = ?e, "load_category_for_crud: repository failure");
            Err(CategoryApplicationError::DatabaseError.into())
        }
    }
}

/// CRUD-family parallel to PR 5's `find_account_by_name`. Wraps the
/// `find_by_name` uniqueness pre-check used by `create_category` and
/// `update_category`, translating any repository failure into
/// `CategoryApplicationError::DatabaseError`.
///
/// Unlike `load_category_for_crud`, `Ok(None)` is the **success** path here
/// (the name is available); the caller decides what to do with `Some(existing)`.
async fn find_category_by_name(
    repo: &dyn AssetCategoryRepository,
    name: &str,
) -> std::result::Result<Option<AssetCategory>, CategoryCrudError> {
    repo.find_by_name(name).await.map_err(|e| {
        tracing::error!(target: BACKEND, name = %name, err = ?e, "find_category_by_name: repository failure");
        CategoryApplicationError::DatabaseError.into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::asset::{
        AssetClass, CategoryDomainError, CreateAssetDTO, MockAssetCategoryRepository,
        MockAssetPriceRepository, MockAssetRepository,
    };
    use std::sync::Arc;
    use std::time::Duration;

    fn make_svc(
        ar: MockAssetRepository,
        cr: MockAssetCategoryRepository,
        pr: MockAssetPriceRepository,
    ) -> AssetService {
        AssetService::new(Box::new(ar), Box::new(cr), Box::new(pr))
    }

    fn make_asset(id: &str, archived: bool) -> Asset {
        Asset::restore(
            id.to_string(),
            "Test Asset".to_string(),
            AssetClass::Stocks,
            make_category(),
            "USD".to_string(),
            1,
            "REF".to_string(),
            archived,
        )
    }

    fn make_cash_asset(id: &str) -> Asset {
        Asset::restore(
            id.to_string(),
            "Cash".to_string(),
            AssetClass::Cash,
            make_category(),
            "USD".to_string(),
            1,
            "USD".to_string(),
            false,
        )
    }

    fn make_category() -> AssetCategory {
        AssetCategory::from_storage(
            SYSTEM_CATEGORY_ID.to_string(),
            "generic.uncategorized".to_string(),
        )
    }

    fn make_price(asset_id: &str, date: &str, price: i64) -> AssetPrice {
        AssetPrice::restore(asset_id.to_string(), date.to_string(), price)
    }

    fn base_dto(name: &str) -> CreateAssetDTO {
        CreateAssetDTO {
            name: name.to_string(),
            reference: "REF-001".to_string(),
            class: AssetClass::Cash,
            currency: "USD".to_string(),
            risk_level: 1,
            category_id: SYSTEM_CATEGORY_ID.to_string(),
        }
    }

    // R1 — empty name is rejected
    #[tokio::test]
    async fn test_create_asset_rejects_empty_name() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .create_asset(CreateAssetDTO {
                name: "".to_string(),
                ..base_dto("ignored")
            })
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::NameEmpty)
            ),
            "got: {err}"
        );
    }

    // R1 — empty reference is rejected
    #[tokio::test]
    async fn test_create_asset_rejects_empty_reference() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .create_asset(CreateAssetDTO {
                reference: "".to_string(),
                ..base_dto("Bond")
            })
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::ReferenceEmpty)
            ),
            "got: {err}"
        );
    }

    // R1 — invalid currency is rejected
    #[tokio::test]
    async fn test_create_asset_rejects_invalid_currency() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .create_asset(CreateAssetDTO {
                currency: "INVALID".to_string(),
                ..base_dto("Bond")
            })
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::InvalidCurrency(_))
            ),
            "got: {err}"
        );
    }

    // R1 — risk level out of range is rejected
    #[tokio::test]
    async fn test_create_asset_rejects_invalid_risk_level() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .create_asset(CreateAssetDTO {
                risk_level: 6,
                ..base_dto("Bond")
            })
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::InvalidRiskLevel(_))
            ),
            "got: {err}"
        );
    }

    // R4 — service normalizes reference to uppercase before passing to asset_repo.create
    #[tokio::test]
    async fn test_create_asset_normalizes_reference_to_uppercase() {
        let mut ar = MockAssetRepository::new();
        ar.expect_create()
            .withf(|a| a.reference == "AAPL")
            .times(1)
            .return_once(Ok);
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(ar, cr, MockAssetPriceRepository::new());
        let asset = svc
            .create_asset(CreateAssetDTO {
                reference: "aapl".to_string(),
                ..base_dto("Apple")
            })
            .await
            .unwrap();
        assert_eq!(asset.reference, "AAPL");
    }

    // R4 — service trims reference spaces before passing to asset_repo.create
    #[tokio::test]
    async fn test_create_asset_normalizes_reference_trims_spaces() {
        let mut ar = MockAssetRepository::new();
        ar.expect_create()
            .withf(|a| a.reference == "AAPL")
            .times(1)
            .return_once(Ok);
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(ar, cr, MockAssetPriceRepository::new());
        let asset = svc
            .create_asset(CreateAssetDTO {
                reference: "  AAPL  ".to_string(),
                ..base_dto("Apple")
            })
            .await
            .unwrap();
        assert_eq!(asset.reference, "AAPL");
    }

    // R5/R6 — updating an archived asset is rejected
    #[tokio::test]
    async fn test_update_archived_asset_is_rejected() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-id", true))));
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(ar, cr, MockAssetPriceRepository::new());
        let err = svc
            .update_asset(UpdateAssetDTO {
                asset_id: "asset-id".to_string(),
                name: "Apple Updated".to_string(),
                reference: "AAPL".to_string(),
                class: AssetClass::Stocks,
                currency: "USD".to_string(),
                risk_level: 4,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::Archived)
            ),
            "got: {err}"
        );
    }

    // R6 — service calls asset_repo.archive with the correct id
    #[tokio::test]
    async fn test_archive_asset_delegates_to_repo() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_asset("asset-id", false))));
        ar.expect_archive()
            .withf(|id| id == "asset-id")
            .times(1)
            .return_once(|_| Ok(()));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        svc.archive_asset("asset-id").await.unwrap();
    }

    // R18 — service calls asset_repo.unarchive with the correct id
    #[tokio::test]
    async fn test_unarchive_asset_delegates_to_repo() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_asset("asset-id", true))));
        ar.expect_unarchive()
            .withf(|id| id == "asset-id")
            .times(1)
            .return_once(|_| Ok(()));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        svc.unarchive_asset("asset-id").await.unwrap();
    }

    // R7 — get_all_assets delegates to asset_repo.get_all (not get_all_including_archived)
    #[tokio::test]
    async fn test_get_all_assets_excludes_archived() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_all()
            .times(1)
            .return_once(|| Ok(vec![make_asset("active-id", false)]));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let result = svc.get_all_assets().await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "active-id");
    }

    // R19 — get_all_assets_with_archived delegates to asset_repo.get_all_including_archived
    #[tokio::test]
    async fn test_get_all_assets_with_archived_includes_both() {
        let active = make_asset("active-id", false);
        let archived = make_asset("archived-id", true);
        let mut ar = MockAssetRepository::new();
        ar.expect_get_all_including_archived()
            .times(1)
            .return_once(move || Ok(vec![active, archived]));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let all = svc.get_all_assets_with_archived().await.unwrap();
        assert!(all.iter().any(|a| a.id == "active-id"));
        assert!(all.iter().any(|a| a.id == "archived-id"));
    }

    // Category tests

    // R1 — duplicate name, same case: service checks find_by_name before creating
    #[tokio::test]
    async fn test_create_category_rejects_duplicate_same_case() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_find_by_name().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "existing-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc.create_category("Bonds").await.unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Application(CategoryApplicationError::DuplicateName)
            ),
            "got: {err:?}"
        );
    }

    // R1 — duplicate name, different case: service checks find_by_name (case-insensitive lookup is the repo's concern)
    #[tokio::test]
    async fn test_create_category_rejects_duplicate_different_case() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_find_by_name().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "existing-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc.create_category("bonds").await.unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Application(CategoryApplicationError::DuplicateName)
            ),
            "got: {err:?}"
        );
    }

    // R2 — system category cannot be renamed (check moved into AssetCategory::update_from)
    #[tokio::test]
    async fn test_update_category_rejects_system_category() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                SYSTEM_CATEGORY_ID.to_string(),
                "uncategorized".to_string(),
            )))
        });
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .update_category(SYSTEM_CATEGORY_ID, "Renamed")
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Validation(CategoryDomainError::SystemReadonly)
            ),
            "got: {err:?}"
        );
    }

    // R1 — update with name already taken by a different category
    #[tokio::test]
    async fn test_update_category_rejects_duplicate_name() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "cat2-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        cr.expect_find_by_name().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "other-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc.update_category("cat2-id", "bonds").await.unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Application(CategoryApplicationError::DuplicateName)
            ),
            "got: {err:?}"
        );
    }

    // R2 — system category cannot be deleted (check moved into AssetCategory::ensure_deletable)
    #[tokio::test]
    async fn test_delete_category_rejects_system_category() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                SYSTEM_CATEGORY_ID.to_string(),
                "uncategorized".to_string(),
            )))
        });
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc.delete_category(SYSTEM_CATEGORY_ID).await.unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Validation(CategoryDomainError::SystemProtected)
            ),
            "got: {err:?}"
        );
    }

    // R3 — service calls reassign_assets_and_delete with the category id and system fallback
    #[tokio::test]
    async fn test_delete_category_reassigns_assets_to_default() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "bonds-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        cr.expect_reassign_assets_and_delete()
            .withf(|cat_id, fallback_id| cat_id == "bonds-id" && fallback_id == SYSTEM_CATEGORY_ID)
            .times(1)
            .return_once(|_, _| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        svc.delete_category("bonds-id").await.unwrap();
    }

    // MKT-043 — record_asset_price rejects unknown asset
    #[tokio::test]
    async fn test_record_asset_price_rejects_unknown_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id().times(1).return_once(|_| Ok(None));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .record_asset_price("nonexistent-id", "2026-01-01", 100.0)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::NotFound(_))
            ),
            "got: {err}"
        );
    }

    // MKT-021 — record_asset_price rejects price <= 0
    #[tokio::test]
    async fn test_record_asset_price_rejects_non_positive_price() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-id", false))));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .record_asset_price("asset-id", "2026-01-01", 0.0)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::NotPositive)
            ),
            "got: {err}"
        );
    }

    // MKT-022 — record_asset_price rejects a future date
    #[tokio::test]
    async fn test_record_asset_price_rejects_future_date() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-id", false))));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .record_asset_price("asset-id", "2099-12-31", 100.0)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::DateInFuture)
            ),
            "got: {err}"
        );
    }

    // MKT-025, MKT-026 — record_asset_price calls upsert with correct micros and publishes event
    #[tokio::test]
    async fn test_record_asset_price_upserts_and_publishes_event_on_success() {
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-id", false))));
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_upsert()
            .withf(|p| p.asset_id == "asset-id" && p.date == "2026-01-01" && p.price == 150_500_000)
            .times(1)
            .return_once(|_| Ok(()));
        let svc = make_svc(ar, MockAssetCategoryRepository::new(), pr).with_event_bus(bus);
        svc.record_asset_price("asset-id", "2026-01-01", 150.5)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(200), rx.changed())
            .await
            .expect("event not received within 200ms")
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::AssetPriceUpdated);
    }

    // MKT-057 — notify_asset_price_updated publishes AssetPriceUpdated when a bus is configured
    #[tokio::test]
    async fn notify_asset_price_updated_publishes_event() {
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(bus);
        svc.notify_asset_price_updated();
        tokio::time::timeout(Duration::from_millis(200), rx.changed())
            .await
            .expect("event not received within 200ms")
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::AssetPriceUpdated);
    }

    // MKT-031 — get_latest_price returns None when no price has been recorded
    #[tokio::test]
    async fn get_latest_price_returns_none_when_no_price_recorded() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_latest().times(1).return_once(|_| Ok(None));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        let result = svc.get_latest_price("asset-id").await.unwrap();
        assert!(result.is_none());
    }

    // MKT-031 — get_latest_price delegates to price_repo.get_latest and returns its result
    #[tokio::test]
    async fn get_latest_price_returns_latest_price_when_one_exists() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_latest()
            .times(1)
            .return_once(|_| Ok(Some(make_price("asset-id", "2026-01-03", 120_000_000))));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        let latest = svc.get_latest_price("asset-id").await.unwrap().unwrap();
        assert_eq!(latest.date, "2026-01-03");
        assert_eq!(latest.price, 120_000_000);
    }

    // -------------------------------------------------------------------------
    // get_asset_prices (MKT-072)
    // -------------------------------------------------------------------------

    // MKT-072 — get_asset_prices returns AssetNotFound for a nonexistent asset_id
    #[tokio::test]
    async fn test_get_asset_prices_rejects_unknown_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id().times(1).return_once(|_| Ok(None));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc.get_asset_prices("nonexistent-id").await.unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::NotFound(_))
            ),
            "got: {err}"
        );
    }

    // MKT-072 — get_asset_prices returns an empty list when the asset exists but has no prices
    #[tokio::test]
    async fn test_get_asset_prices_returns_empty_list_when_no_prices() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-id", false))));
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_all_for_asset()
            .times(1)
            .return_once(|_| Ok(vec![]));
        let svc = make_svc(ar, MockAssetCategoryRepository::new(), pr);
        let prices = svc.get_asset_prices("asset-id").await.unwrap();
        assert!(prices.is_empty());
    }

    // MKT-072 — get_asset_prices passes through whatever order price_repo returns
    #[tokio::test]
    async fn test_get_asset_prices_returns_all_records_sorted_date_descending() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-id", false))));
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_all_for_asset().times(1).return_once(|_| {
            Ok(vec![
                make_price("asset-id", "2026-01-03", 130_000_000),
                make_price("asset-id", "2026-01-02", 120_000_000),
                make_price("asset-id", "2026-01-01", 100_000_000),
            ])
        });
        let svc = make_svc(ar, MockAssetCategoryRepository::new(), pr);
        let prices = svc.get_asset_prices("asset-id").await.unwrap();
        assert_eq!(prices.len(), 3);
        assert_eq!(prices[0].date, "2026-01-03");
        assert_eq!(prices[1].date, "2026-01-02");
        assert_eq!(prices[2].date, "2026-01-01");
        assert_eq!(prices[0].price, 130_000_000);
        assert_eq!(prices[1].price, 120_000_000);
        assert_eq!(prices[2].price, 100_000_000);
    }

    // MKT-072 — get_asset_prices calls price_repo with the requested asset_id
    #[tokio::test]
    async fn test_get_asset_prices_scoped_to_requested_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .withf(|id| id == "asset-a-id")
            .times(1)
            .return_once(|_| Ok(Some(make_asset("asset-a-id", false))));
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_all_for_asset()
            .withf(|id| id == "asset-a-id")
            .times(1)
            .return_once(|_| Ok(vec![make_price("asset-a-id", "2026-01-01", 100_000_000)]));
        let svc = make_svc(ar, MockAssetCategoryRepository::new(), pr);
        let prices = svc.get_asset_prices("asset-a-id").await.unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].asset_id, "asset-a-id");
    }

    // -------------------------------------------------------------------------
    // update_asset_price (MKT-082, MKT-083, MKT-084, MKT-085)
    // -------------------------------------------------------------------------

    // MKT-082 — validation runs before any repo call; no mock expectations needed
    #[tokio::test]
    async fn test_update_asset_price_rejects_non_positive_price() {
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .update_asset_price("asset-id", "2026-01-01", "2026-01-01", 0.0)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::NotPositive)
            ),
            "got: {err}"
        );
    }

    // MKT-082 — non-finite check runs before micro conversion; no repo calls
    #[tokio::test]
    async fn test_update_asset_price_rejects_non_finite_price() {
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .update_asset_price("asset-id", "2026-01-01", "2026-01-01", f64::NAN)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::NonFinite)
            ),
            "got: {err}"
        );
    }

    // MKT-082 — future new_date rejected by AssetPrice::new before DB lookup
    #[tokio::test]
    async fn test_update_asset_price_rejects_future_date() {
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .update_asset_price("asset-id", "2026-01-01", "2099-12-31", 150.0)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::DateInFuture)
            ),
            "got: {err}"
        );
    }

    // MKT-083 — returns NotFound when get_by_asset_and_date returns None
    #[tokio::test]
    async fn test_update_asset_price_returns_not_found_for_missing_record() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(None));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        let err = svc
            .update_asset_price("asset-id", "2026-01-01", "2026-01-01", 100.0)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::NotFound)
            ),
            "got: {err}"
        );
    }

    // MKT-083 — same original_date and new_date: service calls upsert (not replace_atomic)
    #[tokio::test]
    async fn test_update_asset_price_same_date_updates_price_in_place() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(Some(make_price("asset-id", "2026-01-01", 100_000_000))));
        pr.expect_upsert()
            .withf(|p| p.date == "2026-01-01" && p.price == 150_000_000)
            .times(1)
            .return_once(|_| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        svc.update_asset_price("asset-id", "2026-01-01", "2026-01-01", 150.0)
            .await
            .unwrap();
    }

    // MKT-084 — different dates: service calls replace_atomic with original_date and new price
    #[tokio::test]
    async fn test_update_asset_price_date_change_deletes_old_and_upserts_new() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(Some(make_price("asset-id", "2026-01-01", 100_000_000))));
        pr.expect_replace_atomic()
            .withf(|id, orig, new_p| {
                id == "asset-id"
                    && orig == "2026-01-01"
                    && new_p.date == "2026-01-02"
                    && new_p.price == 110_000_000
            })
            .times(1)
            .return_once(|_, _, _| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        svc.update_asset_price("asset-id", "2026-01-01", "2026-01-02", 110.0)
            .await
            .unwrap();
    }

    // MKT-084 — date change always calls replace_atomic regardless of whether target date exists
    #[tokio::test]
    async fn test_update_asset_price_date_change_overwrites_existing_target_date() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(Some(make_price("asset-id", "2026-01-01", 100_000_000))));
        pr.expect_replace_atomic()
            .withf(|id, orig, new_p| {
                id == "asset-id"
                    && orig == "2026-01-01"
                    && new_p.date == "2026-01-02"
                    && new_p.price == 200_000_000
            })
            .times(1)
            .return_once(|_, _, _| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        svc.update_asset_price("asset-id", "2026-01-01", "2026-01-02", 200.0)
            .await
            .unwrap();
    }

    // MKT-085 — publishes AssetPriceUpdated after a successful update
    #[tokio::test]
    async fn test_update_asset_price_publishes_asset_price_updated_event() {
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(Some(make_price("asset-id", "2026-01-01", 100_000_000))));
        pr.expect_upsert().times(1).return_once(|_| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        )
        .with_event_bus(bus);
        svc.update_asset_price("asset-id", "2026-01-01", "2026-01-01", 150.0)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(200), rx.changed())
            .await
            .expect("event not received within 200ms")
            .unwrap();
        assert_eq!(*rx.borrow(), Event::AssetPriceUpdated);
    }

    // -------------------------------------------------------------------------
    // delete_asset_price (MKT-090, MKT-091)
    // -------------------------------------------------------------------------

    // MKT-090 — returns NotFound when get_by_asset_and_date returns None
    #[tokio::test]
    async fn delete_asset_price_returns_not_found_for_missing_record() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(None));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        let err = svc
            .delete_asset_price("asset-id", "2026-01-01")
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetPriceDomainError>(),
                Some(AssetPriceDomainError::NotFound)
            ),
            "got: {err}"
        );
    }

    // MKT-090 — calls price_repo.delete with the correct (asset_id, date) after existence check
    #[tokio::test]
    async fn delete_asset_price_removes_the_record() {
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(Some(make_price("asset-id", "2026-01-01", 100_000_000))));
        pr.expect_delete()
            .withf(|id, date| id == "asset-id" && date == "2026-01-01")
            .times(1)
            .return_once(|_, _| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        );
        svc.delete_asset_price("asset-id", "2026-01-01")
            .await
            .unwrap();
    }

    // MKT-091 — publishes AssetPriceUpdated after a successful delete
    #[tokio::test]
    async fn delete_asset_price_publishes_asset_price_updated_event() {
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let mut pr = MockAssetPriceRepository::new();
        pr.expect_get_by_asset_and_date()
            .times(1)
            .return_once(|_, _| Ok(Some(make_price("asset-id", "2026-01-01", 100_000_000))));
        pr.expect_delete().times(1).return_once(|_, _| Ok(()));
        let svc = make_svc(
            MockAssetRepository::new(),
            MockAssetCategoryRepository::new(),
            pr,
        )
        .with_event_bus(bus);
        svc.delete_asset_price("asset-id", "2026-01-01")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(200), rx.changed())
            .await
            .expect("event not received within 200ms")
            .unwrap();
        assert_eq!(*rx.borrow(), Event::AssetPriceUpdated);
    }

    // MKT-043 — record_asset_price command returns AssetNotFound for an unknown asset_id.
    // This is covered by the existing record_asset_price_rejects_unknown_asset service test above.
    // The command-layer mapping is exercised by the api.rs tests below that verify
    // to_asset_price_error maps AssetDomainError::NotFound → AssetPriceCommandError::AssetNotFound.

    // ── Mock-based unit tests for event bus branches and error paths ──────────

    #[tokio::test]
    async fn update_asset_returns_archived_error() {
        let archived_asset = make_asset("a-id", true);
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(move |_| Ok(Some(archived_asset)));
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(ar, cr, MockAssetPriceRepository::new());

        let err = svc
            .update_asset(UpdateAssetDTO {
                asset_id: "a-id".to_string(),
                name: "New".to_string(),
                reference: "REF".to_string(),
                class: AssetClass::Cash,
                currency: "USD".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap_err();

        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::Archived)
            ),
            "expected Archived, got: {err}"
        );
    }

    #[tokio::test]
    async fn update_asset_returns_category_not_found() {
        let active_asset = make_asset("a-id", false);
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(move |_| Ok(Some(active_asset)));
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().return_once(|_| Ok(None));
        let svc = make_svc(ar, cr, MockAssetPriceRepository::new());

        let err = svc
            .update_asset(UpdateAssetDTO {
                asset_id: "a-id".to_string(),
                name: "New".to_string(),
                reference: "REF".to_string(),
                class: AssetClass::Cash,
                currency: "USD".to_string(),
                risk_level: 1,
                category_id: "missing-cat".to_string(),
            })
            .await
            .unwrap_err();

        assert!(
            matches!(
                err.downcast_ref::<CategoryApplicationError>(),
                Some(CategoryApplicationError::NotFound { .. })
            ),
            "expected CategoryApplicationError::NotFound, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_archive_asset_emits_event_when_bus_present() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_asset("a-id", false))));
        ar.expect_archive().times(1).return_once(|_| Ok(()));
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(Arc::clone(&bus));

        svc.archive_asset("a-id").await.unwrap();

        rx.changed()
            .await
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::AssetUpdated);
    }

    #[tokio::test]
    async fn test_unarchive_asset_emits_event_when_bus_present() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_asset("a-id", true))));
        ar.expect_unarchive().times(1).return_once(|_| Ok(()));
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(Arc::clone(&bus));

        svc.unarchive_asset("a-id").await.unwrap();

        rx.changed()
            .await
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::AssetUpdated);
    }

    #[tokio::test]
    async fn test_delete_asset_emits_event_when_bus_present() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_asset("a-id", false))));
        ar.expect_delete().times(1).return_once(|_| Ok(()));
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(Arc::clone(&bus));

        svc.delete_asset("a-id").await.unwrap();

        rx.changed()
            .await
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::AssetUpdated);
    }

    // CSH-016 — update_asset rejects a system Cash Asset
    #[tokio::test]
    async fn test_update_asset_rejects_cash_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_cash_asset("system-cash-USD"))));
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id()
            .return_once(|_| Ok(Some(make_category())));
        let svc = make_svc(ar, cr, MockAssetPriceRepository::new());

        let err = svc
            .update_asset(UpdateAssetDTO {
                asset_id: "system-cash-USD".to_string(),
                name: "Renamed".to_string(),
                reference: "USD".to_string(),
                class: AssetClass::Cash,
                currency: "USD".to_string(),
                risk_level: 1,
                category_id: SYSTEM_CATEGORY_ID.to_string(),
            })
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::CashAssetNotEditable)
            ),
            "got: {err}"
        );
    }

    // CSH-016 — archive_asset rejects a system Cash Asset
    #[tokio::test]
    async fn test_archive_asset_rejects_cash_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_cash_asset("system-cash-USD"))));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );

        let err = svc.archive_asset("system-cash-USD").await.unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::CashAssetNotEditable)
            ),
            "got: {err}"
        );
    }

    // CSH-016 — unarchive_asset rejects a system Cash Asset
    #[tokio::test]
    async fn test_unarchive_asset_rejects_cash_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_cash_asset("system-cash-USD"))));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );

        let err = svc.unarchive_asset("system-cash-USD").await.unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::CashAssetNotEditable)
            ),
            "got: {err}"
        );
    }

    // CSH-016 — delete_asset rejects a system Cash Asset
    #[tokio::test]
    async fn test_delete_asset_rejects_cash_asset() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id()
            .return_once(|_| Ok(Some(make_cash_asset("system-cash-USD"))));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );

        let err = svc.delete_asset("system-cash-USD").await.unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::CashAssetNotEditable)
            ),
            "got: {err}"
        );
    }

    // CSH-016 — archive_asset surfaces NotFound when the id is unknown
    #[tokio::test]
    async fn test_archive_asset_returns_not_found_for_unknown_id() {
        let mut ar = MockAssetRepository::new();
        ar.expect_get_by_id().return_once(|_| Ok(None));
        let svc = make_svc(
            ar,
            MockAssetCategoryRepository::new(),
            MockAssetPriceRepository::new(),
        );

        let err = svc.archive_asset("missing").await.unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<AssetDomainError>(),
                Some(AssetDomainError::NotFound(_))
            ),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn test_create_category_emits_event_when_bus_present() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_find_by_name().times(1).return_once(|_| Ok(None));
        cr.expect_create().times(1).return_once(Ok);
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(Arc::clone(&bus));

        svc.create_category("NewCat").await.unwrap();

        rx.changed()
            .await
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::CategoryUpdated);
    }

    #[tokio::test]
    async fn test_update_category_emits_event_when_bus_present() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "some-id".to_string(),
                "Old".to_string(),
            )))
        });
        cr.expect_find_by_name().times(1).return_once(|_| Ok(None));
        cr.expect_update().times(1).return_once(Ok);
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(Arc::clone(&bus));

        svc.update_category("some-id", "Updated").await.unwrap();

        rx.changed()
            .await
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::CategoryUpdated);
    }

    #[tokio::test]
    async fn test_delete_category_emits_event_when_bus_present() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "cat-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        cr.expect_reassign_assets_and_delete()
            .times(1)
            .return_once(|_, _| Ok(()));
        let bus = Arc::new(SideEffectEventBus::new());
        let mut rx = bus.subscribe();
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        )
        .with_event_bus(Arc::clone(&bus));

        svc.delete_category("some-cat-id").await.unwrap();

        rx.changed()
            .await
            .expect("watch sender dropped before event fired");
        assert_eq!(*rx.borrow(), Event::CategoryUpdated);
    }

    // -------------------------------------------------------------------------
    // Category CRUD typed-error coverage (PR 6 — first PR enforcing the new
    // gold infra-translation rule: per-BC `*ApplicationError::DatabaseError`
    // with no `hint` payload; full diagnostic preserved server-side via
    // `tracing::error!` only.)
    // -------------------------------------------------------------------------

    #[derive(Debug, thiserror::Error)]
    #[error("simulated DB failure")]
    struct SimulatedDbError;

    // PR 6 — create_category surfaces find_by_name repo failure as
    // CategoryApplicationError::DatabaseError (no payload — diagnostic stays
    // in tracing). Exercises the typed-error contract for the uniqueness
    // pre-check failure path.
    #[tokio::test]
    async fn test_create_category_returns_database_error_when_find_by_name_fails() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_find_by_name()
            .times(1)
            .return_once(|_| Err(SimulatedDbError.into()));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc.create_category("anything").await.unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Application(CategoryApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }

    // PR 6 — update_category surfaces get_by_id Ok(None) as a typed
    // application-class NotFound carrying the requested ID. Distinct from the
    // get_by_id Err path (which becomes DatabaseError).
    #[tokio::test]
    async fn test_update_category_returns_not_found_when_aggregate_missing() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| Ok(None));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc
            .update_category("missing-id", "Anything")
            .await
            .unwrap_err();
        assert!(
            matches!(
                &err,
                CategoryCrudError::Application(CategoryApplicationError::NotFound { id })
                    if id == "missing-id"
            ),
            "got: {err:?}"
        );
    }

    // PR 6 — delete_category surfaces reassign_assets_and_delete repo failure
    // as DatabaseError. Distinct from the get_by_id Ok(None) path (NotFound)
    // and the ensure_deletable path (Validation::SystemProtected).
    #[tokio::test]
    async fn test_delete_category_returns_database_error_when_reassign_fails() {
        let mut cr = MockAssetCategoryRepository::new();
        cr.expect_get_by_id().times(1).return_once(|_| {
            Ok(Some(AssetCategory::from_storage(
                "some-id".to_string(),
                "Bonds".to_string(),
            )))
        });
        cr.expect_reassign_assets_and_delete()
            .times(1)
            .return_once(|_, _| Err(SimulatedDbError.into()));
        let svc = make_svc(
            MockAssetRepository::new(),
            cr,
            MockAssetPriceRepository::new(),
        );
        let err = svc.delete_category("some-id").await.unwrap_err();
        assert!(
            matches!(
                err,
                CategoryCrudError::Application(CategoryApplicationError::DatabaseError)
            ),
            "got: {err:?}"
        );
    }
}
