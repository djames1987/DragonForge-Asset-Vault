use crate::{
    auth,
    backup,
    db,
    engine,
    package,
    licensing,
    semantic,
    tiering,
    error::{AppError, AppResult},
    models::{
        AssetQuery, AssetRow, AuditExportResponse, AuditQuery, AuthMeResponse, BackupCreateResponse, BackupStatusResponse,
        BackupVerifyResponse, CheckoutRequest, CheckoutStatusResponse, CreateProjectRequest,
        CreateUserRequest, DeleteResponse, EnginePreset, HealthResponse, ProjectExportPlan,
        PackageImportResponse, PackageManifest, PackageVersionResponse, ProjectAssetRequest,
        ProjectLicenseEntry, ProjectLicenseReport, RestoreResponse, RestoreVersionRequest,
        SemanticSearchQuery, SemanticSearchResponse, SemanticSearchResult, StorageTierMoveResponse,
        StorageTierStatusResponse, UpdateAssetRequest, UpdateProjectRequest, UpdateUserRequest,
        UploadMetadata, UploadResponse, VaultUser,
    },
    storage::{self, IncomingFile, Storage},
    thumbnail,
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Extension, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    middleware,
    routing::get,
    Json, Router,
};
use sqlx::SqlitePool;
use std::{collections::HashMap, sync::Arc};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub storage: Storage,
    pub semantic: crate::config::SemanticConfig,
    pub backup: crate::config::BackupConfig,
    pub auth: crate::config::AuthConfig,
    pub database_filename: String,
}

pub fn router(state: AppState, max_upload_bytes: usize) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/me", get(auth_me))
        .route("/api/audit", get(list_audit))
        .route("/api/audit/export", get(export_audit))
        .route("/api/users", get(list_users).post(create_user))
        .route(
            "/api/users/:id",
            get(get_user).patch(update_user).delete(delete_user),
        )
        .route("/api/stats", get(stats))
        .route("/api/backups", get(backup_status).post(create_backup))
        .route("/api/backups/:id/verify", axum::routing::post(verify_backup))
        .route("/api/engines/presets", get(engine_presets))
        .route("/api/projects/:id/export-plan", get(project_export_plan))
        .route("/api/licenses/presets", get(license_presets))
        .route("/api/search/semantic", get(semantic_search))
        .route("/api/search/semantic/status", get(semantic_status))
        .route("/api/search/semantic/reindex", axum::routing::post(reindex_all_semantic))
        .route("/api/assets", get(list_assets).post(upload_asset))
        .route(
            "/api/assets/:id",
            get(get_asset).patch(update_asset).delete(delete_asset),
        )
        .route("/api/assets/:id/download", get(download_asset))
        .route("/api/assets/:id/license-status", get(asset_license_status))
        .route("/api/assets/:id/storage-tier", get(asset_storage_tier))
        .route("/api/assets/:id/archive", axum::routing::post(archive_asset_storage))
        .route("/api/assets/:id/recall", axum::routing::post(recall_asset_storage))
        .route("/api/checkouts", get(list_checkouts))
        .route(
            "/api/assets/:id/checkout",
            get(get_checkout).post(checkout_asset).delete(checkin_asset),
        )
        .route("/api/assets/:id/semantic-index", axum::routing::post(reindex_asset_semantic))
        .route("/api/assets/:id/thumbnail", get(asset_thumbnail))
        .route("/api/assets/:id/preview", get(asset_preview))
        .route("/api/assets/:id/restore", axum::routing::post(restore_asset))
        .route("/api/packages", axum::routing::post(import_package))
        .route("/api/assets/:id/package", get(get_current_package))
        .route("/api/assets/:id/package-versions", axum::routing::post(upload_package_version))
        .route("/api/assets/:id/versions/:version/package", get(get_package_version_manifest))
        .route(
            "/api/assets/:id/versions/:version/package/files/:file_id/download",
            get(download_package_file),
        )
        .route(
            "/api/assets/:id/versions",
            get(list_asset_versions).post(upload_asset_version),
        )
        .route(
            "/api/assets/:id/versions/:version/download",
            get(download_asset_version),
        )
        .route(
            "/api/assets/:id/versions/:version/restore",
            axum::routing::post(restore_asset_version),
        )
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).patch(update_project).delete(delete_project),
        )
        .route(
            "/api/projects/:id/assets",
            get(list_project_assets).post(add_project_asset),
        )
        .route("/api/projects/:id/license-report", get(project_license_report))
        .route(
            "/api/projects/:id/assets/:asset_id",
            get(get_project_asset_link).delete(remove_project_asset),
        )
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(
            Arc::new(state.clone()),
            auth::middleware,
        ))
        .with_state(Arc::new(state))
}

async fn health(
    State(state): State<Arc<AppState>>,
) -> Json<HealthResponse> {
    info!("health check");
    Json(HealthResponse {
        ok: true,
        service: "dragonforge-asset-vault",
        phase: 15,
        version: env!("CARGO_PKG_VERSION"),
        auth_enabled: state.auth.enabled,
    })
}

async fn auth_me(
    Extension(context): Extension<auth::AuthContext>,
) -> Json<AuthMeResponse> {
    Json(context.me_response())
}

async fn list_audit(
    State(state): State<Arc<AppState>>,
    Extension(context): Extension<auth::AuthContext>,
    Query(mut query): Query<AuditQuery>,
) -> AppResult<Json<Vec<crate::models::AuditEvent>>> {
    if !context.role.is_admin() {
        query.username = context.username().map(ToString::to_string);
    }
    Ok(Json(db::list_audit_events(&state.db, &query).await?))
}

