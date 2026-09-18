use crate::{
    db,
    error::{AppError, AppResult},
    models::{AuditEvent, AuthMeResponse, UserRole, VaultUser},
    routes::AppState,
};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, Method},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub enabled: bool,
    pub user: Option<VaultUser>,
    pub role: UserRole,
}

impl AuthContext {
    pub fn username(&self) -> Option<&str> {
        self.user.as_ref().map(|user| user.username.as_str())
    }

    pub fn me_response(&self) -> AuthMeResponse {
        AuthMeResponse {
            enabled: self.enabled,
            authenticated: self.user.is_some() || !self.enabled,
            username: self.username().map(ToString::to_string),
            role: Some(self.role.clone()),
        }
    }
}

pub fn hash_token(token: &str) -> AppResult<String> {
    let trimmed = token.trim();
    if trimmed.len() < 16 {
        return Err(AppError::BadRequest(
            "API tokens must be at least 16 characters".to_string(),
        ));
    }
    Ok(hex::encode(Sha256::digest(trimmed.as_bytes())))
}

pub fn parse_role(value: &str) -> AppResult<UserRole> {
    match value {
        "administrator" => Ok(UserRole::Administrator),
        "developer" => Ok(UserRole::Developer),
        "read_only" => Ok(UserRole::ReadOnly),
        other => Err(AppError::Other(anyhow::anyhow!(
            "invalid persisted user role: {other}"
        ))),
    }
}

pub async fn initialize_bootstrap(state: &AppState) -> AppResult<()> {
    if !state.auth.enabled {
        return Ok(());
    }

    let count = db::count_users(&state.db).await?;
    if count > 0 {
        return Ok(());
    }

    let username = state
        .auth
        .bootstrap_admin_user
        .as_deref()
        .unwrap_or("admin")
        .trim();
    let token = state
        .auth
        .bootstrap_admin_token
        .as_deref()
        .ok_or_else(|| {
            AppError::BadRequest(
                "authentication is enabled but no users exist and auth.bootstrap_admin_token is missing"
                    .to_string(),
            )
        })?;
    let hash = hash_token(token)?;
    if let Some(user) = db::ensure_bootstrap_admin(&state.db, username, &hash).await? {
        info!(username = %user.username, "Phase 13 bootstrap administrator created");
    }
    Ok(())
}

pub async fn middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    if request.uri().path() == "/api/health" {
        return Ok(next.run(request).await);
    }

    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let workstation = request
        .headers()
        .get("x-dragonforge-workstation")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    if !state.auth.enabled {
        let username = request
            .headers()
            .get("x-dragonforge-user")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        request.extensions_mut().insert(AuthContext {
            enabled: false,
            user: None,
            role: UserRole::Administrator,
        });
        let response = next.run(request).await;
        record_audit(
            &state,
            &method,
            &path,
            None,
            username.as_deref(),
            Some(UserRole::Administrator.as_str()),
            workstation.as_deref(),
            response.status().as_u16(),
            None,
        )
        .await;
        return Ok(response);
    }

    let token = match bearer_token(&request) {
        Ok(token) => token,
        Err(err) => {
            record_audit(
                &state,
                &method,
                &path,
                None,
                None,
                None,
                workstation.as_deref(),
                401,
                Some("authentication failed"),
            )
            .await;
            return Err(err);
        }
    };
    let hash = hash_token(token)?;
    let user = match db::get_user_by_token_hash(&state.db, &hash).await? {
        Some(user) => user,
        None => {
            record_audit(
                &state,
                &method,
                &path,
                None,
                None,
                None,
                workstation.as_deref(),
                401,
                Some("invalid API token"),
            )
            .await;
            return Err(AppError::Unauthorized("invalid API token".to_string()));
        }
    };
    if !user.enabled {
        record_audit(
            &state,
            &method,
            &path,
            Some(&user.id),
            Some(&user.username),
            Some(&user.role),
            workstation.as_deref(),
            403,
            Some("user disabled"),
        )
        .await;
        return Err(AppError::Forbidden("this DragonForge user is disabled".to_string()));
    }

    let role = parse_role(&user.role)?;
    if let Err(err) = authorize(&request, &role) {
        record_audit(
            &state,
            &method,
            &path,
            Some(&user.id),
            Some(&user.username),
            Some(role.as_str()),
            workstation.as_deref(),
            403,
            Some("authorization denied"),
        )
        .await;
        return Err(err);
    }

    db::touch_user(&state.db, &user.id).await?;
    let username = user.username.clone();
    request.headers_mut().insert(
        "x-dragonforge-user",
        HeaderValue::from_str(&username.replace(['\r', '\n'], "_"))
            .map_err(|_| AppError::BadRequest("invalid username header value".to_string()))?,
    );
    request.extensions_mut().insert(AuthContext {
        enabled: true,
        user: Some(user.clone()),
        role: role.clone(),
    });

    let response = next.run(request).await;
    record_audit(
        &state,
        &method,
        &path,
        Some(&user.id),
        Some(&user.username),
        Some(role.as_str()),
        workstation.as_deref(),
        response.status().as_u16(),
        None,
    )
    .await;
    Ok(response)
}

