mod data;
mod render;
mod views;

use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use leptos_meta::*;

use crate::data::ApiClient;
use crate::views::App;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let api_base = env::var("FERRO_API_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let listen: SocketAddr = env::var("STARTER_SITE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3001".into())
        .parse()
        .expect("STARTER_SITE_ADDR");

    let leptos_options = LeptosOptions::builder()
        .output_name("starter-site")
        .site_root("target/site".to_string())
        .site_pkg_dir("pkg".to_string())
        .build();
    let routes = generate_route_list(App);

    let api = ApiClient::new(api_base.clone());

    let state = SiteState {
        api_base: api_base.clone(),
        options: leptos_options.clone(),
    };

    let app = Router::new()
        .route("/media/{*path}", any(proxy_media))
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let api = api.clone();
                move || {
                    provide_context(api.clone());
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
    let listener = tokio::net::TcpListener::bind(listen).await.expect("bind");
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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest");
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

fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
                <title>"Ferro Demo"</title>
                <style>{include_str!("../style/site.css")}</style>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}