async fn export_audit(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuditQuery>,
) -> AppResult<Json<AuditExportResponse>> {
    let events = db::list_audit_events(&state.db, &query).await?;
    let mut csv = String::from(
        "occurred_at,username,role,workstation,action,method,path,target_type,target_id,result,status_code,detail\n",
    );
    for event in &events {
        let status_code = event.status_code.to_string();
        let fields = [
            event.occurred_at.as_str(),
            event.actor_username.as_deref().unwrap_or(""),
            event.actor_role.as_deref().unwrap_or(""),
            event.workstation.as_deref().unwrap_or(""),
            event.action.as_str(),
            event.method.as_str(),
            event.path.as_str(),
            event.target_type.as_deref().unwrap_or(""),
            event.target_id.as_deref().unwrap_or(""),
            event.result.as_str(),
            status_code.as_str(),
            event.detail.as_deref().unwrap_or(""),
        ];
        csv.push_str(
            &fields
                .into_iter()
                .map(csv_cell)
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }
    Ok(Json(AuditExportResponse {
        generated_at: chrono::Utc::now().to_rfc3339(),
        events,
        csv,
    }))
}

fn csv_cell(value: &str) -> String {
    if value.chars().any(|ch| matches!(ch, ',' | '"' | '\n' | '\r')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

async fn list_users(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<VaultUser>>> {
    Ok(Json(db::list_users(&state.db).await?))
}

async fn get_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<VaultUser>> {
    Ok(Json(db::get_user(&state.db, &id).await?))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<VaultUser>)> {
    let token_hash = auth::hash_token(&request.token)?;
    let user = db::create_user(&state.db, request, token_hash).await?;
    info!(username = %user.username, role = %user.role, "vault user created");
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> AppResult<Json<VaultUser>> {
    let current = db::get_user(&state.db, &id).await?;
    if current.role == "administrator" {
        let removes_admin = request.enabled == Some(false)
            || request.role.as_ref().is_some_and(|role| !role.is_admin());
        if removes_admin && db::count_enabled_administrators(&state.db).await? <= 1 {
            return Err(AppError::Conflict(
                "cannot disable or demote the last enabled Administrator".to_string(),
            ));
        }
    }
    let token_hash = match request.token.as_deref() {
        Some(token) => Some(auth::hash_token(token)?),
        None => None,
    };
    let user = db::update_user(&state.db, &id, request, token_hash).await?;
    info!(username = %user.username, role = %user.role, enabled = user.enabled, "vault user updated");
    Ok(Json(user))
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let current = db::get_user(&state.db, &id).await?;
    if current.role == "administrator"
        && current.enabled
        && db::count_enabled_administrators(&state.db).await? <= 1
    {
        return Err(AppError::Conflict(
            "cannot delete the last enabled Administrator".to_string(),
        ));
    }
    db::delete_user(&state.db, &id).await?;
    info!(username = %current.username, "vault user deleted");
    Ok(StatusCode::NO_CONTENT)
}

fn request_identity(headers: &HeaderMap) -> AppResult<(String, String)> {
    let holder = headers
        .get("x-dragonforge-user")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_string();
    let workstation = headers
        .get("x-dragonforge-workstation")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_string();

    if holder.is_empty() || workstation.is_empty() {
        return Err(AppError::BadRequest(
            "DragonForge user/workstation identity headers are required".to_string(),
        ));
    }
    Ok((holder, workstation))
}

async fn ensure_asset_mutation_allowed(
    state: &AppState,
    headers: &HeaderMap,
    asset_id: &str,
) -> AppResult<()> {
    let Some(checkout) = db::get_asset_checkout(&state.db, asset_id).await? else {
        return Ok(());
    };
    let (holder, workstation) = request_identity(headers)?;
    if checkout.holder == holder && checkout.workstation == workstation {
        Ok(())
    } else {
        Err(AppError::Conflict(format!(
            "asset is checked out by {}@{}",
            checkout.holder, checkout.workstation
        )))
    }
}

async fn asset_storage_tier(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<StorageTierStatusResponse>> {
    Ok(Json(tiering::status(&state.db, &state.storage, &id).await?))
}

async fn archive_asset_storage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<StorageTierMoveResponse>> {
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    info!(asset_id = %id, "asset archive requested");
    Ok(Json(tiering::archive_asset(&state.db, &state.storage, &id).await?))
}

async fn recall_asset_storage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<StorageTierMoveResponse>> {
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    info!(asset_id = %id, "asset recall requested");
    Ok(Json(tiering::recall_asset(&state.db, &state.storage, &id).await?))
}

async fn list_checkouts(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    Ok(Json(db::list_asset_checkouts(&state.db).await?))
}

async fn get_checkout(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let checkout = db::get_asset_checkout(&state.db, &id).await?;
    Ok(Json(CheckoutStatusResponse {
        asset_id: id,
        checkout,
    }))
}

async fn checkout_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(mut request): Json<CheckoutRequest>,
) -> AppResult<impl IntoResponse> {
    let (holder, workstation) = request_identity(&headers)?;
    request.holder = holder;
    request.workstation = workstation;
    let checkout = db::checkout_asset(&state.db, &id, request).await?;
    info!(
        asset_id = %id,
        holder = %checkout.holder,
        workstation = %checkout.workstation,
        "asset checked out"
    );
    Ok((StatusCode::CREATED, Json(checkout)))
}

async fn checkin_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let (holder, workstation) = request_identity(&headers)?;
    db::release_asset_checkout(&state.db, &id, &holder, &workstation).await?;
    info!(
        asset_id = %id,
        holder = %holder,
        workstation = %workstation,
        "asset checked in"
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn refresh_semantic_index_nonfatal(state: &AppState, asset: &crate::models::Asset) {
    if !state.semantic.enabled {
        return;
    }

    let document = semantic::asset_document(asset);
    let hash = semantic::document_hash(&document);
    match semantic::embed_text(&state.semantic, &document).await {
        Ok(embedding) => {
            if let Err(err) = db::upsert_semantic_embedding(
                &state.db,
                &asset.row.id,
                &state.semantic.model,
                &hash,
                &embedding,
            )
            .await
            {
                warn!(
                    asset_id = %asset.row.id,
                    error = %err,
                    "asset saved but semantic index update failed"
                );
            } else {
                info!(
                    asset_id = %asset.row.id,
                    model = %state.semantic.model,
                    dimensions = embedding.len(),
                    "asset semantic index refreshed"
                );
            }
        }
        Err(err) => {
            if let Err(delete_err) = db::delete_semantic_embedding(&state.db, &asset.row.id).await {
                warn!(
                    asset_id = %asset.row.id,
                    error = %delete_err,
                    "failed to clear stale semantic index"
                );
            }
            warn!(
                asset_id = %asset.row.id,
                error = %err,
                "asset saved without semantic index; keyword search remains available"
            );
        }
    }
}

async fn semantic_status(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    let assets = db::list_all_active_assets(&state.db).await?;
    let mut indexed = 0i64;
    let mut stale = 0i64;
    for asset in &assets {
        if let Some(row) = db::get_semantic_embedding(&state.db, &asset.row.id).await? {
            let document = semantic::asset_document(asset);
            if row.model == state.semantic.model
                && row.document_hash == semantic::document_hash(&document)
            {
                indexed += 1;
            } else {
                stale += 1;
            }
        }
    }
    let reachable = semantic::ollama_reachable(&state.semantic).await;
    let last_error = if !state.semantic.enabled {
        Some("Semantic search is disabled in DragonForge.toml".to_string())
    } else if !reachable {
        Some(format!(
            "Ollama is not reachable at {} or the service is unavailable",
            state.semantic.ollama_url
        ))
    } else {
        None
    };
    Ok(Json(semantic::status(
        &state.semantic,
        reachable,
        indexed,
        assets.len() as i64,
        stale,
        last_error,
    )))
}

async fn semantic_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SemanticSearchQuery>,
) -> AppResult<impl IntoResponse> {
    let q = query.q.trim();
    if q.is_empty() {
        return Err(AppError::BadRequest("semantic search query cannot be empty".to_string()));
    }

    let limit = query
        .limit
        .unwrap_or(50)
        .clamp(1, state.semantic.max_results.max(1));
    let assets = db::list_all_active_assets(&state.db).await?;
    let embeddings = db::list_semantic_embeddings(&state.db, &state.semantic.model).await?;
    let embedding_map = embeddings
        .into_iter()
        .map(|row| (row.asset_id.clone(), row))
        .collect::<HashMap<_, _>>();

    let query_embedding = match semantic::embed_text(&state.semantic, q).await {
        Ok(value) => value,
        Err(err) => {
            warn!(error = %err, query = %q, "semantic search fell back to keyword ranking");
            let results = semantic::fallback_results(q, assets, limit);
            return Ok(Json(SemanticSearchResponse {
                query: q.to_string(),
                mode: "keyword_fallback".to_string(),
                model: state.semantic.model.clone(),
                indexed_assets: embedding_map.len() as i64,
                results,
            }));
        }
    };

    let mut results = Vec::new();
    let mut valid_indexed = 0i64;
    for asset in assets {
        let keyword = semantic::keyword_score(&asset, q);
        let mut semantic_score = 0.0f32;

        if let Some(row) = embedding_map.get(&asset.row.id) {
            let document = semantic::asset_document(&asset);
            if row.document_hash == semantic::document_hash(&document) {
                if let Ok(vector) = serde_json::from_str::<Vec<f32>>(&row.embedding_json) {
                    if vector.len() == query_embedding.len() {
                        semantic_score = semantic::cosine_similarity(&query_embedding, &vector);
                        valid_indexed += 1;
                    }
                }
            }
        }

        if semantic_score != 0.0 || keyword > 0.0 {
            results.push(SemanticSearchResult {
                asset,
                semantic_score,
                keyword_score: keyword,
                combined_score: 0.0,
            });
        }
    }

    let results = semantic::combine_results(&state.semantic, results, limit);
    info!(
        query = %q,
        results = results.len(),
        indexed_assets = valid_indexed,
        model = %state.semantic.model,
        "semantic search completed"
    );
    Ok(Json(SemanticSearchResponse {
        query: q.to_string(),
        mode: "hybrid".to_string(),
        model: state.semantic.model.clone(),
        indexed_assets: valid_indexed,
        results,
    }))
}

async fn reindex_all_semantic(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    if !state.semantic.enabled {
        return Err(AppError::BadRequest("semantic search is disabled".to_string()));
    }

    let assets = db::list_all_active_assets(&state.db).await?;
    let requested = assets.len();
    let mut indexed = 0usize;
    let mut failures = Vec::new();

    for asset in assets {
        let document = semantic::asset_document(&asset);
        let hash = semantic::document_hash(&document);
        match semantic::embed_text(&state.semantic, &document).await {
            Ok(embedding) => {
                if let Err(err) = db::upsert_semantic_embedding(
                    &state.db,
                    &asset.row.id,
                    &state.semantic.model,
                    &hash,
                    &embedding,
                )
                .await
                {
                    failures.push(format!("{}: {}", asset.row.name, err));
                } else {
                    indexed += 1;
                }
            }
            Err(err) => failures.push(format!("{}: {}", asset.row.name, err)),
        }
    }

    info!(
        requested,
        indexed,
        failed = failures.len(),
        model = %state.semantic.model,
        "semantic reindex completed"
    );
    Ok(Json(semantic::reindex_response(
        state.semantic.model.clone(),
        requested,
        indexed,
        failures,
    )))
}

async fn reindex_asset_semantic(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let asset = db::get_asset(&state.db, &id, false).await?;
    let document = semantic::asset_document(&asset);
    let hash = semantic::document_hash(&document);
    let embedding = semantic::embed_text(&state.semantic, &document).await?;
    db::upsert_semantic_embedding(
        &state.db,
        &id,
        &state.semantic.model,
        &hash,
        &embedding,
    )
    .await?;
    info!(asset_id = %id, model = %state.semantic.model, "asset semantic index updated");
    Ok(Json(semantic::reindex_response(
        state.semantic.model.clone(),
        1,
        1,
        Vec::new(),
    )))
}

async fn engine_presets() -> Json<&'static [EnginePreset]> {
    Json(engine::presets())
}

async fn project_export_plan(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<ProjectExportPlan>> {
    let project = db::get_project(&state.db, &id).await?;
    Ok(Json(engine::plan(&project)?))
}

async fn backup_status(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<BackupStatusResponse>> {
    Ok(Json(backup::status(&state.backup).await?))
}

async fn create_backup(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<BackupCreateResponse>> {
    info!(
        backup_directory = %state.backup.directory.display(),
        replication_targets = state.backup.replication_targets.len(),
        "backup requested"
    );
    let response = backup::create_backup(
        &state.db,
        state.storage.root(),
        &state.database_filename,
        state.storage.archive_directory_opt(),
        &state.backup,
    )
    .await?;
    Ok(Json(response))
}

async fn verify_backup(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<BackupVerifyResponse>> {
    info!(backup_id = %id, "backup verification requested");
    Ok(Json(backup::verify_named_backup(&state.backup, &id).await?))
}

async fn license_presets() -> Json<&'static [crate::models::LicensePreset]> {
    Json(licensing::presets())
}

async fn asset_license_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let asset = db::get_asset(&state.db, &id, false).await?;
    Ok(Json(licensing::assess(&asset.row)))
}

async fn project_license_report(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let project = db::get_project(&state.db, &id).await?;
    let links = db::list_project_links(&state.db, &id).await?;
    let mut entries = Vec::with_capacity(links.len());

    for link in links {
        let asset = db::get_asset(&state.db, &link.asset_id, true).await?;
        let assessment = licensing::assess(&asset.row);
        entries.push(ProjectLicenseEntry {
            asset_id: asset.row.id.clone(),
            asset_name: asset.row.name.clone(),
            version_number: link.version_number,
            license_id: assessment.license_id.clone(),
            status: assessment.status.clone(),
            creator: asset.row.creator.clone(),
            source_url: asset.row.source_url.clone(),
            attribution_required: asset.row.attribution_required,
            warnings: assessment.warnings.clone(),
            credit_line: licensing::attribution_line(&asset.row),
        });
    }

    let warning_count = entries.iter().filter(|e| matches!(e.status, crate::models::LicenseStatus::Warning)).count();
    let unknown_count = entries.iter().filter(|e| matches!(e.status, crate::models::LicenseStatus::Unknown)).count();

    let mut credits = String::new();
    credits.push_str(&format!("{} — Asset Credits\n", project.name));
    credits.push_str("Generated by DragonForge Asset Vault\n\n");
    for entry in &entries {
        credits.push_str(&entry.credit_line);
        credits.push('\n');
    }

    let mut csv = String::from("asset_id,asset_name,version,license,status,creator,source_url,attribution_required\n");
    for entry in &entries {
        let escape = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
        csv.push_str(&format!(
            "{},{},{},{},{:?},{},{},{}\n",
            escape(&entry.asset_id),
            escape(&entry.asset_name),
            entry.version_number,
            escape(&entry.license_id),
            entry.status,
            escape(entry.creator.as_deref().unwrap_or("")),
            escape(entry.source_url.as_deref().unwrap_or("")),
            entry.attribution_required
        ));
    }

    info!(
        project_id = %id,
        entries = entries.len(),
        warning_count,
        unknown_count,
        "project license report generated"
    );

    Ok(Json(ProjectLicenseReport {
        project_id: id,
        generated_at: chrono::Utc::now().to_rfc3339(),
        entries,
        warning_count,
        unknown_count,
        credits_text: credits,
        csv_manifest: csv,
    }))
}

async fn stats(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    info!("vault stats requested");
    Ok(Json(db::stats(&state.db).await?))
}

async fn list_assets(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AssetQuery>,
) -> AppResult<impl IntoResponse> {
    info!(
        search = ?query.q,
        category = ?query.category,
        tag = ?query.tag,
        extension = ?query.extension,
        include_deleted = query.include_deleted,
        deleted_only = query.deleted_only,
        limit = ?query.limit,
        offset = ?query.offset,
        "asset list requested"
    );
    let assets = db::list_assets(&state.db, &query).await?;
    info!(count = assets.len(), "asset list returned");
    Ok(Json(assets))
}

async fn get_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset metadata requested");
    Ok(Json(db::get_asset(&state.db, &id, false).await?))
}

async fn upload_asset(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    info!("asset upload request received");
    let mut metadata = UploadMetadata::default();
    let mut incoming: Option<IncomingFile> = None;

    while let Some(field) = multipart.next_field().await? {
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "file" => {
                if incoming.is_some() {
                    warn!("upload rejected because more than one file was supplied");
                    return Err(AppError::BadRequest(
                        "only one file may be uploaded per request".to_string(),
                    ));
                }
                incoming = Some(storage::stream_field_to_temp(&state.storage, field).await?);
            }
            "name" => metadata.name = clean_optional(field.text().await?),
            "category" => metadata.category = clean_optional(field.text().await?),
            "description" => metadata.description = clean_optional(field.text().await?),
            "source_url" => metadata.source_url = clean_optional(field.text().await?),
            "creator" => metadata.creator = clean_optional(field.text().await?),
            "license" => metadata.license = clean_optional(field.text().await?),
            "attribution_required" => {
                metadata.attribution_required = parse_bool(&field.text().await?)?;
            }
            "tags" => {
                metadata.tags = field
                    .text()
                    .await?
                    .split(',')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect();
            }
            _ => {
                info!(field = %field_name, "unknown upload metadata field ignored");
            }
        }
    }

    let incoming = incoming.ok_or_else(|| {
        AppError::BadRequest("multipart field 'file' is required".to_string())
    })?;

    info!(
        filename = %incoming.original_filename,
        byte_size = incoming.byte_size,
        sha256 = %incoming.sha256,
        category = ?metadata.category,
        "asset upload parsed"
    );

    if let Some(existing) = db::get_asset_by_hash(&state.db, &incoming.sha256).await? {
        state.storage.remove_temp(&incoming.temp_path).await;
        info!(
            asset_id = %existing.row.id,
            name = %existing.row.name,
            sha256 = %incoming.sha256,
            "duplicate upload blocked"
        );
        let body = UploadResponse {
            duplicate: true,
            asset: existing,
        };
        return Ok((StatusCode::OK, Json(body)).into_response());
    }

    let fallback_name = std::path::Path::new(&incoming.original_filename)
        .file_stem()
        .and_then(|v| v.to_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(&incoming.original_filename)
        .to_string();
    let name = metadata.name.unwrap_or(fallback_name);
    if name.trim().is_empty() {
        state.storage.remove_temp(&incoming.temp_path).await;
        warn!("upload rejected because asset name was empty");
        return Err(AppError::BadRequest(
            "asset name cannot be empty".to_string(),
        ));
    }

    let final_path = match state
        .storage
        .commit_temp(
            &incoming.temp_path,
            &incoming.sha256,
            incoming.extension.as_deref(),
        )
        .await
    {
        Ok(path) => path,
        Err(err) => {
            state.storage.remove_temp(&incoming.temp_path).await;
            return Err(err);
        }
    };

    let relative_path = state.storage.relative_path(&final_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let row = AssetRow {
        id: Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        original_filename: incoming.original_filename,
        extension: incoming.extension,
        mime_type: incoming.mime_type,
        byte_size: incoming.byte_size,
        sha256: incoming.sha256,
        storage_path: relative_path,
        category: metadata.category,
        description: metadata.description,
        source_url: metadata.source_url,
        creator: metadata.creator,
        license: metadata.license,
        attribution_required: metadata.attribution_required,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    };

    let asset = db::insert_asset(&state.db, &row, &metadata.tags).await?;
    info!(
        asset_id = %asset.row.id,
        name = %asset.row.name,
        filename = %asset.row.original_filename,
        byte_size = asset.row.byte_size,
        "asset stored and cataloged"
    );

    refresh_semantic_index_nonfatal(&state, &asset).await;

    if thumbnail::is_previewable_asset(asset.row.extension.as_deref()) {
        if let Err(err) = thumbnail::get_or_create_preview(&state.storage, &asset.row).await {
            warn!(
                asset_id = %asset.row.id,
                extension = ?asset.row.extension,
                error = %err,
                "asset stored but preview generation failed"
            );
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(UploadResponse {
            duplicate: false,
            asset,
        }),
    )
        .into_response())
}

async fn update_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<UpdateAssetRequest>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset metadata update requested");
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    let asset = db::update_asset(&state.db, &id, request).await?;
    refresh_semantic_index_nonfatal(&state, &asset).await;
    info!(asset_id = %id, "asset metadata updated");
    Ok(Json(asset))
}

async fn delete_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset soft delete requested");
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    db::soft_delete(&state.db, &id).await?;
    db::delete_semantic_embedding(&state.db, &id).await?;
    db::clear_asset_checkout(&state.db, &id).await?;
    info!(asset_id = %id, "asset soft deleted; semantic index and checkout removed");
    Ok(Json(DeleteResponse { id, deleted: true }))
}

async fn download_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    info!(asset_id = %id, "asset download requested");
    let asset = db::get_asset(&state.db, &id, false).await?;
    let path = state.storage.resolve_relative(&asset.row.storage_path)?;
    let file = File::open(&path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound
        } else {
            AppError::Io(err)
        }
    })?;

    let mut headers = HeaderMap::new();
    let safe_filename: String = asset
        .row
        .original_filename
        .chars()
        .map(|c| {
            if c == char::from(13) || c == char::from(10) || c == '"' {
                '_'
            } else {
                c
            }
        })
        .collect();
    let disposition = format!(r#"attachment; filename="{}""#, safe_filename);
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .map_err(|_| AppError::BadRequest("invalid filename metadata".to_string()))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&asset.row.byte_size.to_string())
            .map_err(|_| AppError::Other(anyhow::anyhow!("invalid content length")))?,
    );
    if let Some(mime) = asset.row.mime_type.as_deref() {
        if let Ok(value) = HeaderValue::from_str(mime) {
            headers.insert(header::CONTENT_TYPE, value);
        }
    }

    info!(
        asset_id = %id,
        filename = %asset.row.original_filename,
        byte_size = asset.row.byte_size,
        path = %path.display(),
        "asset download started"
    );

    let stream = ReaderStream::new(file);
    Ok((headers, Body::from_stream(stream)).into_response())
}

