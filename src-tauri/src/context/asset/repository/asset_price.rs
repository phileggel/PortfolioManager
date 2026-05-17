use super::super::domain::{AssetPrice, AssetPriceRepository, AssetPriceSource};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};

/// SQLite implementation of AssetPriceRepository.
pub struct SqliteAssetPriceRepository {
    pool: Pool<Sqlite>,
}

impl SqliteAssetPriceRepository {
    /// Creates a new repository backed by the given connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AssetPriceRepository for SqliteAssetPriceRepository {
    async fn upsert(&self, price: AssetPrice) -> Result<()> {
        let source = price.source.as_str();
        sqlx::query!(
            "INSERT INTO asset_prices (asset_id, date, price, source) VALUES (?, ?, ?, ?)
             ON CONFLICT(asset_id, date) DO UPDATE SET price = excluded.price, source = excluded.source",
            price.asset_id,
            price.date,
            price.price,
            source,
        )
        .execute(&self.pool)
        .await
        .context("Failed to upsert asset price")?;
        Ok(())
    }

    async fn get_latest(&self, asset_id: &str) -> Result<Option<AssetPrice>> {
        let row = sqlx::query!(
            "SELECT asset_id, date, price, source FROM asset_prices WHERE asset_id = ? ORDER BY date DESC LIMIT 1",
            asset_id,
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch latest asset price")?;

        Ok(row.map(|r| {
            AssetPrice::restore(
                r.asset_id,
                r.date,
                r.price,
                AssetPriceSource::from_storage(&r.source),
            )
        }))
    }

    async fn get_all_for_asset(&self, asset_id: &str) -> Result<Vec<AssetPrice>> {
        let rows = sqlx::query!(
            "SELECT asset_id, date, price, source FROM asset_prices WHERE asset_id = ? ORDER BY date DESC",
            asset_id,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch asset prices")?;

        Ok(rows
            .into_iter()
            .map(|r| {
                AssetPrice::restore(
                    r.asset_id,
                    r.date,
                    r.price,
                    AssetPriceSource::from_storage(&r.source),
                )
            })
            .collect())
    }

    async fn get_by_asset_and_date(
        &self,
        asset_id: &str,
        date: &str,
    ) -> Result<Option<AssetPrice>> {
        let row = sqlx::query!(
            "SELECT asset_id, date, price, source FROM asset_prices WHERE asset_id = ? AND date = ?",
            asset_id,
            date,
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch asset price by date")?;

        Ok(row.map(|r| {
            AssetPrice::restore(
                r.asset_id,
                r.date,
                r.price,
                AssetPriceSource::from_storage(&r.source),
            )
        }))
    }

    async fn delete(&self, asset_id: &str, date: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM asset_prices WHERE asset_id = ? AND date = ?",
            asset_id,
            date,
        )
        .execute(&self.pool)
        .await
        .context("Failed to delete asset price")?;
        Ok(())
    }

    async fn replace_atomic(
        &self,
        asset_id: &str,
        original_date: &str,
        new_price: AssetPrice,
    ) -> Result<()> {
        debug_assert_eq!(
            asset_id, new_price.asset_id,
            "replace_atomic: asset_id parameter must match new_price.asset_id"
        );
        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin transaction")?;

        sqlx::query!(
            "DELETE FROM asset_prices WHERE asset_id = ? AND date = ?",
            asset_id,
            original_date,
        )
        .execute(&mut *tx)
        .await
        .context("Failed to delete original asset price")?;

        let source = new_price.source.as_str();
        sqlx::query!(
            "INSERT INTO asset_prices (asset_id, date, price, source) VALUES (?, ?, ?, ?)
             ON CONFLICT(asset_id, date) DO UPDATE SET price = excluded.price, source = excluded.source",
            new_price.asset_id,
            new_price.date,
            new_price.price,
            source,
        )
        .execute(&mut *tx)
        .await
        .context("Failed to upsert new asset price")?;

        tx.commit()
            .await
            .context("Failed to commit price replacement")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> Pool<Sqlite> {
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

    /// Inserts a throwaway asset row so FK constraints on asset_prices are satisfied.
    async fn seed_asset(pool: &Pool<Sqlite>, asset_id: &str) {
        sqlx::query!(
            "INSERT INTO assets (id, name, reference, asset_class, currency, risk_level, category_id, is_archived)
             VALUES (?, 'Test Asset', 'REF', 'cash', 'USD', 1, 'default-uncategorized', 0)",
            asset_id,
        )
        .execute(pool)
        .await
        .expect("seed asset");
    }

    // -------------------------------------------------------------------------
    // MKT-100 — source column round-trips through upsert → get_latest / get_all_for_asset
    // / get_by_asset_and_date. These fail until the migration adds the column and
    // the repository methods read/write it.
    // -------------------------------------------------------------------------

    // MKT-100 / MKT-102 — upsert with source=Stooq and read back via get_latest
    #[tokio::test]
    async fn upsert_and_get_latest_roundtrip_source_stooq() {
        use crate::context::asset::AssetPriceSource;
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Stooq,
        ))
        .await
        .unwrap();

        let price = repo.get_latest("asset-1").await.unwrap().unwrap();
        assert_eq!(price.source, AssetPriceSource::Stooq);
    }

    // MKT-100 / MKT-101 — upsert with source=Manual and read back via get_by_asset_and_date
    #[tokio::test]
    async fn upsert_and_get_by_date_roundtrip_source_manual() {
        use crate::context::asset::AssetPriceSource;
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            50_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        let price = repo
            .get_by_asset_and_date("asset-1", "2026-01-01")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(price.source, AssetPriceSource::Manual);
    }

    // MKT-100 — get_all_for_asset includes source on every returned row
    #[tokio::test]
    async fn get_all_for_asset_includes_source_field() {
        use crate::context::asset::AssetPriceSource;
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();
        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            110_000_000,
            AssetPriceSource::Stooq,
        ))
        .await
        .unwrap();

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert_eq!(prices.len(), 2);
        // Sorted date desc: 2026-01-02 first
        assert_eq!(prices[0].source, AssetPriceSource::Stooq);
        assert_eq!(prices[1].source, AssetPriceSource::Manual);
    }

