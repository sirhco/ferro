use std::collections::BTreeMap;
use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql::{Context, InputObject, Object, Schema, Subscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::Router;
use futures::stream::{Stream, StreamExt as _};
use tokio_stream::wrappers::BroadcastStream;
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

pub type FerroSchema = Schema<Query, Mutation, SubscriptionRoot>;

/// Flat event node for GraphQL subscriptions. Each variant of [`HookEvent`]
/// surfaces as a single `ContentEventNode` with `kind` discriminating and the
/// remaining fields populated when applicable.
pub struct ContentEventNode {
    kind: &'static str,
    content_id: Option<String>,
    type_id: Option<String>,
    type_slug: Option<String>,
    site_id: Option<String>,
    slug: Option<String>,
    status: Option<String>,
    rows_migrated: Option<u64>,
}

#[Object]
impl ContentEventNode {
    async fn kind(&self) -> &str {
        self.kind
    }
    async fn content_id(&self) -> Option<&str> {
        self.content_id.as_deref()
    }
    async fn type_id(&self) -> Option<&str> {
        self.type_id.as_deref()
    }
    async fn type_slug(&self) -> Option<&str> {
        self.type_slug.as_deref()
    }
    async fn site_id(&self) -> Option<&str> {
        self.site_id.as_deref()
    }
    async fn slug(&self) -> Option<&str> {
        self.slug.as_deref()
    }
    async fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }
    async fn rows_migrated(&self) -> Option<u64> {
        self.rows_migrated
    }
}

impl From<&HookEvent> for ContentEventNode {
    fn from(evt: &HookEvent) -> Self {
        match evt {
            HookEvent::ContentCreated { content, type_slug } => Self {
                kind: "content.created",
                content_id: Some(content.id.to_string()),
                type_id: Some(content.type_id.to_string()),
                type_slug: type_slug.clone(),
                site_id: Some(content.site_id.to_string()),
                slug: Some(content.slug.clone()),
                status: Some(format!("{:?}", content.status).to_lowercase()),
                rows_migrated: None,
            },
            HookEvent::ContentUpdated { after, type_slug, .. } => Self {
                kind: "content.updated",
                content_id: Some(after.id.to_string()),
                type_id: Some(after.type_id.to_string()),
                type_slug: type_slug.clone(),
                site_id: Some(after.site_id.to_string()),
                slug: Some(after.slug.clone()),
                status: Some(format!("{:?}", after.status).to_lowercase()),
                rows_migrated: None,
            },
            HookEvent::ContentPublished { content, type_slug } => Self {
                kind: "content.published",
                content_id: Some(content.id.to_string()),
                type_id: Some(content.type_id.to_string()),
                type_slug: type_slug.clone(),
                site_id: Some(content.site_id.to_string()),
                slug: Some(content.slug.clone()),
                status: Some(format!("{:?}", content.status).to_lowercase()),
                rows_migrated: None,
            },
            HookEvent::ContentDeleted {
                site_id,
                type_id,
                content_id,
                slug,
                type_slug,
            } => Self {
                kind: "content.deleted",
                content_id: Some(content_id.to_string()),
                type_id: Some(type_id.to_string()),
                type_slug: type_slug.clone(),
                site_id: Some(site_id.to_string()),
                slug: Some(slug.clone()),
                status: None,
                rows_migrated: None,
            },
            HookEvent::TypeMigrated {
                site_id,
                type_id,
                type_slug,
                rows_migrated,
                ..
            } => Self {
                kind: "type.migrated",
                content_id: None,
                type_id: Some(type_id.to_string()),
                type_slug: type_slug.clone(),
                site_id: Some(site_id.to_string()),
                slug: None,
                status: None,
                rows_migrated: Some(*rows_migrated),
            },
            _ => Self {
                kind: "event",
                content_id: None,
                type_id: None,
                type_slug: None,
                site_id: None,
                slug: None,
                status: None,
                rows_migrated: None,
            },
        }
    }
}

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Live stream of content + schema events.
    ///
    /// Auth comes from `connection_init` (see [`subscription_handler`]); each
    /// emitted event is gated against the caller's `AuthContext` so a
    /// subscriber only observes events on content types they can `Read`.
    /// Lagged receivers skip ahead silently.
    async fn content_changes<'ctx>(
        &self,
        ctx: &'ctx Context<'_>,
    ) -> async_graphql::Result<impl Stream<Item = ContentEventNode> + 'ctx> {
        let state = ctx.data::<Arc<AppState>>()?;
        let auth = ctx
            .data_opt::<AuthUser>()
            .ok_or_else(|| async_graphql::Error::new("unauthenticated"))?;
        let auth_ctx = auth.ctx.clone();
        let rx = state.hooks.subscribe();
        Ok(BroadcastStream::new(rx).filter_map(move |res| {
            let auth_ctx = auth_ctx.clone();
            async move {
                let evt = res.ok()?;
                if user_can_read_event(&auth_ctx, &evt) {
                    Some(ContentEventNode::from(&evt))
                } else {
                    None
                }
            }
        }))
    }
}