async fn asset_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    info!(asset_id = %id, "legacy thumbnail route requested");
    let asset = db::get_asset(&state.db, &id, false).await?;
    let Some(path) = thumbnail::get_or_create_preview(&state.storage, &asset.row).await? else {
        return Err(AppError::NotFound);
    };
    serve_preview_file(&id, &path, "legacy thumbnail served").await
}

async fn asset_preview(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    info!(asset_id = %id, "preview requested");
    let asset = db::get_asset(&state.db, &id, false).await?;
    let package_files = db::list_package_files(&state.db, &id, asset.current_version).await?;
    let Some(path) = (if package_files.is_empty() {
        thumbnail::get_or_create_preview(&state.storage, &asset.row).await?
    } else {
        thumbnail::get_or_create_package_preview(
            &state.storage, &asset.row, asset.current_version, &package_files
        ).await?
    }) else {
        info!(
            asset_id = %id,
            extension = ?asset.row.extension,
            "preview unavailable for unsupported asset type"
        );
        return Err(AppError::NotFound);
    };

    serve_preview_file(&id, &path, "preview served").await
}

async fn serve_preview_file(id: &str, path: &std::path::Path, event: &str) -> AppResult<Response> {
    let metadata = tokio::fs::metadata(path).await?;
    let file = File::open(path).await?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&metadata.len().to_string())
            .map_err(|_| AppError::Other(anyhow::anyhow!("invalid preview content length")))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );

    info!(
        asset_id = %id,
        path = %path.display(),
        byte_size = metadata.len(),
        "{event}"
    );

    let stream = ReaderStream::new(file);
    Ok((headers, Body::from_stream(stream)).into_response())
}