    // MKT-100 — replace_atomic preserves source on the new price row
    #[tokio::test]
    async fn replace_atomic_preserves_source_on_new_row() {
        use crate::context::asset::AssetPriceSource;
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Stooq,
        ))
        .await
        .unwrap();

        let new_price = AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            110_000_000,
            AssetPriceSource::Manual,
        );
        repo.replace_atomic("asset-1", "2026-01-01", new_price)
            .await
            .unwrap();

        let price = repo.get_latest("asset-1").await.unwrap().unwrap();
        assert_eq!(price.source, AssetPriceSource::Manual);
        assert_eq!(price.date, "2026-01-02");
    }

    // -------------------------------------------------------------------------

    // get_all_for_asset — returns all rows for the given asset, sorted date descending (MKT-072)
    #[tokio::test]
    async fn get_all_for_asset_returns_rows_date_descending() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();
        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-03".into(),
            130_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();
        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            120_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert_eq!(prices.len(), 3);
        assert_eq!(prices[0].date, "2026-01-03");
        assert_eq!(prices[1].date, "2026-01-02");
        assert_eq!(prices[2].date, "2026-01-01");
    }

    // get_all_for_asset — returns empty list when no prices exist for the asset (MKT-072)
    #[tokio::test]
    async fn get_all_for_asset_returns_empty_list_when_none_recorded() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert!(prices.is_empty());
    }

    // get_all_for_asset — does not return rows belonging to a different asset
    #[tokio::test]
    async fn get_all_for_asset_scoped_to_requested_asset() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        seed_asset(&pool, "asset-2").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();
        repo.upsert(AssetPrice::restore(
            "asset-2".into(),
            "2026-01-01".into(),
            200_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].asset_id, "asset-1");
    }

    // get_by_asset_and_date — returns the record when it exists (MKT-083)
    #[tokio::test]
    async fn get_by_asset_and_date_returns_record_when_present() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        let result = repo
            .get_by_asset_and_date("asset-1", "2026-01-01")
            .await
            .unwrap();
        assert!(result.is_some());
        let price = result.unwrap();
        assert_eq!(price.asset_id, "asset-1");
        assert_eq!(price.date, "2026-01-01");
        assert_eq!(price.price, 100_000_000);
    }

    // get_by_asset_and_date — returns None when no record for that (asset_id, date) exists
    #[tokio::test]
    async fn get_by_asset_and_date_returns_none_when_absent() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        let result = repo
            .get_by_asset_and_date("asset-1", "2026-01-01")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    // delete — removes the record when it exists (MKT-090)
    #[tokio::test]
    async fn delete_removes_the_record() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();
        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            110_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        repo.delete("asset-1", "2026-01-01").await.unwrap();

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].date, "2026-01-02");
    }

    // delete — is a no-op (does not error) when the record does not exist
    // (presence check is the service's responsibility, not the repo's)
    #[tokio::test]
    async fn delete_is_noop_when_record_absent() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        // Should not error even though no record exists
        let result = repo.delete("asset-1", "2026-01-01").await;
        assert!(result.is_ok());
    }

    // replace_atomic — deletes original_date and upserts at new_date atomically (MKT-084)
    #[tokio::test]
    async fn replace_atomic_moves_price_to_new_date() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        let new_price = AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            110_000_000,
            AssetPriceSource::Manual,
        );
        repo.replace_atomic("asset-1", "2026-01-01", new_price)
            .await
            .unwrap();

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert_eq!(prices.len(), 1, "old date must be gone");
        assert_eq!(prices[0].date, "2026-01-02");
        assert_eq!(prices[0].price, 110_000_000);
    }

    // replace_atomic — overwrites an existing record at new_date (MKT-084, silent overwrite)
    #[tokio::test]
    async fn replace_atomic_overwrites_existing_record_at_new_date() {
        let pool = setup_pool().await;
        seed_asset(&pool, "asset-1").await;
        let repo = SqliteAssetPriceRepository::new(pool);

        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-01".into(),
            100_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();
        repo.upsert(AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            105_000_000,
            AssetPriceSource::Manual,
        ))
        .await
        .unwrap();

        // Move 2026-01-01 to 2026-01-02 — must overwrite 105_000_000
        let new_price = AssetPrice::restore(
            "asset-1".into(),
            "2026-01-02".into(),
            200_000_000,
            AssetPriceSource::Manual,
        );
        repo.replace_atomic("asset-1", "2026-01-01", new_price)
            .await
            .unwrap();

        let prices = repo.get_all_for_asset("asset-1").await.unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].date, "2026-01-02");
        assert_eq!(prices[0].price, 200_000_000);
    }
}
