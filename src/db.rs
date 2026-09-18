use crate::{
    error::{AppError, AppResult},
    models::{
        Asset, AssetQuery, AssetRow, AssetVersion, CreateProjectRequest, PackageFile, Project,
        ProjectAsset, ProjectAssetRequest, StatsResponse, UpdateAssetRequest, UpdateProjectRequest,
    },
};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::path::Path;
use uuid::Uuid;

pub async fn connect(path: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    initialize(&pool).await?;
    Ok(pool)
}

async fn initialize(pool: &SqlitePool) -> anyhow::Result<()> {
    const STATEMENTS: &[&str] = &[
        r#"
        CREATE TABLE IF NOT EXISTS assets (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            extension TEXT,
            mime_type TEXT,
            byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
            sha256 TEXT NOT NULL UNIQUE,
            storage_path TEXT NOT NULL,
            category TEXT,
            description TEXT,
            source_url TEXT,
            creator TEXT,
            license TEXT,
            attribution_required INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS asset_tags (
            asset_id TEXT NOT NULL,
            tag TEXT NOT NULL COLLATE NOCASE,
            PRIMARY KEY(asset_id, tag),
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            engine TEXT NOT NULL DEFAULT 'Generic',
            local_path TEXT NOT NULL,
            description TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS project_assets (
            project_id TEXT NOT NULL,
            asset_id TEXT NOT NULL,
            relative_path TEXT,
            version_number INTEGER NOT NULL DEFAULT 1,
            added_at TEXT NOT NULL,
            PRIMARY KEY(project_id, asset_id),
            FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS package_files (
            id TEXT PRIMARY KEY NOT NULL,
            asset_id TEXT NOT NULL,
            version_number INTEGER NOT NULL,
            relative_path TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            extension TEXT,
            mime_type TEXT,
            byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
            sha256 TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            is_primary INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            UNIQUE(asset_id, version_number, relative_path),
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS package_dependencies (
            asset_id TEXT NOT NULL,
            version_number INTEGER NOT NULL,
            dependency_path TEXT NOT NULL,
            missing INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(asset_id, version_number, dependency_path),
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS asset_versions (
            id TEXT PRIMARY KEY NOT NULL,
            asset_id TEXT NOT NULL,
            version_number INTEGER NOT NULL,
            original_filename TEXT NOT NULL,
            extension TEXT,
            mime_type TEXT,
            byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
            sha256 TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            note TEXT,
            created_at TEXT NOT NULL,
            UNIQUE(asset_id, version_number),
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        "CREATE INDEX IF NOT EXISTS idx_assets_category ON assets(category)",
        "CREATE INDEX IF NOT EXISTS idx_assets_extension ON assets(extension)",
        "CREATE INDEX IF NOT EXISTS idx_assets_created_at ON assets(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_assets_deleted_at ON assets(deleted_at)",
        "CREATE INDEX IF NOT EXISTS idx_asset_tags_tag ON asset_tags(tag)",
        "CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name)",
        "CREATE INDEX IF NOT EXISTS idx_project_assets_asset ON project_assets(asset_id)",
        "CREATE INDEX IF NOT EXISTS idx_asset_versions_asset ON asset_versions(asset_id, version_number)",
        "CREATE INDEX IF NOT EXISTS idx_asset_versions_sha ON asset_versions(sha256)",
        "CREATE INDEX IF NOT EXISTS idx_package_files_asset ON package_files(asset_id, version_number)",
        "CREATE INDEX IF NOT EXISTS idx_package_files_sha ON package_files(sha256)",
        "CREATE INDEX IF NOT EXISTS idx_package_dependencies_asset ON package_dependencies(asset_id, version_number)",
    ];

    for statement in STATEMENTS {
        sqlx::query(statement).execute(pool).await?;
    }

    let columns = sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_table_info('project_assets')",
    )
    .fetch_all(pool)
    .await?;
    if !columns.iter().any(|name| name == "version_number") {
        sqlx::query(
            "ALTER TABLE project_assets ADD COLUMN version_number INTEGER NOT NULL DEFAULT 1",
        )
        .execute(pool)
        .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO asset_versions(
            id, asset_id, version_number, original_filename, extension, mime_type,
            byte_size, sha256, storage_path, note, created_at
        )
        SELECT
            lower(hex(randomblob(16))), a.id, 1, a.original_filename, a.extension, a.mime_type,
            a.byte_size, a.sha256, a.storage_path, 'Imported from pre-Phase-6 asset',
            a.created_at
        FROM assets a
        WHERE NOT EXISTS (
            SELECT 1 FROM asset_versions v WHERE v.asset_id = a.id
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_asset(pool: &SqlitePool, id: &str, include_deleted: bool) -> AppResult<Asset> {
    let row = sqlx::query_as::<_, AssetRow>(
        r#"SELECT * FROM assets WHERE id = ? AND (? OR deleted_at IS NULL)"#,
    )
    .bind(id)
    .bind(include_deleted)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    hydrate(pool, row).await
}

pub async fn get_asset_by_hash(pool: &SqlitePool, hash: &str) -> AppResult<Option<Asset>> {
    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE sha256 = ?")
        .bind(hash)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(row) => Ok(Some(hydrate(pool, row).await?)),
        None => Ok(None),
    }
}

pub async fn list_assets(pool: &SqlitePool, query: &AssetQuery) -> AppResult<Vec<Asset>> {
    let q = query.q.as_ref().map(|v| format!("%{}%", v.trim()));
    let tag = query.tag.as_ref().map(|v| v.trim().to_string());
    let extension = query
        .extension
        .as_ref()
        .map(|v| v.trim().trim_start_matches('.').to_ascii_lowercase());
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows = sqlx::query_as::<_, AssetRow>(
        r#"
        SELECT a.*
        FROM assets a
        WHERE
          (
            (?1 = 1 AND a.deleted_at IS NOT NULL)
            OR
            (?1 = 0 AND (?2 = 1 OR a.deleted_at IS NULL))
          )
          AND (?3 IS NULL OR a.category = ?3 COLLATE NOCASE)
          AND (?4 IS NULL OR a.extension = ?4 COLLATE NOCASE)
          AND (
                ?5 IS NULL
                OR a.name LIKE ?5
                OR a.original_filename LIKE ?5
                OR COALESCE(a.description, '') LIKE ?5
                OR COALESCE(a.creator, '') LIKE ?5
                OR COALESCE(a.license, '') LIKE ?5
                OR COALESCE(a.extension, '') LIKE ?5
              )
          AND (
                ?6 IS NULL
                OR EXISTS (
                    SELECT 1 FROM asset_tags t
                    WHERE t.asset_id = a.id AND t.tag = ?6 COLLATE NOCASE
                )
              )
        ORDER BY a.created_at DESC
        LIMIT ?7 OFFSET ?8
        "#,
    )
    .bind(query.deleted_only)
    .bind(query.include_deleted)
    .bind(query.category.as_deref())
    .bind(extension.as_deref())
    .bind(q.as_deref())
    .bind(tag.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let mut assets = Vec::with_capacity(rows.len());
    for row in rows {
        assets.push(hydrate(pool, row).await?);
    }
    Ok(assets)
}

pub async fn insert_asset(pool: &SqlitePool, row: &AssetRow, tags: &[String]) -> AppResult<Asset> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO assets (
            id, name, original_filename, extension, mime_type, byte_size, sha256,
            storage_path, category, description, source_url, creator, license,
            attribution_required, created_at, updated_at, deleted_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&row.id)
    .bind(&row.name)
    .bind(&row.original_filename)
    .bind(&row.extension)
    .bind(&row.mime_type)
    .bind(row.byte_size)
    .bind(&row.sha256)
    .bind(&row.storage_path)
    .bind(&row.category)
    .bind(&row.description)
    .bind(&row.source_url)
    .bind(&row.creator)
    .bind(&row.license)
    .bind(row.attribution_required)
    .bind(&row.created_at)
    .bind(&row.updated_at)
    .bind(&row.deleted_at)
    .execute(&mut *tx)
    .await?;

    replace_tags_tx(&mut tx, &row.id, tags).await?;
    sqlx::query(
        r#"
        INSERT INTO asset_versions(
            id, asset_id, version_number, original_filename, extension, mime_type,
            byte_size, sha256, storage_path, note, created_at
        ) VALUES(?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&row.id)
    .bind(&row.original_filename)
    .bind(&row.extension)
    .bind(&row.mime_type)
    .bind(row.byte_size)
    .bind(&row.sha256)
    .bind(&row.storage_path)
    .bind("Initial version")
    .bind(&row.created_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get_asset(pool, &row.id, true).await
}

pub async fn update_asset(
    pool: &SqlitePool,
    id: &str,
    req: UpdateAssetRequest,
) -> AppResult<Asset> {
    let current = get_asset(pool, id, true).await?;
    if current.row.deleted_at.is_some() {
        return Err(AppError::Conflict(
            "deleted assets must be restored before editing".to_string(),
        ));
    }

    let name = req.name.unwrap_or(current.row.name);
    if name.trim().is_empty() {
        return Err(AppError::BadRequest("name cannot be empty".to_string()));
    }

    let category = req.category.or(current.row.category);
    let description = req.description.or(current.row.description);
    let source_url = req.source_url.or(current.row.source_url);
    let creator = req.creator.or(current.row.creator);
    let license = req.license.or(current.row.license);
    let attribution_required = req
        .attribution_required
        .unwrap_or(current.row.attribution_required);
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE assets SET
            name = ?, category = ?, description = ?, source_url = ?, creator = ?,
            license = ?, attribution_required = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(name.trim())
    .bind(category.as_deref())
    .bind(description.as_deref())
    .bind(source_url.as_deref())
    .bind(creator.as_deref())
    .bind(license.as_deref())
    .bind(attribution_required)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    if let Some(tags) = req.tags {
        replace_tags_tx(&mut tx, id, &tags).await?;
    }

    tx.commit().await?;
    get_asset(pool, id, true).await
}

pub async fn list_asset_versions(pool: &SqlitePool, asset_id: &str) -> AppResult<Vec<AssetVersion>> {
    get_asset(pool, asset_id, true).await?;
    Ok(sqlx::query_as::<_, AssetVersion>(
        "SELECT * FROM asset_versions WHERE asset_id = ? ORDER BY version_number DESC",
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_asset_version(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
) -> AppResult<AssetVersion> {
    sqlx::query_as::<_, AssetVersion>(
        "SELECT * FROM asset_versions WHERE asset_id = ? AND version_number = ?",
    )
    .bind(asset_id)
    .bind(version_number)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn add_asset_version(
    pool: &SqlitePool,
    asset_id: &str,
    original_filename: String,
    extension: Option<String>,
    mime_type: Option<String>,
    byte_size: i64,
    sha256: String,
    storage_path: String,
    note: Option<String>,
) -> AppResult<Asset> {
    let current = get_asset(pool, asset_id, false).await?;
    if current.row.sha256 == sha256 {
        return Err(AppError::Conflict("new version is identical to the current version".to_string()));
    }

    let next_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM asset_versions WHERE asset_id = ?",
    )
    .bind(asset_id)
    .fetch_one(pool)
    .await?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO asset_versions(
            id, asset_id, version_number, original_filename, extension, mime_type,
            byte_size, sha256, storage_path, note, created_at
        ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(asset_id)
    .bind(next_version)
    .bind(&original_filename)
    .bind(&extension)
    .bind(&mime_type)
    .bind(byte_size)
    .bind(&sha256)
    .bind(&storage_path)
    .bind(note.as_deref())
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE assets SET
            original_filename = ?, extension = ?, mime_type = ?, byte_size = ?,
            sha256 = ?, storage_path = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&original_filename)
    .bind(&extension)
    .bind(&mime_type)
    .bind(byte_size)
    .bind(&sha256)
    .bind(&storage_path)
    .bind(&now)
    .bind(asset_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    get_asset(pool, asset_id, false).await
}

pub async fn add_package_version(
    pool: &SqlitePool,
    asset_id: &str,
    original_filename: String,
    extension: Option<String>,
    mime_type: Option<String>,
    byte_size: i64,
    sha256: String,
    storage_path: String,
    note: Option<String>,
) -> AppResult<Asset> {
    get_asset(pool, asset_id, false).await?;
    let next_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM asset_versions WHERE asset_id = ?",
    )
    .bind(asset_id)
    .fetch_one(pool)
    .await?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO asset_versions(
            id, asset_id, version_number, original_filename, extension, mime_type,
            byte_size, sha256, storage_path, note, created_at
        ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(asset_id)
    .bind(next_version)
    .bind(&original_filename)
    .bind(&extension)
    .bind(&mime_type)
    .bind(byte_size)
    .bind(&sha256)
    .bind(&storage_path)
    .bind(note.as_deref())
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE assets SET
            original_filename = ?, extension = ?, mime_type = ?, byte_size = ?,
            sha256 = ?, storage_path = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&original_filename)
    .bind(&extension)
    .bind(&mime_type)
    .bind(byte_size)
    .bind(&sha256)
    .bind(&storage_path)
    .bind(&now)
    .bind(asset_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    get_asset(pool, asset_id, false).await
}

pub async fn restore_asset_version(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
    note: Option<String>,
) -> AppResult<Asset> {
    let version = get_asset_version(pool, asset_id, version_number).await?;
    let restore_note = note.or_else(|| Some(format!("Restored from version {}", version_number)));
    let asset = add_asset_version(
        pool,
        asset_id,
        version.original_filename,
        version.extension,
        version.mime_type,
        version.byte_size,
        version.sha256,
        version.storage_path,
        restore_note,
    )
    .await?;
    clone_package_files(pool, asset_id, version_number, asset.current_version).await?;
    Ok(asset)
}

pub async fn replace_package_files(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
    files: &[PackageFile],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM package_files WHERE asset_id = ? AND version_number = ?")
        .bind(asset_id)
        .bind(version_number)
        .execute(&mut *tx)
        .await?;

    for file in files {
        sqlx::query(
            r#"
            INSERT INTO package_files(
                id, asset_id, version_number, relative_path, original_filename,
                extension, mime_type, byte_size, sha256, storage_path, is_primary, created_at
            ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&file.id)
        .bind(asset_id)
        .bind(version_number)
        .bind(&file.relative_path)
        .bind(&file.original_filename)
        .bind(&file.extension)
        .bind(&file.mime_type)
        .bind(file.byte_size)
        .bind(&file.sha256)
        .bind(&file.storage_path)
        .bind(file.is_primary)
        .bind(&file.created_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_package_files(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
) -> AppResult<Vec<PackageFile>> {
    get_asset_version(pool, asset_id, version_number).await?;
    Ok(sqlx::query_as::<_, PackageFile>(
        "SELECT * FROM package_files WHERE asset_id = ? AND version_number = ? ORDER BY is_primary DESC, relative_path COLLATE NOCASE",
    )
    .bind(asset_id)
    .bind(version_number)
    .fetch_all(pool)
    .await?)
}

pub async fn get_package_file(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
    file_id: &str,
) -> AppResult<PackageFile> {
    sqlx::query_as::<_, PackageFile>(
        "SELECT * FROM package_files WHERE asset_id = ? AND version_number = ? AND id = ?",
    )
    .bind(asset_id)
    .bind(version_number)
    .bind(file_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn clone_package_files(
    pool: &SqlitePool,
    asset_id: &str,
    source_version: i64,
    target_version: i64,
) -> AppResult<()> {
    let source = list_package_files(pool, asset_id, source_version).await?;
    if source.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let cloned = source
        .into_iter()
        .map(|file| PackageFile {
            id: Uuid::new_v4().to_string(),
            asset_id: asset_id.to_string(),
            version_number: target_version,
            relative_path: file.relative_path,
            original_filename: file.original_filename,
            extension: file.extension,
            mime_type: file.mime_type,
            byte_size: file.byte_size,
            sha256: file.sha256,
            storage_path: file.storage_path,
            is_primary: file.is_primary,
            created_at: now.clone(),
        })
        .collect::<Vec<_>>();
    replace_package_files(pool, asset_id, target_version, &cloned).await?;
    let (referenced, missing) = list_package_dependencies(pool, asset_id, source_version).await?;
    replace_package_dependencies(pool, asset_id, target_version, &referenced, &missing).await
}

pub async fn replace_package_dependencies(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
    referenced: &[String],
    missing: &[String],
) -> AppResult<()> {
    let missing_set = missing.iter().collect::<std::collections::HashSet<_>>();
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM package_dependencies WHERE asset_id = ? AND version_number = ?")
        .bind(asset_id)
        .bind(version_number)
        .execute(&mut *tx)
        .await?;
    for dependency in referenced {
        sqlx::query(
            "INSERT INTO package_dependencies(asset_id, version_number, dependency_path, missing) VALUES(?, ?, ?, ?)",
        )
        .bind(asset_id)
        .bind(version_number)
        .bind(dependency)
        .bind(missing_set.contains(dependency))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_package_dependencies(
    pool: &SqlitePool,
    asset_id: &str,
    version_number: i64,
) -> AppResult<(Vec<String>, Vec<String>)> {
    let rows = sqlx::query_as::<_, (String, bool)>(
        "SELECT dependency_path, missing FROM package_dependencies WHERE asset_id = ? AND version_number = ? ORDER BY dependency_path COLLATE NOCASE",
    )
    .bind(asset_id)
    .bind(version_number)
    .fetch_all(pool)
    .await?;
    let referenced = rows.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>();
    let missing = rows
        .into_iter()
        .filter_map(|(path, is_missing)| is_missing.then_some(path))
        .collect::<Vec<_>>();
    Ok((referenced, missing))
}

pub async fn soft_delete(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE assets SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn restore_asset(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE assets SET deleted_at = NULL, updated_at = ? WHERE id = ? AND deleted_at IS NOT NULL",
    )
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn stats(pool: &SqlitePool) -> AppResult<StatsResponse> {
    let active_assets: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL")
            .fetch_one(pool)
            .await?;
    let deleted_assets: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE deleted_at IS NOT NULL")
            .fetch_one(pool)
            .await?;
    let total_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(byte_size), 0) FROM assets WHERE deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    let unique_tags: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT tag) FROM asset_tags")
        .fetch_one(pool)
        .await?;
    let projects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(pool)
        .await?;

    Ok(StatsResponse {
        active_assets,
        deleted_assets,
        total_bytes,
        unique_tags,
        projects,
    })
}

pub async fn list_projects(pool: &SqlitePool) -> AppResult<Vec<Project>> {
    Ok(sqlx::query_as::<_, Project>(
        "SELECT * FROM projects ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_project(pool: &SqlitePool, id: &str) -> AppResult<Project> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn create_project(
    pool: &SqlitePool,
    req: CreateProjectRequest,
) -> AppResult<Project> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("project name cannot be empty".to_string()));
    }
    if req.local_path.trim().is_empty() {
        return Err(AppError::BadRequest("project local path cannot be empty".to_string()));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let engine = req
        .engine
        .unwrap_or_else(|| "Generic".to_string())
        .trim()
        .to_string();

    sqlx::query(
        r#"
        INSERT INTO projects(id, name, engine, local_path, description, created_at, updated_at)
        VALUES(?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(req.name.trim())
    .bind(if engine.is_empty() { "Generic" } else { &engine })
    .bind(req.local_path.trim())
    .bind(req.description.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get_project(pool, &id).await
}

pub async fn update_project(
    pool: &SqlitePool,
    id: &str,
    req: UpdateProjectRequest,
) -> AppResult<Project> {
    let current = get_project(pool, id).await?;
    let name = req.name.unwrap_or(current.name);
    let engine = req.engine.unwrap_or(current.engine);
    let local_path = req.local_path.unwrap_or(current.local_path);
    let description = req.description.or(current.description);

    if name.trim().is_empty() || local_path.trim().is_empty() {
        return Err(AppError::BadRequest(
            "project name and local path cannot be empty".to_string(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE projects
        SET name = ?, engine = ?, local_path = ?, description = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(name.trim())
    .bind(engine.trim())
    .bind(local_path.trim())
    .bind(description.as_deref())
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;

    get_project(pool, id).await
}

pub async fn delete_project(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn add_project_asset(
    pool: &SqlitePool,
    project_id: &str,
    req: ProjectAssetRequest,
) -> AppResult<ProjectAsset> {
    get_project(pool, project_id).await?;
    let asset = get_asset(pool, &req.asset_id, false).await?;
    let version_number = req.version_number.unwrap_or(asset.current_version);
    get_asset_version(pool, &req.asset_id, version_number).await?;
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO project_assets(project_id, asset_id, relative_path, version_number, added_at)
        VALUES(?, ?, ?, ?, ?)
        ON CONFLICT(project_id, asset_id)
        DO UPDATE SET
            relative_path = excluded.relative_path,
            version_number = excluded.version_number,
            added_at = excluded.added_at
        "#,
    )
    .bind(project_id)
    .bind(&req.asset_id)
    .bind(req.relative_path.as_deref())
    .bind(version_number)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(ProjectAsset {
        project_id: project_id.to_string(),
        asset_id: req.asset_id,
        relative_path: req.relative_path,
        version_number,
        added_at: now,
    })
}

pub async fn list_project_assets(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Vec<Asset>> {
    get_project(pool, project_id).await?;
    let rows = sqlx::query_as::<_, AssetRow>(
        r#"
        SELECT a.*
        FROM assets a
        JOIN project_assets pa ON pa.asset_id = a.id
        WHERE pa.project_id = ? AND a.deleted_at IS NULL
        ORDER BY pa.added_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let mut assets = Vec::with_capacity(rows.len());
    for row in rows {
        assets.push(hydrate(pool, row).await?);
    }
    Ok(assets)
}

pub async fn remove_project_asset(
    pool: &SqlitePool,
    project_id: &str,
    asset_id: &str,
) -> AppResult<()> {
    let result = sqlx::query(
        "DELETE FROM project_assets WHERE project_id = ? AND asset_id = ?",
    )
    .bind(project_id)
    .bind(asset_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

async fn hydrate(pool: &SqlitePool, row: AssetRow) -> AppResult<Asset> {
    let tags = sqlx::query_scalar::<_, String>(
        "SELECT tag FROM asset_tags WHERE asset_id = ? ORDER BY tag COLLATE NOCASE",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await?;
    let current_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 1) FROM asset_versions WHERE asset_id = ?",
    )
    .bind(&row.id)
    .fetch_one(pool)
    .await?;
    Ok(Asset {
        row,
        tags,
        current_version,
    })
}

async fn replace_tags_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    asset_id: &str,
    tags: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM asset_tags WHERE asset_id = ?")
        .bind(asset_id)
        .execute(&mut **tx)
        .await?;

    let mut normalized = tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();

    for tag in normalized {
        sqlx::query("INSERT INTO asset_tags(asset_id, tag) VALUES(?, ?)")
            .bind(asset_id)
            .bind(tag)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
