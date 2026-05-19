use std::str::FromStr;

use super::super::domain::{exchange, Asset, AssetCategory, AssetClass, AssetRepository};
use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};

#[derive(sqlx::FromRow)]
struct AssetRow {
    id: String,
    name: String,
    reference: String,
    asset_class: String,
    currency: String,
    risk_level: i64,
    category_id: String,
    category_name: String,
    is_archived: bool,
    exchange_code: Option<String>,
}

impl From<AssetRow> for Asset {
    fn from(row: AssetRow) -> Self {
        let asset_class = AssetClass::from_str(&row.asset_class).unwrap_or_default();
        let exchange = row.exchange_code.as_deref().and_then(exchange::lookup);
        Asset::restore(
            row.id,
            row.name,
            asset_class,
            AssetCategory::from_storage(row.category_id, row.category_name),
            row.currency,
            row.risk_level.try_into().unwrap_or(0),
            row.reference,
            row.is_archived,
            exchange,
        )
    }
}

/// SQLite implementation of the AssetRepository.
#[derive(Clone)]
pub struct SqliteAssetRepository {
    pool: Pool<Sqlite>,
}

impl SqliteAssetRepository {
    /// Creates a new SqliteAssetRepository.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AssetRepository for SqliteAssetRepository {
    async fn get_all(&self) -> Result<Vec<Asset>> {
        let rows = sqlx::query_as!(
            AssetRow,
            r#"
            SELECT
                a.id, a.name, a.reference, a.asset_class, a.currency, a.risk_level,
                c.id as category_id,
                c.name as category_name,
                a.is_archived as "is_archived: bool",
                a.exchange_code
            FROM assets a
            JOIN categories c ON a.category_id = c.id
            WHERE a.is_deleted = 0 AND a.is_archived = 0 AND c.is_deleted = 0
            "#
        )
        .fetch_all(&self.pool)
        .await
        .with_context(|| "Failed to fetch assets from database")?;

        Ok(rows.into_iter().map(Asset::from).collect())
    }

    async fn get_all_including_archived(&self) -> Result<Vec<Asset>> {
        let rows = sqlx::query_as!(
            AssetRow,
            r#"
            SELECT
                a.id, a.name, a.reference, a.asset_class, a.currency, a.risk_level,
                c.id as category_id,
                c.name as category_name,
                a.is_archived as "is_archived: bool",
                a.exchange_code
            FROM assets a
            JOIN categories c ON a.category_id = c.id
            WHERE a.is_deleted = 0 AND c.is_deleted = 0
            "#
        )
        .fetch_all(&self.pool)
        .await
        .with_context(|| "Failed to fetch assets including archived from database")?;

        Ok(rows.into_iter().map(Asset::from).collect())
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Asset>> {
        let row = sqlx::query_as!(
            AssetRow,
            r#"
            SELECT
                a.id, a.name, a.reference, a.asset_class, a.currency, a.risk_level,
                c.id as category_id,
                c.name as category_name,
                a.is_archived as "is_archived: bool",
                a.exchange_code
            FROM assets a
            JOIN categories c ON a.category_id = c.id
            WHERE a.id = ?
                AND a.is_deleted = 0
                AND c.is_deleted = 0
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Failed to fetch asset with id: {}", id))?;

        Ok(row.map(Asset::from))
    }

    async fn create(&self, asset: Asset) -> Result<Asset> {
        let asset_class_str = asset.class.to_string();
        let exchange_code = asset.exchange.as_ref().map(|e| e.code.clone());
        sqlx::query!(
            r#"INSERT INTO assets (id, name, reference, asset_class, currency, risk_level, is_deleted, is_archived, category_id, exchange_code) VALUES (?, ?, ?, ?, ?, ?, 0, 0, ?, ?)"#,
            asset.id,
            asset.name,
            asset.reference,
            asset_class_str,
            asset.currency,
            asset.risk_level,
            asset.category.id,
            exchange_code
        )
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to create asset: {}", asset.name))?;
        Ok(asset)
    }

    async fn update(&self, asset: Asset) -> Result<Asset> {
        let asset_class_str = asset.class.to_string();
        let exchange_code = asset.exchange.as_ref().map(|e| e.code.clone());
        sqlx::query!(
            r#"UPDATE assets SET name = ?, reference = ?, asset_class = ?, currency = ?, risk_level = ?, category_id = ?, exchange_code = ? WHERE id = ? AND is_archived = 0"#,
            asset.name,
            asset.reference,
            asset_class_str,
            asset.currency,
            asset.risk_level,
            asset.category.id,
            exchange_code,
            asset.id
        )
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to update asset with id: {}", asset.id))?;
        Ok(asset)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query!(r#"UPDATE assets SET is_deleted = 1 WHERE id = ?"#, id)
            .execute(&self.pool)
            .await
            .with_context(|| format!("Failed to soft delete asset with id: {}", id))?;
        Ok(())
    }

    async fn archive(&self, id: &str) -> Result<()> {
        sqlx::query!(
            r#"UPDATE assets SET is_archived = 1 WHERE id = ? AND is_deleted = 0"#,
            id
        )
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to archive asset with id: {}", id))?;
        Ok(())
    }

    async fn unarchive(&self, id: &str) -> Result<()> {
        sqlx::query!(
            r#"UPDATE assets SET is_archived = 0 WHERE id = ? AND is_deleted = 0"#,
            id
        )
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to unarchive asset with id: {}", id))?;
        Ok(())
    }
}
