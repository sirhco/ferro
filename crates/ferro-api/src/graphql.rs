use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::Router;
use ferro_core::{Content, Site};

use crate::state::AppState;

pub type FerroSchema = Schema<Query, EmptyMutation, EmptySubscription>;

pub struct Query;

#[Object]
impl Query {
    async fn sites(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Site>> {
        let state = ctx.data::<Arc<AppState>>()?;
        Ok(state.repo.sites().list().await?)
    }

    async fn content(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Content type slug")] type_slug: String,
        #[graphql(desc = "Entry slug")] slug: String,
    ) -> async_graphql::Result<Option<Content>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let sites = state.repo.sites().list().await?;
        let Some(site) = sites.first() else {
            return Ok(None);
        };
        let Some(ty) = state.repo.types().by_slug(site.id, &type_slug).await? else {
            return Ok(None);
        };
        Ok(state.repo.content().by_slug(site.id, ty.id, &slug).await?)
    }
}

// We implement GraphQL output types for core domain via `SimpleObject` in a
// future pass. For v0.1 we expose `Site` and `Content` as `Json` passthroughs
// so the wiring compiles before the field-by-field GraphQL schema lands.
async_graphql::scalar!(Site);
async_graphql::scalar!(Content);

pub fn schema(state: Arc<AppState>) -> FerroSchema {
    Schema::build(Query, EmptyMutation, EmptySubscription).data(state).finish()
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphiql", get(graphiql))
}

async fn graphql_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema(state).execute(req.into_inner()).await.into()
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}