/// Whether `ctx` has `Read` permission on the content type referenced by
/// `evt`. `TypeMigrated` requires `ManageSchema` instead of `Read`.
pub(crate) fn user_can_read_event(ctx: &ferro_auth::AuthContext, evt: &HookEvent) -> bool {
    let type_id = match evt {
        HookEvent::ContentCreated { content, .. }
        | HookEvent::ContentPublished { content, .. } => content.type_id,
        HookEvent::ContentUpdated { after, .. } => after.type_id,
        HookEvent::ContentDeleted { type_id, .. } => *type_id,
        HookEvent::TypeMigrated { .. } => {
            return ferro_auth::authorize(ctx, Permission::ManageSchema).is_ok();
        }
        _ => return false,
    };
    ferro_auth::authorize(ctx, Permission::Read(Scope::Type { id: type_id })).is_ok()
}

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
            .dispatch(HookEvent::ContentCreated {
                content: created.clone(),
                type_slug: Some(ty.slug.clone()),
            })
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
                type_slug: Some(ty.slug.clone()),
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
            .dispatch(HookEvent::ContentPublished {
                content: published.clone(),
                type_slug: Some(ty.slug.clone()),
            })
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
                type_slug: Some(ty.slug.clone()),
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
    Schema::build(Query, Mutation, SubscriptionRoot).data(state).finish()
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphiql", get(graphiql))
        .route("/graphql/ws", get(subscription_handler))
}

/// WebSocket entrypoint for GraphQL subscriptions.
///
/// Auth: the standard `graphql-ws` / `graphql-transport-ws` `connection_init`
/// payload must include a JWT under `token` (we also accept `authToken` and
/// `Authorization: Bearer ...` for client compatibility). Connections without
/// a valid token are rejected before any subscribe message is processed.
///
/// Each connection rebuilds the schema so its broadcast receiver is fresh.
async fn subscription_handler(
    State(state): State<Arc<AppState>>,
    ws: axum::extract::WebSocketUpgrade,
    proto: async_graphql_axum::GraphQLProtocol,
) -> impl IntoResponse {
    let schema = schema(state.clone());
    ws.protocols(async_graphql::http::ALL_WEBSOCKET_PROTOCOLS).on_upgrade(move |socket| {
        let state = state.clone();
        async move {
            async_graphql_axum::GraphQLWebSocket::new(socket, schema, proto)
                .on_connection_init(move |payload| {
                    let state = state.clone();
                    async move {
                        let auth = authenticate_init(&state, &payload).await?;
                        let mut data = async_graphql::Data::default();
                        data.insert(auth);
                        Ok(data)
                    }
                })
                .serve()
                .await
        }
    })
}

/// Pull a JWT out of the `connection_init` payload and resolve it to an
/// [`AuthUser`]. Looks at `token`, `authToken`, and `Authorization: Bearer ...`
/// in that order to match common client conventions.
async fn authenticate_init(
    state: &AppState,
    payload: &serde_json::Value,
) -> async_graphql::Result<AuthUser> {
    let token = extract_token(payload).ok_or_else(|| async_graphql::Error::new("missing token"))?;
    let claims = state
        .jwt
        .verify(&token)
        .map_err(|_| async_graphql::Error::new("invalid token"))?;
    let user_id = claims
        .user_id()
        .map_err(|_| async_graphql::Error::new("invalid token"))?;
    let user = state
        .repo
        .users()
        .get(user_id)
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?
        .ok_or_else(|| async_graphql::Error::new("unknown user"))?;
    if !user.active {
        return Err(async_graphql::Error::new("account disabled"));
    }
    let mut roles = Vec::with_capacity(user.roles.len());
    for id in &user.roles {
        if let Some(r) = state
            .repo
            .users()
            .get_role(*id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
        {
            roles.push(r);
        }
    }
    let ctx = ferro_auth::AuthContext { user_id: user.id, roles };
    Ok(AuthUser { user, claims, ctx })
}

fn extract_token(payload: &serde_json::Value) -> Option<String> {
    if let Some(t) = payload.get("token").and_then(|v| v.as_str()) {
        return Some(t.to_string());
    }
    if let Some(t) = payload.get("authToken").and_then(|v| v.as_str()) {
        return Some(t.to_string());
    }
    if let Some(auth) = payload.get("Authorization").and_then(|v| v.as_str()) {
        if let Some(rest) = auth.strip_prefix("Bearer ").or_else(|| auth.strip_prefix("bearer ")) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let auth = match AuthUser::try_from_headers(&state, &headers).await {
        Ok(v) => v,
        Err(e) => {
            // Token present but invalid — surface as GraphQL error rather than
            // silently dropping to anonymous.
            return async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
                e.to_string(),
                None,
            )])
            .into();
        }
    };
    let mut req = req.into_inner();
    if let Some(a) = auth {
        req = req.data(a);
    }
    schema(state).execute(req).await.into()
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}
