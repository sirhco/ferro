use std::{env, time::Duration};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use starter_site_app::{shell, App};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let api_base = env::var("FERRO_API_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let seo_root: std::path::PathBuf =
        env::var("FERRO_SEO_ROOT").unwrap_or_else(|_| "plugins/seo/data".into()).into();

    let conf = get_configuration(None).expect("leptos config: run via cargo-leptos");
    let leptos_options = conf.leptos_options;
    let listen = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let api = starter_site_app::data::ApiClient::new(api_base.clone());
    let seo = starter_site_app::seo::SeoLoader::new(seo_root);

    let state = SiteState { api_base: api_base.clone(), options: leptos_options.clone() };

    let pkg_dir = std::path::Path::new(leptos_options.site_root.as_ref())
        .join(leptos_options.site_pkg_dir.as_ref());
    let pkg_serve = ServeDir::new(&pkg_dir).precompressed_br().precompressed_gzip();

    let app = Router::new()
        .nest_service(
            "/pkg",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
                ))
                .service(pkg_serve),
        )
        .route("/media/{*path}", any(proxy_media))
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let api = api.clone();
                let seo = seo.clone();
                move || {
                    provide_context(api.clone());
                    provide_context(seo.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler::<SiteState, _>(shell))
        .with_state(state);

    tracing::info!(addr = %listen, api_base, "starter-site listening");
    let listener = tokio::net::TcpListener::bind(&listen).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

#[derive(Clone)]
struct SiteState {
    api_base: String,
    options: LeptosOptions,
}

impl axum::extract::FromRef<SiteState> for LeptosOptions {
    fn from_ref(s: &SiteState) -> Self {
        s.options.clone()
    }
}

async fn proxy_media(
    State(state): State<SiteState>,
    uri: Uri,
    req: Request<Body>,
) -> impl IntoResponse {
    let path = uri.path();
    let upstream = format!("{}{}", state.api_base.trim_end_matches('/'), path);
    let client =
        reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("reqwest");
    let mut builder = client.request(req.method().clone(), &upstream);
    for (k, v) in req.headers().iter() {
        if k == "host" {
            continue;
        }
        builder = builder.header(k, v);
    }
    let resp = match builder.send().await {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_GATEWAY, "media upstream").into_response(),
    };
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.bytes().await.unwrap_or_default();
    let mut out = Response::new(Body::from(bytes));
    *out.status_mut() = status;
    for (k, v) in headers.iter() {
        out.headers_mut().insert(k, v.clone());
    }
    out
}