async fn list_asset_versions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset version history requested");
    let versions = db::list_asset_versions(&state.db, &id).await?;
    info!(asset_id = %id, count = versions.len(), "asset version history returned");
    Ok(Json(versions))
}

async fn upload_asset_version(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    let current = db::get_asset(&state.db, &id, false).await?;
    info!(asset_id = %id, current_version = current.current_version, "new asset version upload requested");

    let mut incoming: Option<IncomingFile> = None;
    let mut note: Option<String> = None;

    while let Some(field) = multipart.next_field().await? {
        match field.name().unwrap_or_default() {
            "file" => {
                if incoming.is_some() {
                    return Err(AppError::BadRequest(
                        "only one file may be uploaded per version".to_string(),
                    ));
                }
                incoming = Some(storage::stream_field_to_temp(&state.storage, field).await?);
            }
            "note" => note = clean_optional(field.text().await?),
            _ => {}
        }
    }

    let incoming = incoming.ok_or_else(|| {
        AppError::BadRequest("multipart field 'file' is required".to_string())
    })?;

    if incoming.sha256 == current.row.sha256 {
        state.storage.remove_temp(&incoming.temp_path).await;
        return Err(AppError::Conflict(
            "new version is identical to the current version".to_string(),
        ));
    }

    let final_path = match state
        .storage
        .commit_temp(
            &incoming.temp_path,
            &incoming.sha256,
            incoming.extension.as_deref(),
        )
        .await
    {
        Ok(path) => path,
        Err(err) => {
            state.storage.remove_temp(&incoming.temp_path).await;
            return Err(err);
        }
    };

    let relative_path = state.storage.relative_path(&final_path)?;
    let asset = db::add_asset_version(
        &state.db,
        &id,
        incoming.original_filename,
        incoming.extension,
        incoming.mime_type,
        incoming.byte_size,
        incoming.sha256,
        relative_path,
        note,
    )
    .await?;

    if thumbnail::is_previewable_asset(asset.row.extension.as_deref()) {
        if let Err(err) = thumbnail::get_or_create_preview(&state.storage, &asset.row).await {
            warn!(asset_id = %id, error = %err, "new version stored but preview generation failed");
        }
    }

    refresh_semantic_index_nonfatal(&state, &asset).await;
    info!(
        asset_id = %id,
        version = asset.current_version,
        sha256 = %asset.row.sha256,
        "new asset version stored"
    );
    Ok((StatusCode::CREATED, Json(asset)))
}

