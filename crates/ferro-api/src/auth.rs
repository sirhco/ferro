use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use ferro_auth::{AuthContext, JwtClaims};
use ferro_core::{Role, RoleId, User};

use crate::error::ApiError;
use crate::state::AppState;

/// Authenticated caller: validated JWT, hydrated user, roles resolved into
/// an `AuthContext` ready for policy checks.
#[derive(Clone)]
pub struct AuthUser {
    pub user: User,
    pub claims: JwtClaims,
    pub ctx: AuthContext,
}

impl AuthUser {
    /// Try to resolve an `AuthUser` from request headers. Returns `Ok(None)` if
    /// the request is unauthenticated; returns `Err` only for malformed tokens
    /// or backend failures. Callers decide whether missing auth is fatal.
    pub async fn try_from_headers(
        state: &AppState,
        headers: &HeaderMap,
    ) -> Result<Option<Self>, ApiError> {
        let Some(header) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
            return Ok(None);
        };
        let token = match header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
        {
            Some(t) if !t.trim().is_empty() => t.trim(),
            _ => return Ok(None),
        };

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
        Ok(Some(Self { user, claims, ctx }))
    }
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        AuthUser::try_from_headers(state, &parts.headers)
            .await?
            .ok_or(ApiError::Unauthorized)
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
