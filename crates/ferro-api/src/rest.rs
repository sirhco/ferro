use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use ferro_core::{Content, ContentQuery, NewContent, Page, Site};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/sites", get(list_sites))
        .route("/api/v1/content/:type_slug", get(list_content).post(create_content))
        .route("/api/v1/content/:type_slug/:slug", get(get_content))
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
        status: params.status.and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
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
    let sites = state.repo.sites().list().await?;
    let site = sites.first().ok_or(ApiError::NotFound)?;
    let ty = state
        .repo
        .types()
        .by_slug(site.id, &type_slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let content = state
        .repo
        .content()
        .by_slug(site.id, ty.id, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(content))
}

async fn create_content(
    State(state): State<Arc<AppState>>,
    Path(type_slug): Path<String>,
    Json(body): Json<NewContent>,
) -> ApiResult<Json<Content>> {
    let sites = state.repo.sites().list().await?;
    let site = sites.first().ok_or(ApiError::NotFound)?;
    let ty = state
        .repo
        .types()
        .by_slug(site.id, &type_slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    if body.type_id != ty.id {
        return Err(ApiError::BadRequest("type_id does not match URL type slug".into()));
    }
    body.validate(&ty)?;
    Ok(Json(state.repo.content().create(site.id, body).await?))
}
