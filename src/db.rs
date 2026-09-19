use crate::{
    engine,
    error::{AppError, AppResult},
    models::{
        Asset, AssetCheckout, AssetQuery, AssetRelationship, AssetRelationshipEntry,
        AssetVersion, CheckoutRequest, CreateAssetRelationshipRequest,
        CreateProjectRequest, PackageFile, Project, ProjectAsset, ProjectAssetBrowserEntry,
        ProjectAssetCount, ProjectAssetRequest,
        AuditEvent, AuditQuery, CreateUserRequest, SemanticEmbeddingRow, StatsResponse,
        StorageObjectRef, UpdateAssetRelationshipRequest, UpdateAssetRequest, UpdateProjectRequest,
        UpdateUserRequest, UserRole,
        VaultUser,
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
        CREATE TABLE IF NOT EXISTS asset_relationships (
            id TEXT PRIMARY KEY NOT NULL,
            source_asset_id TEXT NOT NULL,
            related_asset_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            label TEXT,
            note TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(source_asset_id, related_asset_id, kind),
            FOREIGN KEY(source_asset_id) REFERENCES assets(id) ON DELETE CASCADE,
            FOREIGN KEY(related_asset_id) REFERENCES assets(id) ON DELETE CASCADE
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
        CREATE TABLE IF NOT EXISTS audit_events (
            id TEXT PRIMARY KEY NOT NULL,
            occurred_at TEXT NOT NULL,
            actor_user_id TEXT,
            actor_username TEXT,
            actor_role TEXT,
            workstation TEXT,
            action TEXT NOT NULL,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            target_type TEXT,
            target_id TEXT,
            result TEXT NOT NULL,
            status_code INTEGER NOT NULL,
            detail TEXT
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS vault_users (
            id TEXT PRIMARY KEY NOT NULL,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            role TEXT NOT NULL CHECK(role IN ('administrator', 'developer', 'read_only')),
            token_hash TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_used_at TEXT
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS asset_storage_tiers (
            asset_id TEXT PRIMARY KEY NOT NULL,
            tier TEXT NOT NULL CHECK(tier IN ('hot', 'archive')),
            transitioned_at TEXT NOT NULL,
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS asset_checkouts (
            asset_id TEXT PRIMARY KEY NOT NULL,
            holder TEXT NOT NULL,
            workstation TEXT NOT NULL,
            note TEXT,
            checked_out_at TEXT NOT NULL,
            FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS semantic_embeddings (
            asset_id TEXT PRIMARY KEY NOT NULL,
            model TEXT NOT NULL,
            document_hash TEXT NOT NULL,
            dimensions INTEGER NOT NULL,
            embedding_json TEXT NOT NULL,
            indexed_at TEXT NOT NULL,
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
        "CREATE INDEX IF NOT EXISTS idx_asset_relationships_source ON asset_relationships(source_asset_id, kind)",
        "CREATE INDEX IF NOT EXISTS idx_asset_relationships_related ON asset_relationships(related_asset_id, kind)",
        "CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name)",
        "CREATE INDEX IF NOT EXISTS idx_project_assets_asset ON project_assets(asset_id)",
        "CREATE INDEX IF NOT EXISTS idx_asset_versions_asset ON asset_versions(asset_id, version_number)",
        "CREATE INDEX IF NOT EXISTS idx_asset_versions_sha ON asset_versions(sha256)",
        "CREATE INDEX IF NOT EXISTS idx_package_files_asset ON package_files(asset_id, version_number)",
        "CREATE INDEX IF NOT EXISTS idx_package_files_sha ON package_files(sha256)",
        "CREATE INDEX IF NOT EXISTS idx_package_dependencies_asset ON package_dependencies(asset_id, version_number)",
        "CREATE INDEX IF NOT EXISTS idx_semantic_embeddings_model ON semantic_embeddings(model)",
        "CREATE INDEX IF NOT EXISTS idx_asset_checkouts_holder ON asset_checkouts(holder, workstation)",
        "CREATE INDEX IF NOT EXISTS idx_vault_users_username ON vault_users(username)",
        "CREATE INDEX IF NOT EXISTS idx_vault_users_token_hash ON vault_users(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_audit_occurred_at ON audit_events(occurred_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_events(actor_username, occurred_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_events(action, occurred_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_audit_target ON audit_events(target_type, target_id, occurred_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_audit_result ON audit_events(result, occurred_at DESC)",
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

pub async fn list_asset_relationships(
    pool: &SqlitePool,
    asset_id: &str,
) -> AppResult<Vec<AssetRelationshipEntry>> {
    get_asset(pool, asset_id, true).await?;

    let rows = sqlx::query_as::<_, AssetRelationship>(
        r#"
        SELECT id, source_asset_id, related_asset_id, kind, label, note, created_at, updated_at
        FROM asset_relationships
        WHERE source_asset_id = ? OR related_asset_id = ?
        ORDER BY kind COLLATE NOCASE, created_at, id
        "#,
    )
    .bind(asset_id)
    .bind(asset_id)
    .fetch_all(pool)
    .await?;

    let mut entries = Vec::with_capacity(rows.len());
    for relationship in rows {
        let outgoing = relationship.source_asset_id == asset_id;
        let other_id = if outgoing {
            &relationship.related_asset_id
        } else {
            &relationship.source_asset_id
        };
        let asset = get_asset(pool, other_id, true).await?;
        entries.push(AssetRelationshipEntry {
            relationship,
            direction: if outgoing { "outgoing" } else { "incoming" }.to_string(),
            asset,
        });
    }
    Ok(entries)
}

pub async fn create_asset_relationship(
    pool: &SqlitePool,
    source_asset_id: &str,
    request: CreateAssetRelationshipRequest,
) -> AppResult<AssetRelationship> {
    get_asset(pool, source_asset_id, true).await?;
    get_asset(pool, &request.related_asset_id, true).await?;

    if source_asset_id == request.related_asset_id {
        return Err(AppError::BadRequest(
            "an asset cannot be related to itself".to_string(),
        ));
    }

    let kind = request.kind.as_str();
    let duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_relationships WHERE source_asset_id = ? AND related_asset_id = ? AND kind = ?",
    )
    .bind(source_asset_id)
    .bind(&request.related_asset_id)
    .bind(kind)
    .fetch_one(pool)
    .await?;
    if duplicate > 0 {
        return Err(AppError::Conflict(
            "this asset relationship already exists".to_string(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let label = request.label.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    let note = request.note.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());

    sqlx::query(
        r#"
        INSERT INTO asset_relationships(
            id, source_asset_id, related_asset_id, kind, label, note, created_at, updated_at
        ) VALUES(?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(source_asset_id)
    .bind(&request.related_asset_id)
    .bind(kind)
    .bind(label)
    .bind(note)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get_asset_relationship(pool, &id).await
}

pub async fn get_asset_relationship(
    pool: &SqlitePool,
    relationship_id: &str,
) -> AppResult<AssetRelationship> {
    sqlx::query_as::<_, AssetRelationship>(
        r#"
        SELECT id, source_asset_id, related_asset_id, kind, label, note, created_at, updated_at
        FROM asset_relationships
        WHERE id = ?
        "#,
    )
    .bind(relationship_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn update_asset_relationship(
    pool: &SqlitePool,
    asset_id: &str,
    relationship_id: &str,
    request: UpdateAssetRelationshipRequest,
) -> AppResult<AssetRelationship> {
    let current = get_asset_relationship(pool, relationship_id).await?;
    if current.source_asset_id != asset_id && current.related_asset_id != asset_id {
        return Err(AppError::NotFound);
    }

    let kind = request
        .kind
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| current.kind.clone());
    let label = request
        .label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or(current.label.clone());
    let note = request
        .note
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or(current.note.clone());
    let now = chrono::Utc::now().to_rfc3339();

    let duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM asset_relationships WHERE source_asset_id = ? AND related_asset_id = ? AND kind = ? AND id <> ?",
    )
    .bind(&current.source_asset_id)
    .bind(&current.related_asset_id)
    .bind(&kind)
    .bind(relationship_id)
    .fetch_one(pool)
    .await?;
    if duplicate > 0 {
        return Err(AppError::Conflict(
            "this asset relationship already exists".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE asset_relationships SET kind = ?, label = ?, note = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&kind)
    .bind(label)
    .bind(note)
    .bind(&now)
    .bind(relationship_id)
    .execute(pool)
    .await?;

    get_asset_relationship(pool, relationship_id).await
}

pub async fn delete_asset_relationship(
    pool: &SqlitePool,
    asset_id: &str,
    relationship_id: &str,
) -> AppResult<()> {
    let current = get_asset_relationship(pool, relationship_id).await?;
    if current.source_asset_id != asset_id && current.related_asset_id != asset_id {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM asset_relationships WHERE id = ?")
        .bind(relationship_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_audit_event(
    pool: &SqlitePool,
    event: &AuditEvent,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO audit_events(
            id, occurred_at, actor_user_id, actor_username, actor_role, workstation,
            action, method, path, target_type, target_id, result, status_code, detail
        ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&event.id)
    .bind(&event.occurred_at)
    .bind(&event.actor_user_id)
    .bind(&event.actor_username)
    .bind(&event.actor_role)
    .bind(&event.workstation)
    .bind(&event.action)
    .bind(&event.method)
    .bind(&event.path)
    .bind(&event.target_type)
    .bind(&event.target_id)
    .bind(&event.result)
    .bind(event.status_code)
    .bind(&event.detail)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_audit_events(
    pool: &SqlitePool,
    query: &AuditQuery,
) -> AppResult<Vec<AuditEvent>> {
    let username = query.username.as_ref().map(|v| v.trim().to_string());
    let action = query.action.as_ref().map(|v| format!("%{}%", v.trim()));
    let result = query.result.as_ref().map(|v| v.trim().to_string());
    let target_type = query.target_type.as_ref().map(|v| v.trim().to_string());
    let target_id = query.target_id.as_ref().map(|v| v.trim().to_string());
    let from = query.from.as_ref().map(|v| v.trim().to_string());
    let to = query.to.as_ref().map(|v| v.trim().to_string());
    let limit = query.limit.unwrap_or(250).clamp(1, 2000);
    let offset = query.offset.unwrap_or(0).max(0);

    Ok(sqlx::query_as::<_, AuditEvent>(
        r#"
        SELECT *
        FROM audit_events
        WHERE (? IS NULL OR actor_username = ? COLLATE NOCASE)
          AND (? IS NULL OR action LIKE ?)
          AND (? IS NULL OR result = ? COLLATE NOCASE)
          AND (? IS NULL OR target_type = ? COLLATE NOCASE)
          AND (? IS NULL OR target_id = ?)
          AND (? IS NULL OR occurred_at >= ?)
          AND (? IS NULL OR occurred_at <= ?)
        ORDER BY occurred_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(username.as_deref()).bind(username.as_deref())
    .bind(action.as_deref()).bind(action.as_deref())
    .bind(result.as_deref()).bind(result.as_deref())
    .bind(target_type.as_deref()).bind(target_type.as_deref())
    .bind(target_id.as_deref()).bind(target_id.as_deref())
    .bind(from.as_deref()).bind(from.as_deref())
    .bind(to.as_deref()).bind(to.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn list_users(pool: &SqlitePool) -> AppResult<Vec<VaultUser>> {
    Ok(sqlx::query_as::<_, VaultUser>(
        "SELECT id, username, role, enabled, created_at, updated_at, last_used_at FROM vault_users ORDER BY username COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_user_by_token_hash(
    pool: &SqlitePool,
    token_hash: &str,
) -> AppResult<Option<VaultUser>> {
    Ok(sqlx::query_as::<_, VaultUser>(
        "SELECT id, username, role, enabled, created_at, updated_at, last_used_at FROM vault_users WHERE token_hash = ?",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?)
}

pub async fn count_enabled_administrators(pool: &SqlitePool) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM vault_users WHERE role = 'administrator' AND enabled = 1",
    )
    .fetch_one(pool)
    .await?)
}

pub async fn count_users(pool: &SqlitePool) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM vault_users")
        .fetch_one(pool)
        .await?)
}

pub async fn create_user(
    pool: &SqlitePool,
    request: CreateUserRequest,
    token_hash: String,
) -> AppResult<VaultUser> {
    let username = request.username.trim();
    if username.is_empty() {
        return Err(AppError::BadRequest("username cannot be empty".to_string()));
    }
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO vault_users(id, username, role, token_hash, enabled, created_at, updated_at) VALUES(?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&id)
    .bind(username)
    .bind(request.role.as_str())
    .bind(token_hash)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    get_user(pool, &id).await
}

pub async fn get_user(pool: &SqlitePool, id: &str) -> AppResult<VaultUser> {
    sqlx::query_as::<_, VaultUser>(
        "SELECT id, username, role, enabled, created_at, updated_at, last_used_at FROM vault_users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn update_user(
    pool: &SqlitePool,
    id: &str,
    request: UpdateUserRequest,
    token_hash: Option<String>,
) -> AppResult<VaultUser> {
    let current = get_user(pool, id).await?;
    let role = request.role
        .map(|role| role.as_str().to_string())
        .unwrap_or(current.role.clone());
    let enabled = request.enabled.unwrap_or(current.enabled);
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(token_hash) = token_hash {
        sqlx::query("UPDATE vault_users SET role = ?, enabled = ?, token_hash = ?, updated_at = ? WHERE id = ?")
            .bind(&role)
            .bind(enabled)
            .bind(token_hash)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("UPDATE vault_users SET role = ?, enabled = ?, updated_at = ? WHERE id = ?")
            .bind(&role)
            .bind(enabled)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
    }
    get_user(pool, id).await
}

pub async fn delete_user(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM vault_users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn touch_user(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("UPDATE vault_users SET last_used_at = ? WHERE id = ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn ensure_bootstrap_admin(
    pool: &SqlitePool,
    username: &str,
    token_hash: &str,
) -> AppResult<Option<VaultUser>> {
    if count_users(pool).await? > 0 {
        return Ok(None);
    }
    let request = CreateUserRequest {
        username: username.to_string(),
        role: UserRole::Administrator,
        token: String::new(),
    };
    Ok(Some(create_user(pool, request, token_hash.to_string()).await?))
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

pub async fn list_asset_storage_objects(
    pool: &SqlitePool,
    asset_id: &str,
) -> AppResult<Vec<StorageObjectRef>> {
    get_asset(pool, asset_id, true).await?;
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT storage_path, sha256 FROM assets WHERE id = ?
        UNION
        SELECT storage_path, sha256 FROM asset_versions WHERE asset_id = ?
        UNION
        SELECT storage_path, sha256 FROM package_files WHERE asset_id = ?
        "#,
    )
    .bind(asset_id)
    .bind(asset_id)
    .bind(asset_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(storage_path, sha256)| StorageObjectRef { storage_path, sha256 })
        .collect())
}

pub async fn replace_asset_storage_paths(
    pool: &SqlitePool,
    asset_id: &str,
    mappings: &[(String, String)],
    tier: &str,
) -> AppResult<String> {
    if tier != "hot" && tier != "archive" {
        return Err(AppError::BadRequest("invalid storage tier".to_string()));
    }
    get_asset(pool, asset_id, true).await?;
    let transitioned_at = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    for (old, new) in mappings {
        sqlx::query("UPDATE assets SET storage_path = ? WHERE id = ? AND storage_path = ?")
            .bind(new)
            .bind(asset_id)
            .bind(old)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE asset_versions SET storage_path = ? WHERE asset_id = ? AND storage_path = ?")
            .bind(new)
            .bind(asset_id)
            .bind(old)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE package_files SET storage_path = ? WHERE asset_id = ? AND storage_path = ?")
            .bind(new)
            .bind(asset_id)
            .bind(old)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO asset_storage_tiers(asset_id, tier, transitioned_at)
        VALUES(?, ?, ?)
        ON CONFLICT(asset_id)
        DO UPDATE SET tier = excluded.tier, transitioned_at = excluded.transitioned_at
        "#,
    )
    .bind(asset_id)
    .bind(tier)
    .bind(&transitioned_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(transitioned_at)
}

pub async fn get_asset_storage_tier(
    pool: &SqlitePool,
    asset_id: &str,
) -> AppResult<(String, Option<String>)> {
    get_asset(pool, asset_id, true).await?;
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT tier, transitioned_at FROM asset_storage_tiers WHERE asset_id = ?",
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;
    if let Some((tier, transitioned_at)) = row {
        return Ok((tier, Some(transitioned_at)));
    }

    let current: String = sqlx::query_scalar("SELECT storage_path FROM assets WHERE id = ?")
        .bind(asset_id)
        .fetch_one(pool)
        .await?;
    Ok((
        if current.starts_with("archive://") { "archive" } else { "hot" }.to_string(),
        None,
    ))
}

pub async fn count_storage_path_references(
    pool: &SqlitePool,
    storage_path: &str,
) -> AppResult<i64> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT
            (SELECT COUNT(*) FROM assets WHERE storage_path = ?) +
            (SELECT COUNT(*) FROM asset_versions WHERE storage_path = ?) +
            (SELECT COUNT(*) FROM package_files WHERE storage_path = ?)
        "#,
    )
    .bind(storage_path)
    .bind(storage_path)
    .bind(storage_path)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn get_asset_checkout(
    pool: &SqlitePool,
    asset_id: &str,
) -> AppResult<Option<AssetCheckout>> {
    get_asset(pool, asset_id, true).await?;
    Ok(sqlx::query_as::<_, AssetCheckout>(
        "SELECT asset_id, holder, workstation, note, checked_out_at FROM asset_checkouts WHERE asset_id = ?",
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_asset_checkouts(pool: &SqlitePool) -> AppResult<Vec<AssetCheckout>> {
    Ok(sqlx::query_as::<_, AssetCheckout>(
        "SELECT asset_id, holder, workstation, note, checked_out_at FROM asset_checkouts ORDER BY checked_out_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn checkout_asset(
    pool: &SqlitePool,
    asset_id: &str,
    request: CheckoutRequest,
) -> AppResult<AssetCheckout> {
    get_asset(pool, asset_id, false).await?;
    let holder = request.holder.trim();
    let workstation = request.workstation.trim();
    if holder.is_empty() || workstation.is_empty() {
        return Err(AppError::BadRequest(
            "checkout holder and workstation are required".to_string(),
        ));
    }

    if let Some(existing) = get_asset_checkout(pool, asset_id).await? {
        if existing.holder == holder && existing.workstation == workstation {
            return Ok(existing);
        }
        return Err(AppError::Conflict(format!(
            "asset is already checked out by {}@{}",
            existing.holder, existing.workstation
        )));
    }

    let checked_out_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO asset_checkouts(asset_id, holder, workstation, note, checked_out_at) VALUES(?, ?, ?, ?, ?)",
    )
    .bind(asset_id)
    .bind(holder)
    .bind(workstation)
    .bind(request.note.as_deref())
    .bind(&checked_out_at)
    .execute(pool)
    .await?;

    Ok(AssetCheckout {
        asset_id: asset_id.to_string(),
        holder: holder.to_string(),
        workstation: workstation.to_string(),
        note: request.note,
        checked_out_at,
    })
}

pub async fn clear_asset_checkout(pool: &SqlitePool, asset_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM asset_checkouts WHERE asset_id = ?")
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn release_asset_checkout(
    pool: &SqlitePool,
    asset_id: &str,
    holder: &str,
    workstation: &str,
) -> AppResult<()> {
    if let Some(existing) = get_asset_checkout(pool, asset_id).await? {
        if existing.holder != holder || existing.workstation != workstation {
            return Err(AppError::Conflict(format!(
                "asset is checked out by {}@{}",
                existing.holder, existing.workstation
            )));
        }
    } else {
        return Err(AppError::NotFound);
    }

    sqlx::query("DELETE FROM asset_checkouts WHERE asset_id = ?")
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn upsert_semantic_embedding(
    pool: &SqlitePool,
    asset_id: &str,
    model: &str,
    document_hash: &str,
    embedding: &[f32],
) -> AppResult<()> {
    let json = serde_json::to_string(embedding)
        .map_err(|err| AppError::Other(anyhow::anyhow!("embedding serialization failed: {err}")))?;
    sqlx::query(
        r#"
        INSERT INTO semantic_embeddings(
            asset_id, model, document_hash, dimensions, embedding_json, indexed_at
        ) VALUES(?, ?, ?, ?, ?, ?)
        ON CONFLICT(asset_id) DO UPDATE SET
            model = excluded.model,
            document_hash = excluded.document_hash,
            dimensions = excluded.dimensions,
            embedding_json = excluded.embedding_json,
            indexed_at = excluded.indexed_at
        "#,
    )
    .bind(asset_id)
    .bind(model)
    .bind(document_hash)
    .bind(embedding.len() as i64)
    .bind(json)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_semantic_embedding(pool: &SqlitePool, asset_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM semantic_embeddings WHERE asset_id = ?")
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_semantic_embedding(
    pool: &SqlitePool,
    asset_id: &str,
) -> AppResult<Option<SemanticEmbeddingRow>> {
    Ok(sqlx::query_as::<_, SemanticEmbeddingRow>(
        "SELECT asset_id, model, document_hash, dimensions, embedding_json, indexed_at FROM semantic_embeddings WHERE asset_id = ?",
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_semantic_embeddings(
    pool: &SqlitePool,
    model: &str,
) -> AppResult<Vec<SemanticEmbeddingRow>> {
    Ok(sqlx::query_as::<_, SemanticEmbeddingRow>(
        "SELECT asset_id, model, document_hash, dimensions, embedding_json, indexed_at FROM semantic_embeddings WHERE model = ?",
    )
    .bind(model)
    .fetch_all(pool)
    .await?)
}

pub async fn semantic_index_counts(
    pool: &SqlitePool,
    model: &str,
) -> AppResult<(i64, i64)> {
    let indexed: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM semantic_embeddings se
        JOIN assets a ON a.id = se.asset_id
        WHERE se.model = ? AND a.deleted_at IS NULL
        "#,
    )
    .bind(model)
    .fetch_one(pool)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok((indexed, total))
}

pub async fn list_all_active_assets(pool: &SqlitePool) -> AppResult<Vec<Asset>> {
    let rows = sqlx::query_as::<_, AssetRow>(
        "SELECT * FROM assets WHERE deleted_at IS NULL ORDER BY created_at DESC",
    )
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
    let source_package_files = list_package_files(pool, asset_id, version_number).await?;
    let asset = if source_package_files.is_empty() {
        add_asset_version(
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
        .await?
    } else {
        add_package_version(
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
        .await?
    };
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
    let requested_engine = req.engine.unwrap_or_else(|| "Generic".to_string());
    let engine = engine::require(if requested_engine.trim().is_empty() {
        "Generic"
    } else {
        requested_engine.trim()
    })?
    .id;

    sqlx::query(
        r#"
        INSERT INTO projects(id, name, engine, local_path, description, created_at, updated_at)
        VALUES(?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(req.name.trim())
    .bind(engine)
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
    let requested_engine = req.engine.unwrap_or(current.engine);
    let engine = engine::require(requested_engine.trim())?.id.to_string();
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

pub async fn list_project_links(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Vec<ProjectAsset>> {
    get_project(pool, project_id).await?;
    Ok(sqlx::query_as::<_, ProjectAsset>(
        "SELECT project_id, asset_id, relative_path, version_number, added_at FROM project_assets WHERE project_id = ? ORDER BY added_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?)
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

pub async fn list_project_asset_counts(
    pool: &SqlitePool,
) -> AppResult<Vec<ProjectAssetCount>> {
    Ok(sqlx::query_as::<_, ProjectAssetCount>(
        r#"
        SELECT p.id AS project_id, COUNT(a.id) AS asset_count
        FROM projects p
        LEFT JOIN project_assets pa ON pa.project_id = p.id
        LEFT JOIN assets a ON a.id = pa.asset_id AND a.deleted_at IS NULL
        GROUP BY p.id
        ORDER BY p.name COLLATE NOCASE
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_project_asset_browser(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Vec<ProjectAssetBrowserEntry>> {
    let links = list_project_links(pool, project_id).await?;
    let mut entries = Vec::with_capacity(links.len());

    for link in links {
        let asset = get_asset(pool, &link.asset_id, false).await?;
        entries.push(ProjectAssetBrowserEntry {
            outdated: link.version_number < asset.current_version,
            pinned_version: link.version_number,
            relative_path: link.relative_path.clone(),
            added_at: link.added_at.clone(),
            asset,
        });
    }

    Ok(entries)
}

pub async fn get_project_asset_link(
    pool: &SqlitePool,
    project_id: &str,
    asset_id: &str,
) -> AppResult<ProjectAsset> {
    get_project(pool, project_id).await?;
    sqlx::query_as::<_, ProjectAsset>(
        "SELECT project_id, asset_id, relative_path, version_number, added_at FROM project_assets WHERE project_id = ? AND asset_id = ?",
    )
    .bind(project_id)
    .bind(asset_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
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
