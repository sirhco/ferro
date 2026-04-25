//! Server-Sent Events live-preview endpoint.
//!
//! Subscribers receive every `HookEvent` as it fires through the registry.
//! Optional `?type=<slug>` query param filters to a single content type.
//! Browser clients should use the standard `EventSource` API; server-to-server
//! clients can stream the body directly.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use ferro_plugin::HookEvent;
use futures::stream::{Stream, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    /// Restrict to events touching a specific content-type slug. Currently
    /// matches `ContentCreated`/`Updated`/`Published`/`Deleted`/`TypeMigrated`
    /// against the type's slug. Other event kinds are dropped when set.
    #[serde(rename = "type")]
    pub type_slug: Option<String>,

    /// Optional bearer token query param. Browser EventSource cannot set
    /// custom headers, so callers may pass the token here instead. Standard
    /// `Authorization: Bearer <jwt>` header is also honored.
    pub token: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/events", get(events))
}

async fn events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Auth: prefer Authorization header; fall back to ?token=. We need the
    // resolved `AuthUser` so each event can be filtered by RBAC, not just
    // gate the connection.
    let auth = match AuthUser::try_from_headers(&state, &headers).await? {
        Some(a) => a,
        None => {
            let token = params.token.as_deref().ok_or(ApiError::Unauthorized)?;
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
            // Resolve roles to build an AuthContext for per-event RBAC.
            let mut roles = Vec::with_capacity(user.roles.len());
            for id in &user.roles {
                if let Some(r) = state.repo.users().get_role(*id).await? {
                    roles.push(r);
                }
            }
            AuthUser {
                user: user.clone(),
                claims,
                ctx: ferro_auth::AuthContext { user_id: user.id, roles },
            }
        }
    };

    let receiver = state.hooks.subscribe();
    let want_type = params.type_slug;
    let auth_ctx = auth.ctx.clone();

    let stream = BroadcastStream::new(receiver)
        .filter_map(move |result| {
            let want = want_type.clone();
            let auth_ctx = auth_ctx.clone();
            async move {
                let evt = result.ok()?;
                if !event_matches(&evt, want.as_deref()) {
                    return None;
                }
                if !crate::graphql::user_can_read_event(&auth_ctx, &evt) {
                    return None;
                }
                Some(evt)
            }
        })
        .map(|evt| {
            let payload = serde_json::to_string(&evt).unwrap_or_default();
            Ok::<_, Infallible>(Event::default().event(event_kind(&evt)).data(payload))
        });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn event_kind(evt: &HookEvent) -> &'static str {
    // `HookEvent` is `#[non_exhaustive]`; new variants added in ferro-plugin
    // surface here as `event` until we wire a discriminant.
    match evt {
        HookEvent::ContentCreated { .. } => "content.created",
        HookEvent::ContentUpdated { .. } => "content.updated",
        HookEvent::ContentPublished { .. } => "content.published",
        HookEvent::ContentDeleted { .. } => "content.deleted",
        HookEvent::TypeMigrated { .. } => "type.migrated",
        _ => "event",
    }
}

/// Decide whether `evt` should reach a subscriber filtered by `type_slug`.
/// Without a filter, every event passes through. With a filter set, only
/// events whose `type_slug` matches are forwarded; events that don't carry
/// a slug (a small minority) are dropped to keep the contract crisp.
fn event_matches(evt: &HookEvent, type_slug: Option<&str>) -> bool {
    match type_slug {
        None => true,
        Some(want) => evt.type_slug() == Some(want),
    }
}
