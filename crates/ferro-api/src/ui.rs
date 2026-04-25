//! Operator-facing HTML routes: landing page + minimal admin SPA.
//!
//! The proper Leptos admin app lives in `ferro-admin` but its SSR wiring is
//! still on the roadmap (see `ferro-cli/src/serve.rs`). Until that lands we
//! ship a self-contained vanilla-JS SPA so operators have a working UI from
//! `ferro init` → `ferro serve` without extra build steps.

use axum::response::Html;
use axum::routing::get;
use axum::Router;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(landing))
        .route("/admin", get(admin))
        .route("/admin/", get(admin))
}

async fn landing() -> Html<&'static str> {
    Html(LANDING_HTML)
}

async fn admin() -> Html<&'static str> {
    Html(ADMIN_HTML)
}

const LANDING_HTML: &str = include_str!("ui/landing.html");
const ADMIN_HTML: &str = include_str!("ui/admin.html");
