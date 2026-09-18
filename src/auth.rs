use crate::{
    db,
    error::{AppError, AppResult},
    models::{AuthMeResponse, UserRole, VaultUser},
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

    if !state.auth.enabled {
        request.extensions_mut().insert(AuthContext {
            enabled: false,
            user: None,
            role: UserRole::Administrator,
        });
        return Ok(next.run(request).await);
    }

    let token = bearer_token(&request)?;
    let hash = hash_token(token)?;
    let user = db::get_user_by_token_hash(&state.db, &hash)
        .await?
        .ok_or_else(|| AppError::Unauthorized("invalid API token".to_string()))?;
    if !user.enabled {
        return Err(AppError::Forbidden("this DragonForge user is disabled".to_string()));
    }

    let role = parse_role(&user.role)?;
    authorize(&request, &role)?;

    db::touch_user(&state.db, &user.id).await?;
    let username = user.username.clone();
    request.headers_mut().insert(
        "x-dragonforge-user",
        HeaderValue::from_str(&username.replace(['\r', '\n'], "_"))
            .map_err(|_| AppError::BadRequest("invalid username header value".to_string()))?,
    );
    request.extensions_mut().insert(AuthContext {
        enabled: true,
        user: Some(user),
        role,
    });

    Ok(next.run(request).await)
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
