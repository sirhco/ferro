use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use ferro_auth::{authorize, AuthContext};
use ferro_core::{
    Content, ContentPatch, ContentQuery, ContentType, ContentTypeId, NewContent, Page,
    Permission, Scope, Site, User,
};
use ferro_plugin::HookEvent;
use ferro_storage::schema as schema_migrator;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/sites", get(list_sites))
        .route(
            "/api/v1/content/{type_slug}",
            get(list_content).post(create_content),
        )
        .route(
            "/api/v1/content/{type_slug}/{slug}",
            get(get_content).patch(update_content).delete(delete_content),
        )
        .route(
            "/api/v1/content/{type_slug}/{slug}/publish",
            post(publish_content),
        )
        .route("/api/v1/types", get(list_types).post(create_type))
        .route(
            "/api/v1/types/{slug}",
            get(get_type).patch(update_type).delete(delete_type),
        )
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<Arc<AppState>>) -> ApiResult<&'static str> {
    state.repo.health().await?;
    Ok("ok")
}

async fn list_sites(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Site>>> {
    Ok(Json(state.repo.sites().list().await?))
}

#[derive(Debug, Deserialize)]
struct ListParams {
    locale: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

async fn list_content(
    State(state): State<Arc<AppState>>,
    Path(type_slug): Path<String>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<Page<Content>>> {
    let q = ContentQuery {
        type_slug: Some(type_slug),
        locale: params.locale.and_then(|l| l.parse().ok()),
        status: params
            .status
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
        page: params.page,
        per_page: params.per_page,
        ..Default::default()
    };
    Ok(Json(state.repo.content().list(q).await?))
}

async fn get_content(
    State(state): State<Arc<AppState>>,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<Json<Content>> {
    let (_site, _ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    Ok(Json(content))
}

async fn create_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(type_slug): Path<String>,
    Json(body): Json<NewContent>,
) -> ApiResult<Json<Content>> {
    let (site, ty) = resolve_type(&state, &type_slug).await?;
    if body.type_id != ty.id {
        return Err(ApiError::BadRequest(
            "type_id does not match URL type slug".into(),
        ));
    }
    require_write(&auth.ctx, ty.id)?;
    body.validate(&ty)?;
    let created = state.repo.content().create(site.id, body).await?;
    state.hooks.dispatch(HookEvent::ContentCreated { content: created.clone() }).await;
    Ok(Json(created))
}

async fn update_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
    Json(patch): Json<ContentPatch>,
) -> ApiResult<Json<Content>> {
    let (_site, ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    require_write(&auth.ctx, ty.id)?;
    patch.validate(&ty)?;
    let before = content.clone();
    let after = state.repo.content().update(content.id, patch).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentUpdated {
            before: Box::new(before),
            after: Box::new(after.clone()),
        })
        .await;
    Ok(Json(after))
}

async fn delete_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let (site, ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    require_write(&auth.ctx, ty.id)?;
    state.repo.content().delete(content.id).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentDeleted {
            site_id: site.id,
            type_id: ty.id,
            content_id: content.id,
            slug: content.slug,
        })
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn publish_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((type_slug, slug)): Path<(String, String)>,
) -> ApiResult<Json<Content>> {
    let (_site, ty, content) = resolve_entry(&state, &type_slug, &slug).await?;
    authorize(&auth.ctx, Permission::Publish(Scope::Type { id: ty.id }))
        .map_err(|_| ApiError::Forbidden("publish denied".into()))?;
    let published = state.repo.content().publish(content.id).await?;
    state
        .hooks
        .dispatch(HookEvent::ContentPublished { content: published.clone() })
        .await;
    Ok(Json(published))
}

#[derive(Debug, Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
    user: User,
}

const JWT_TTL_SECS: i64 = 60 * 60 * 12; // 12 hours

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<LoginResponse>> {
    let (user, _session) = state.auth.login(&body.email, &body.password, None, None).await?;
    let role_names: Vec<String> = user.roles.iter().map(|r| r.to_string()).collect();
    let token = state
        .jwt
        .mint(user.id, role_names, JWT_TTL_SECS)
        .map_err(ApiError::Auth)?;
    Ok(Json(LoginResponse { token, user }))
}

async fn logout(State(_state): State<Arc<AppState>>, _auth: AuthUser) -> ApiResult<StatusCode> {
    // Stateless JWT: nothing to revoke server-side without a denylist.
    // Client discards the token; 204 signals the intent.
    Ok(StatusCode::NO_CONTENT)
}

async fn me(auth: AuthUser) -> Json<User> {
    Json(auth.user)
}

async fn resolve_type(state: &AppState, type_slug: &str) -> ApiResult<(Site, ContentType)> {
    let sites = state.repo.sites().list().await?;
    let site = sites.into_iter().next().ok_or(ApiError::NotFound)?;
    let ty = state
        .repo
        .types()
        .by_slug(site.id, type_slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((site, ty))
}

async fn resolve_entry(
    state: &AppState,
    type_slug: &str,
    slug: &str,
) -> ApiResult<(Site, ContentType, Content)> {
    let (site, ty) = resolve_type(state, type_slug).await?;
    let content = state
        .repo
        .content()
        .by_slug(site.id, ty.id, slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((site, ty, content))
}

fn require_write(ctx: &AuthContext, ty: ContentTypeId) -> ApiResult<()> {
    authorize(ctx, Permission::Write(Scope::Type { id: ty }))
        .map_err(|_| ApiError::Forbidden("write denied".into()))
}

fn require_manage_schema(ctx: &AuthContext) -> ApiResult<()> {
    authorize(ctx, Permission::ManageSchema)
        .map_err(|_| ApiError::Forbidden("schema management denied".into()))
}

// --- Content-type routes ---

async fn list_types(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<ContentType>>> {
    let sites = state.repo.sites().list().await?;
    let Some(site) = sites.into_iter().next() else {
        return Ok(Json(Vec::new()));
    };
    Ok(Json(state.repo.types().list(site.id).await?))
}

async fn get_type(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> ApiResult<Json<ContentType>> {
    let (_site, ty) = resolve_type(&state, &slug).await?;
    Ok(Json(ty))
}

async fn create_type(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut ty): Json<ContentType>,
) -> ApiResult<Json<ContentType>> {
    require_manage_schema(&auth.ctx)?;
    let sites = state.repo.sites().list().await?;
    let site = sites.into_iter().next().ok_or(ApiError::NotFound)?;
    // Force site scoping to the caller's site (prevents client-supplied site_id
    // from pointing at an unrelated tenant).
    ty.site_id = site.id;
    Ok(Json(state.repo.types().upsert(ty).await?))
}

#[derive(Debug, Serialize)]
struct TypeUpdateResponse {
    #[serde(rename = "type")]
    ty: ContentType,
    /// Number of content rows rewritten by the schema migrator.
    rows_migrated: u64,
    /// Field changes that were applied.
    changes: Vec<ferro_core::FieldChange>,
}

async fn update_type(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(new_ty): Json<ContentType>,
) -> ApiResult<Json<TypeUpdateResponse>> {
    require_manage_schema(&auth.ctx)?;
    let (site, old) = resolve_type(&state, &slug).await?;
    if new_ty.id != old.id {
        return Err(ApiError::BadRequest("content-type id cannot change".into()));
    }
    let changes = ContentType::diff(&old, &new_ty);
    let saved = state.repo.types().upsert(new_ty).await?;
    let rows_migrated =
        schema_migrator::apply_changes(&*state.repo, site.id, saved.id, &changes).await?;
    state
        .hooks
        .dispatch(HookEvent::TypeMigrated {
            site_id: site.id,
            type_id: saved.id,
            rows_migrated,
            changes: changes.clone(),
        })
        .await;
    Ok(Json(TypeUpdateResponse { ty: saved, rows_migrated, changes }))
}

async fn delete_type(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> ApiResult<StatusCode> {
    require_manage_schema(&auth.ctx)?;
    let (_site, ty) = resolve_type(&state, &slug).await?;
    state.repo.types().delete(ty.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
