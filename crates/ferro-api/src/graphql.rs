use std::collections::BTreeMap;
use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql::{Context, EmptySubscription, InputObject, Object, Request, Schema};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use ferro_auth::authorize;
use ferro_core::{
    ContentPatch, ContentType, ContentTypeId, FieldValue, NewContent, Permission, Scope, Site,
};
use ferro_plugin::HookEvent;

use crate::auth::AuthUser;
use crate::state::AppState;

// --- Output nodes ---

pub struct SiteNode(ferro_core::Site);

#[Object]
impl SiteNode {
    async fn id(&self) -> String {
        self.0.id.to_string()
    }
    async fn slug(&self) -> &str {
        &self.0.slug
    }
    async fn name(&self) -> &str {
        &self.0.name
    }
    async fn description(&self) -> Option<&str> {
        self.0.description.as_deref()
    }
    async fn default_locale(&self) -> String {
        self.0.default_locale.to_string()
    }
}

pub struct ContentNode(ferro_core::Content);

#[Object]
impl ContentNode {
    async fn id(&self) -> String {
        self.0.id.to_string()
    }
    async fn slug(&self) -> &str {
        &self.0.slug
    }
    async fn status(&self) -> String {
        format!("{:?}", self.0.status).to_lowercase()
    }
    async fn locale(&self) -> String {
        self.0.locale.to_string()
    }
    async fn data(&self) -> async_graphql::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.0.data)?)
    }
}

pub struct LoginPayload {
    token: String,
    user: ferro_core::User,
}

#[Object]
impl LoginPayload {
    async fn token(&self) -> &str {
        &self.token
    }
    async fn user_id(&self) -> String {
        self.user.id.to_string()
    }
    async fn email(&self) -> &str {
        &self.user.email
    }
    async fn handle(&self) -> &str {
        &self.user.handle
    }
}

// --- Input objects ---

#[derive(InputObject)]
struct NewContentInput {
    type_slug: String,
    slug: String,
    locale: Option<String>,
    /// Field data as a JSON object keyed by field slug.
    data: serde_json::Value,
}

#[derive(InputObject)]
struct ContentPatchInput {
    slug: Option<String>,
    /// Optional partial field data as a JSON object.
    data: Option<serde_json::Value>,
}

// --- Schema ---

pub type FerroSchema = Schema<Query, Mutation, EmptySubscription>;

pub struct Query;

#[Object]
impl Query {
    async fn sites(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<SiteNode>> {
        let state = ctx.data::<Arc<AppState>>()?;
        Ok(state.repo.sites().list().await?.into_iter().map(SiteNode).collect())
    }

    async fn content(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Content type slug")] type_slug: String,
        #[graphql(desc = "Entry slug")] slug: String,
    ) -> async_graphql::Result<Option<ContentNode>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let sites = state.repo.sites().list().await?;
        let Some(site) = sites.first() else {
            return Ok(None);
        };
        let Some(ty) = state.repo.types().by_slug(site.id, &type_slug).await? else {
            return Ok(None);
        };
        Ok(state.repo.content().by_slug(site.id, ty.id, &slug).await?.map(ContentNode))
    }

    async fn me(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<String>> {
        Ok(ctx.data_opt::<AuthUser>().map(|a| a.user.email.clone()))
    }
}

pub struct Mutation;

const JWT_TTL_SECS: i64 = 60 * 60 * 12;

#[Object]
impl Mutation {
    async fn login(
        &self,
        ctx: &Context<'_>,
        email: String,
        password: String,
    ) -> async_graphql::Result<LoginPayload> {
        let state = ctx.data::<Arc<AppState>>()?;
        let (user, _session) = state.auth.login(&email, &password, None, None).await?;
        let roles: Vec<String> = user.roles.iter().map(|r| r.to_string()).collect();
        let token = state.jwt.mint(user.id, roles, JWT_TTL_SECS)?;
        Ok(LoginPayload { token, user })
    }

    async fn create_content(
        &self,
        ctx: &Context<'_>,
        input: NewContentInput,
    ) -> async_graphql::Result<ContentNode> {
        let state = ctx.data::<Arc<AppState>>()?;
        let auth = require_auth(ctx)?;
        let (site, ty) = resolve_type(state, &input.type_slug).await?;
        require_write(auth, ty.id)?;

        let data: BTreeMap<String, FieldValue> = serde_json::from_value(input.data)
            .map_err(|e| async_graphql::Error::new(format!("invalid data: {e}")))?;
        let locale = input
            .locale
            .map(|s| s.parse().map_err(|e: ferro_core::CoreError| e.to_string()))
            .transpose()?
            .unwrap_or_default();
        let new = NewContent {
            type_id: ty.id,
            slug: input.slug,
            locale,
            data,
            author_id: Some(auth.user.id),
        };
        new.validate(&ty)?;
        let created = state.repo.content().create(site.id, new).await?;
        state
            .hooks
            .dispatch(HookEvent::ContentCreated { content: created.clone() })
            .await;
        Ok(ContentNode(created))
    }

