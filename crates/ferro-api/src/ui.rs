//! Operator-facing landing page. The admin UI is the Leptos SSR app in
//! `ferro-admin`, mounted by the CLI at `/admin/*`.

use axum::response::Html;
use axum::routing::get;
use axum::Router;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/", get(landing))
}

async fn landing() -> Html<&'static str> {
    Html(LANDING_HTML)
}

const LANDING_HTML: &str = include_str!("ui/landing.html");
