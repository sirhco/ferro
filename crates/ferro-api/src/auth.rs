use std::sync::Arc;

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use ferro_auth::{AuthContext, JwtClaims};
use ferro_core::{Role, RoleId, User};

use crate::error::ApiError;
use crate::state::AppState;

/// Axum extractor that validates a bearer JWT, loads the user, and hydrates
/// an `AuthContext` for policy checks.
pub struct AuthUser {
    pub user: User,
    pub claims: JwtClaims,
    pub ctx: AuthContext,
}

#[async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or(ApiError::Unauthorized)?
            .trim();
        if token.is_empty() {
            return Err(ApiError::Unauthorized);
        }

        let claims = state.jwt.verify(token).map_err(|_| ApiError::Unauthorized)?;
        let user_id = claims.user_id().map_err(|_| ApiError::Unauthorized)?;
        let user = state
            .repo
            .users()
            .get(user_id)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        if !user.active {
            return Err(ApiError::Forbidden("account disabled".into()));
        }

        let roles = resolve_roles(state, &user.roles).await?;
        let ctx = AuthContext { user_id: user.id, roles };

        Ok(Self { user, claims, ctx })
    }
}

async fn resolve_roles(state: &AppState, ids: &[RoleId]) -> Result<Vec<Role>, ApiError> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(role) = state.repo.users().get_role(*id).await? {
            out.push(role);
        }
    }
    Ok(out)
}