async fn download_asset_version(
    State(state): State<Arc<AppState>>,
    Path((id, version)): Path<(String, i64)>,
) -> AppResult<Response> {
    info!(asset_id = %id, version, "asset version download requested");
    let item = db::get_asset_version(&state.db, &id, version).await?;
    let path = state.storage.resolve_relative(&item.storage_path)?;
    let file = File::open(&path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound
        } else {
            AppError::Io(err)
        }
    })?;

    let safe_filename: String = item
        .original_filename
        .chars()
        .map(|ch| if ch == char::from(13) || ch == char::from(10) || ch == '"' { '_' } else { ch })
        .collect();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(r#"attachment; filename="{}""#, safe_filename))
            .map_err(|_| AppError::BadRequest("invalid filename metadata".to_string()))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&item.byte_size.to_string())
            .map_err(|_| AppError::Other(anyhow::anyhow!("invalid content length")))?,
    );
    if let Some(mime) = item.mime_type.as_deref() {
        if let Ok(value) = HeaderValue::from_str(mime) {
            headers.insert(header::CONTENT_TYPE, value);
        }
    }

    info!(asset_id = %id, version, path = %path.display(), "asset version download started");
    Ok((headers, Body::from_stream(ReaderStream::new(file))).into_response())
}

async fn restore_asset_version(
    State(state): State<Arc<AppState>>,
    Path((id, version)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<RestoreVersionRequest>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, source_version = version, "asset version restore requested");
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    let asset = db::restore_asset_version(&state.db, &id, version, request.note).await?;

    if thumbnail::is_previewable_asset(asset.row.extension.as_deref()) {
        let package_files =
            db::list_package_files(&state.db, &id, asset.current_version).await?;
        let preview_result = if package_files.is_empty() {
            thumbnail::get_or_create_preview(&state.storage, &asset.row).await
        } else {
            thumbnail::get_or_create_package_preview(
                &state.storage,
                &asset.row,
                asset.current_version,
                &package_files,
            )
            .await
        };

        if let Err(err) = preview_result {
            warn!(
                asset_id = %id,
                current_version = asset.current_version,
                package_files = package_files.len(),
                error = %err,
                "restored version preview generation failed"
            );
        }
    }

    info!(
        asset_id = %id,
        source_version = version,
        current_version = asset.current_version,
        "asset version restored as new current revision"
    );
    Ok(Json(asset))
}

