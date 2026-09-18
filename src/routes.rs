use crate::{
    db,
    error::{AppError, AppResult},
    models::{
        AssetQuery, AssetRow, CreateProjectRequest, DeleteResponse, HealthResponse,
        ProjectAssetRequest, RestoreResponse, UpdateAssetRequest, UpdateProjectRequest,
        UploadMetadata, UploadResponse,
    },
    storage::{self, IncomingFile, Storage},
    thumbnail,
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub storage: Storage,
}

pub fn router(state: AppState, max_upload_bytes: usize) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/stats", get(stats))
        .route("/api/assets", get(list_assets).post(upload_asset))
        .route(
            "/api/assets/:id",
            get(get_asset).patch(update_asset).delete(delete_asset),
        )
        .route("/api/assets/:id/download", get(download_asset))
        .route("/api/assets/:id/thumbnail", get(asset_thumbnail))
        .route("/api/assets/:id/restore", axum::routing::post(restore_asset))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).patch(update_project).delete(delete_project),
        )
        .route(
            "/api/projects/:id/assets",
            get(list_project_assets).post(add_project_asset),
        )
        .route(
            "/api/projects/:id/assets/:asset_id",
            axum::routing::delete(remove_project_asset),
        )
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state))
}

async fn health() -> Json<HealthResponse> {
    info!("health check");
    Json(HealthResponse {
        ok: true,
        service: "dragonforge-asset-vault",
        phase: 4,
        version: env!("CARGO_PKG_VERSION"),
    })
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

    if thumbnail::is_previewable_image(asset.row.extension.as_deref()) {
        if let Err(err) = thumbnail::get_or_create_thumbnail(&state.storage, &asset.row).await {
            warn!(
                asset_id = %asset.row.id,
                error = %err,
                "asset stored but thumbnail generation failed"
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
    Json(request): Json<UpdateAssetRequest>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset metadata update requested");
    let asset = db::update_asset(&state.db, &id, request).await?;
    info!(asset_id = %id, "asset metadata updated");
    Ok(Json(asset))
}

async fn delete_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset soft delete requested");
    db::soft_delete(&state.db, &id).await?;
    info!(asset_id = %id, "asset soft deleted");
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
    info!(asset_id = %id, "thumbnail requested");
    let asset = db::get_asset(&state.db, &id, false).await?;
    let Some(path) = thumbnail::get_or_create_thumbnail(&state.storage, &asset.row).await? else {
        info!(asset_id = %id, "thumbnail unavailable for non-image asset");
        return Err(AppError::NotFound);
    };

    let metadata = tokio::fs::metadata(&path).await?;
    let file = File::open(&path).await?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&metadata.len().to_string())
            .map_err(|_| AppError::Other(anyhow::anyhow!("invalid thumbnail content length")))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );

    info!(
        asset_id = %id,
        path = %path.display(),
        byte_size = metadata.len(),
        "thumbnail served"
    );

    let stream = ReaderStream::new(file);
    Ok((headers, Body::from_stream(stream)).into_response())
}

async fn restore_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    info!(asset_id = %id, "asset restore requested");
    db::restore_asset(&state.db, &id).await?;
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