    async fn update_content(
        &self,
        ctx: &Context<'_>,
        type_slug: String,
        slug: String,
        patch: ContentPatchInput,
    ) -> async_graphql::Result<ContentNode> {
        let state = ctx.data::<Arc<AppState>>()?;
        let auth = require_auth(ctx)?;
        let (_site, ty, content) = resolve_entry(state, &type_slug, &slug).await?;
        require_write(auth, ty.id)?;

        let data = patch
            .data
            .map(serde_json::from_value::<BTreeMap<String, FieldValue>>)
            .transpose()
            .map_err(|e| async_graphql::Error::new(format!("invalid patch data: {e}")))?;
        let cp = ContentPatch { slug: patch.slug, status: None, data };
        cp.validate(&ty)?;
        let before = content.clone();
        let after = state.repo.content().update(content.id, cp).await?;
        state
            .hooks
            .dispatch(HookEvent::ContentUpdated {
                before: Box::new(before),
                after: Box::new(after.clone()),
            })
            .await;
        Ok(ContentNode(after))
    }

    async fn publish_content(
        &self,
        ctx: &Context<'_>,
        type_slug: String,
        slug: String,
    ) -> async_graphql::Result<ContentNode> {
        let state = ctx.data::<Arc<AppState>>()?;
        let auth = require_auth(ctx)?;
        let (_site, ty, content) = resolve_entry(state, &type_slug, &slug).await?;
        authorize(&auth.ctx, Permission::Publish(Scope::Type { id: ty.id }))
            .map_err(|_| async_graphql::Error::new("publish denied"))?;
        let published = state.repo.content().publish(content.id).await?;
        state
            .hooks
            .dispatch(HookEvent::ContentPublished { content: published.clone() })
            .await;
        Ok(ContentNode(published))
    }

    async fn delete_content(
        &self,
        ctx: &Context<'_>,
        type_slug: String,
        slug: String,
    ) -> async_graphql::Result<bool> {
        let state = ctx.data::<Arc<AppState>>()?;
        let auth = require_auth(ctx)?;
        let (site, ty, content) = resolve_entry(state, &type_slug, &slug).await?;
        require_write(auth, ty.id)?;
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
        Ok(true)
    }
}

fn require_auth<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a AuthUser> {
    ctx.data_opt::<AuthUser>()
        .ok_or_else(|| async_graphql::Error::new("unauthenticated"))
}

fn require_write(auth: &AuthUser, ty: ContentTypeId) -> async_graphql::Result<()> {
    authorize(&auth.ctx, Permission::Write(Scope::Type { id: ty }))
        .map_err(|_| async_graphql::Error::new("write denied"))?;
    Ok(())
}

async fn resolve_type(
    state: &AppState,
    type_slug: &str,
) -> async_graphql::Result<(Site, ContentType)> {
    let sites = state.repo.sites().list().await?;
    let site = sites
        .into_iter()
        .next()
        .ok_or_else(|| async_graphql::Error::new("no site"))?;
    let ty = state
        .repo
        .types()
        .by_slug(site.id, type_slug)
        .await?
        .ok_or_else(|| async_graphql::Error::new("content type not found"))?;
    Ok((site, ty))
}

async fn resolve_entry(
    state: &AppState,
    type_slug: &str,
    slug: &str,
) -> async_graphql::Result<(Site, ContentType, ferro_core::Content)> {
    let (site, ty) = resolve_type(state, type_slug).await?;
    let content = state
        .repo
        .content()
        .by_slug(site.id, ty.id, slug)
        .await?
        .ok_or_else(|| async_graphql::Error::new("content not found"))?;
    Ok((site, ty, content))
}

// --- Router ---

pub fn schema(state: Arc<AppState>) -> FerroSchema {
    Schema::build(Query, Mutation, EmptySubscription).data(state).finish()
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphiql", get(graphiql))
}

async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<Request>,
) -> Json<async_graphql::Response> {
    let auth = match AuthUser::try_from_headers(&state, &headers).await {
        Ok(v) => v,
        Err(e) => {
            // Token present but invalid — surface as GraphQL error rather than
            // silently dropping to anonymous.
            return Json(async_graphql::Response::from_errors(vec![
                async_graphql::ServerError::new(e.to_string(), None),
            ]));
        }
    };
    let req = if let Some(a) = auth { req.data(a) } else { req };
    Json(schema(state).execute(req).await)
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}