async fn import_package(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    info!("package import requested");
    let mut metadata = UploadMetadata::default();
    let mut incoming: Option<IncomingFile> = None;
    let mut primary_path: Option<String> = None;

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => incoming = Some(storage::stream_field_to_temp(&state.storage, field).await?),
            "primary_path" => primary_path = clean_optional(field.text().await?),
            "name" => metadata.name = clean_optional(field.text().await?),
            "category" => metadata.category = clean_optional(field.text().await?),
            "description" => metadata.description = clean_optional(field.text().await?),
            "source_url" => metadata.source_url = clean_optional(field.text().await?),
            "creator" => metadata.creator = clean_optional(field.text().await?),
            "license" => metadata.license = clean_optional(field.text().await?),
            "attribution_required" => metadata.attribution_required = parse_bool(&field.text().await?)?,
            "tags" => {
                metadata.tags = field.text().await?.split(',')
                    .map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
            }
            _ => {}
        }
    }

    let incoming = incoming.ok_or_else(|| AppError::BadRequest("package ZIP file is required".to_string()))?;
    if incoming.extension.as_deref() != Some("zip") {
        state.storage.remove_temp(&incoming.temp_path).await;
        return Err(AppError::BadRequest("package import requires a .zip file".to_string()));
    }

    let extracted = package::extract_zip(incoming.temp_path.clone(), primary_path).await?;
    state.storage.remove_temp(&incoming.temp_path).await;

    let asset_id = Uuid::new_v4().to_string();
    let stored = package::store_extracted_package(&state.storage, &asset_id, 1, extracted).await?;
    let primary = stored.files.iter().find(|file| file.is_primary)
        .cloned().ok_or_else(|| AppError::BadRequest("package has no primary file".to_string()))?;

    if db::get_asset_by_hash(&state.db, &primary.sha256).await?.is_some() {
        return Err(AppError::Conflict(
            "an asset with the same primary file already exists".to_string(),
        ));
    }

    let fallback_name = std::path::Path::new(&stored.primary_path)
        .file_stem().and_then(|v| v.to_str()).unwrap_or("Package").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let row = AssetRow {
        id: asset_id.clone(),
        name: metadata.name.unwrap_or(fallback_name),
        original_filename: primary.original_filename.clone(),
        extension: primary.extension.clone(),
        mime_type: primary.mime_type.clone(),
        byte_size: primary.byte_size,
        sha256: primary.sha256.clone(),
        storage_path: primary.storage_path.clone(),
        category: metadata.category,
        description: metadata.description,
        source_url: metadata.source_url,
        creator: metadata.creator,
        license: metadata.license,
        attribution_required: metadata.attribution_required,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    };
    let asset = db::insert_asset(&state.db, &row, &metadata.tags).await?;
    db::replace_package_files(&state.db, &asset_id, 1, &stored.files).await?;
    db::replace_package_dependencies(
        &state.db, &asset_id, 1,
        &stored.referenced_dependencies, &stored.missing_dependencies,
    ).await?;

    let manifest = PackageManifest {
        asset_id: asset_id.clone(),
        version_number: 1,
        primary_path: stored.primary_path,
        files: stored.files,
        referenced_dependencies: stored.referenced_dependencies,
        missing_dependencies: stored.missing_dependencies,
    };

    if thumbnail::is_previewable_asset(asset.row.extension.as_deref()) {
        if let Err(err) = thumbnail::get_or_create_package_preview(
            &state.storage, &asset.row, 1, &manifest.files
        ).await {
            warn!(asset_id = %asset_id, error = %err, "package imported but preview generation failed");
        }
    }

    refresh_semantic_index_nonfatal(&state, &asset).await;
    info!(
        asset_id = %asset_id,
        file_count = manifest.files.len(),
        missing_dependencies = manifest.missing_dependencies.len(),
        "package imported"
    );
    Ok((StatusCode::CREATED, Json(PackageImportResponse { asset, manifest })))
}

