use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Request, Schema};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::state::AppState;

/// Thin wrappers that let us expose domain types through async-graphql without
/// touching `ferro-core`. Each wrapper exposes only the fields the admin /
/// preview flow currently needs; richer GraphQL shapes land with v0.3.
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

pub type FerroSchema = Schema<Query, EmptyMutation, EmptySubscription>;

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
}

pub fn schema(state: Arc<AppState>) -> FerroSchema {
    Schema::build(Query, EmptyMutation, EmptySubscription).data(state).finish()
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphiql", get(graphiql))
}

async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Request>,
) -> Json<async_graphql::Response> {
    Json(schema(state).execute(req).await)
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}