async fn record_audit(
    state: &AppState,
    method: &Method,
    path: &str,
    actor_user_id: Option<&str>,
    actor_username: Option<&str>,
    actor_role: Option<&str>,
    workstation: Option<&str>,
    status_code: u16,
    detail: Option<&str>,
) {
    let (target_type, target_id) = audit_target(path);
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        actor_user_id: actor_user_id.map(ToString::to_string),
        actor_username: actor_username.map(ToString::to_string),
        actor_role: actor_role.map(ToString::to_string),
        workstation: workstation.map(ToString::to_string),
        action: audit_action(method, path),
        method: method.to_string(),
        path: path.to_string(),
        target_type,
        target_id,
        result: if (200..400).contains(&status_code) {
            "success".to_string()
        } else {
            "failure".to_string()
        },
        status_code: i64::from(status_code),
        detail: detail.map(ToString::to_string),
    };
    if let Err(err) = db::insert_audit_event(&state.db, &event).await {
        warn!(error = %err, action = %event.action, path = %event.path, "audit event could not be recorded");
    }
}

fn audit_action(method: &Method, path: &str) -> String {
    if path == "/api/assets" && *method == Method::POST {
        return "asset.upload".to_string();
    }
    if path == "/api/packages" && *method == Method::POST {
        return "package.import".to_string();
    }
    if path == "/api/projects" && *method == Method::POST {
        return "project.create".to_string();
    }
    if path == "/api/users" && *method == Method::POST {
        return "user.create".to_string();
    }
    if path.starts_with("/api/backups") && *method == Method::POST {
        return if path.ends_with("/verify") { "backup.verify" } else { "backup.create" }.to_string();
    }
    if path.ends_with("/checkout") {
        return match *method {
            Method::POST => "asset.checkout",
            Method::DELETE => "asset.checkin",
            _ => "asset.checkout.view",
        }
        .to_string();
    }
    if path.ends_with("/archive") {
        return "asset.archive".to_string();
    }
    if path.ends_with("/recall") {
        return "asset.recall".to_string();
    }
    if path.ends_with("/restore") && path.contains("/versions/") {
        return "asset.version.restore".to_string();
    }
    if path.ends_with("/restore") {
        return "asset.restore".to_string();
    }
    if path.contains("/package-versions") && *method == Method::POST {
        return "package.version.upload".to_string();
    }
    if path.ends_with("/versions") && *method == Method::POST {
        return "asset.version.upload".to_string();
    }
    if path.contains("/projects/") && path.contains("/assets/") && *method == Method::DELETE {
        return "project.asset.remove".to_string();
    }
    if path.contains("/projects/") && path.ends_with("/assets") && *method == Method::POST {
        return "project.asset.add".to_string();
    }
    if path.starts_with("/api/users/") {
        return match *method {
            Method::PATCH => "user.update",
            Method::DELETE => "user.delete",
            _ => "user.view",
        }
        .to_string();
    }
    if path.starts_with("/api/projects/") {
        return match *method {
            Method::PATCH => "project.update",
            Method::DELETE => "project.delete",
            _ => "project.view",
        }
        .to_string();
    }
    if path.starts_with("/api/assets/") {
        return match *method {
            Method::PATCH => "asset.update",
            Method::DELETE => "asset.delete",
            _ => "asset.view",
        }
        .to_string();
    }
    if path.starts_with("/api/audit") {
        return "audit.view".to_string();
    }
    format!("api.{}", method.as_str().to_ascii_lowercase())
}

fn audit_target(path: &str) -> (Option<String>, Option<String>) {
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() >= 3 && parts[0] == "api" {
        let kind = match parts[1] {
            "assets" => "asset",
            "projects" => "project",
            "users" => "user",
            "backups" => "backup",
            _ => return (None, None),
        };
        return (Some(kind.to_string()), Some(parts[2].to_string()));
    }
    (None, None)
}

fn bearer_token(request: &Request<Body>) -> AppResult<&str> {
    let value = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing Authorization: Bearer token".to_string()))?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Unauthorized("invalid Authorization header".to_string()))
}

fn authorize(request: &Request<Body>, role: &UserRole) -> AppResult<()> {
    let path = request.uri().path();
    let method = request.method();

    if path == "/api/audit/export" {
        return if role.is_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Administrator role is required for audit export".to_string(),
            ))
        };
    }

    if path.starts_with("/api/users") {
        return if role.is_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Administrator role is required for user management".to_string(),
            ))
        };
    }

    if path.starts_with("/api/backups") && method != Method::GET {
        return if role.is_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Administrator role is required for backup mutations".to_string(),
            ))
        };
    }

    if matches!(*method, Method::GET | Method::HEAD) {
        return Ok(());
    }

    if role.can_write() {
        Ok(())
    } else {
        warn!(%path, %method, "read-only user attempted mutation");
        Err(AppError::Forbidden(
            "Developer or Administrator role is required for this operation".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable() {
        let a = hash_token("1234567890abcdef").unwrap();
        let b = hash_token("1234567890abcdef").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn short_tokens_are_rejected() {
        assert!(hash_token("short").is_err());
    }

    #[test]
    fn persisted_roles_parse() {
        assert_eq!(parse_role("developer").unwrap(), UserRole::Developer);
        assert!(parse_role("owner").is_err());
    }
}