async fn get_current_package(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let asset = db::get_asset(&state.db, &id, false).await?;
    Ok(Json(build_package_manifest(&state, &id, asset.current_version).await?))
}

async fn get_package_version_manifest(
    State(state): State<Arc<AppState>>,
    Path((id, version)): Path<(String, i64)>,
) -> AppResult<impl IntoResponse> {
    Ok(Json(build_package_manifest(&state, &id, version).await?))
}

async fn upload_package_version(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    let current = db::get_asset(&state.db, &id, false).await?;
    let mut incoming: Option<IncomingFile> = None;
    let mut primary_path: Option<String> = None;
    let mut note: Option<String> = None;

    while let Some(field) = multipart.next_field().await? {
        match field.name().unwrap_or_default() {
            "file" => incoming = Some(storage::stream_field_to_temp(&state.storage, field).await?),
            "primary_path" => primary_path = clean_optional(field.text().await?),
            "note" => note = clean_optional(field.text().await?),
            _ => {}
        }
    }

    let incoming = incoming.ok_or_else(|| AppError::BadRequest("package ZIP file is required".to_string()))?;
    if incoming.extension.as_deref() != Some("zip") {
        state.storage.remove_temp(&incoming.temp_path).await;
        return Err(AppError::BadRequest("package version requires a .zip file".to_string()));
    }

    let extracted = package::extract_zip(incoming.temp_path.clone(), primary_path).await?;
    state.storage.remove_temp(&incoming.temp_path).await;

    let next_version = current.current_version + 1;
    let stored = package::store_extracted_package(&state.storage, &id, next_version, extracted).await?;
    let primary = stored.files.iter().find(|file| file.is_primary)
        .cloned().ok_or_else(|| AppError::BadRequest("package has no primary file".to_string()))?;

    let asset = db::add_package_version(
        &state.db, &id,
        primary.original_filename.clone(), primary.extension.clone(), primary.mime_type.clone(),
        primary.byte_size, primary.sha256.clone(), primary.storage_path.clone(), note,
    ).await?;

    db::replace_package_files(&state.db, &id, asset.current_version, &stored.files).await?;
    db::replace_package_dependencies(
        &state.db, &id, asset.current_version,
        &stored.referenced_dependencies, &stored.missing_dependencies,
    ).await?;

    let manifest = PackageManifest {
        asset_id: id.clone(),
        version_number: asset.current_version,
        primary_path: stored.primary_path,
        files: stored.files,
        referenced_dependencies: stored.referenced_dependencies,
        missing_dependencies: stored.missing_dependencies,
    };

    if thumbnail::is_previewable_asset(asset.row.extension.as_deref()) {
        if let Err(err) = thumbnail::get_or_create_package_preview(
            &state.storage, &asset.row, asset.current_version, &manifest.files
        ).await {
            warn!(asset_id = %id, error = %err, "package version stored but preview generation failed");
        }
    }

    refresh_semantic_index_nonfatal(&state, &asset).await;
    info!(
        asset_id = %id,
        version = asset.current_version,
        file_count = manifest.files.len(),
        missing_dependencies = manifest.missing_dependencies.len(),
        "package version stored"
    );
    Ok((StatusCode::CREATED, Json(PackageVersionResponse { asset, manifest })))
}

async fn download_package_file(
    State(state): State<Arc<AppState>>,
    Path((id, version, file_id)): Path<(String, i64, String)>,
) -> AppResult<Response> {
    let item = db::get_package_file(&state.db, &id, version, &file_id).await?;
    let path = state.storage.resolve_relative(&item.storage_path)?;
    let file = File::open(&path).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&item.byte_size.to_string())
            .map_err(|_| AppError::Other(anyhow::anyhow!("invalid package file length")))?,
    );
    if let Some(mime) = item.mime_type.as_deref() {
        if let Ok(value) = HeaderValue::from_str(mime) {
            headers.insert(header::CONTENT_TYPE, value);
        }
    }
    info!(
        asset_id = %id, version, relative_path = %item.relative_path,
        "package file download started"
    );
    Ok((headers, Body::from_stream(ReaderStream::new(file))).into_response())
}

async fn build_package_manifest(
    state: &AppState,
    asset_id: &str,
    version: i64,
) -> AppResult<PackageManifest> {
    let version_row = db::get_asset_version(&state.db, asset_id, version).await?;
    let mut files = db::list_package_files(&state.db, asset_id, version).await?;
    if files.is_empty() {
        files.push(crate::models::PackageFile {
            id: format!("single-{version}"),
            asset_id: asset_id.to_string(),
            version_number: version,
            relative_path: version_row.original_filename.clone(),
            original_filename: version_row.original_filename.clone(),
            extension: version_row.extension.clone(),
            mime_type: version_row.mime_type.clone(),
            byte_size: version_row.byte_size,
            sha256: version_row.sha256.clone(),
            storage_path: version_row.storage_path.clone(),
            is_primary: true,
            created_at: version_row.created_at.clone(),
        });
    }
    let primary_path = files.iter().find(|file| file.is_primary)
        .map(|file| file.relative_path.clone())
        .unwrap_or_else(|| version_row.original_filename.clone());
    let (referenced_dependencies, missing_dependencies) =
        db::list_package_dependencies(&state.db, asset_id, version).await?;
    Ok(PackageManifest {
        asset_id: asset_id.to_string(),
        version_number: version,
        primary_path,
        files,
        referenced_dependencies,
        missing_dependencies,
    })
}

async fn restore_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset restore requested");
    ensure_asset_mutation_allowed(&state, &headers, &id).await?;
    db::restore_asset(&state.db, &id).await?;
    let restored_asset = db::get_asset(&state.db, &id, false).await?;
    refresh_semantic_index_nonfatal(&state, &restored_asset).await;
    info!(asset_id = %id, "asset restored");
    Ok(Json(RestoreResponse { id, restored: true }))
}

async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    info!("project list requested");
    let projects = db::list_projects(&state.db).await?;
    info!(count = projects.len(), "project list returned");
    Ok(Json(projects))
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(project_id = %id, "project requested");
    Ok(Json(db::get_project(&state.db, &id).await?))
}

async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateProjectRequest>,
) -> AppResult<impl IntoResponse> {
    info!(
        name = %request.name,
        engine = ?request.engine,
        local_path = %request.local_path,
        "project create requested"
    );
    let project = db::create_project(&state.db, request).await?;
    info!(project_id = %project.id, name = %project.name, "project created");
    Ok((StatusCode::CREATED, Json(project)))
}

async fn update_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> AppResult<impl IntoResponse> {
    info!(project_id = %id, "project update requested");
    let project = db::update_project(&state.db, &id, request).await?;
    info!(project_id = %id, "project updated");
    Ok(Json(project))
}

async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(project_id = %id, "project delete requested");
    db::delete_project(&state.db, &id).await?;
    info!(project_id = %id, "project deleted");
    Ok(StatusCode::NO_CONTENT)
}

async fn list_project_assets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(project_id = %id, "project asset list requested");
    let assets = db::list_project_assets(&state.db, &id).await?;
    info!(project_id = %id, count = assets.len(), "project asset list returned");
    Ok(Json(assets))
}

async fn add_project_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ProjectAssetRequest>,
) -> AppResult<impl IntoResponse> {
    info!(
        project_id = %id,
        asset_id = %request.asset_id,
        relative_path = ?request.relative_path,
        version_number = ?request.version_number,
        "project asset registration requested"
    );
    let link = db::add_project_asset(&state.db, &id, request).await?;
    info!(
        project_id = %link.project_id,
        asset_id = %link.asset_id,
        "project asset registered"
    );
    Ok((StatusCode::CREATED, Json(link)))
}

async fn get_project_asset_link(
    State(state): State<Arc<AppState>>,
    Path((id, asset_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    info!(project_id = %id, asset_id = %asset_id, "project asset link requested");
    Ok(Json(db::get_project_asset_link(&state.db, &id, &asset_id).await?))
}

async fn remove_project_asset(
    State(state): State<Arc<AppState>>,
    Path((id, asset_id)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    info!(project_id = %id, asset_id = %asset_id, "project asset removal requested");
    db::remove_project_asset(&state.db, &id, &asset_id).await?;
    info!(project_id = %id, asset_id = %asset_id, "project asset removed");
    Ok(StatusCode::NO_CONTENT)
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_bool(value: &str) -> AppResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" | "" => Ok(false),
        other => Err(AppError::BadRequest(format!(
            "invalid boolean value: {other}"
        ))),
    }
}
