//! Public starter-blog site (Leptos islands).
//!
//! Routes are server-rendered. Only `#[island]` components ship JS+WASM to
//! the browser (see `islands.rs`). cargo-leptos `--split` splits the WASM
//! bundle per route so initial page load only fetches what's needed; brotli
//! compression is applied post-build by `ferro build`.

#![deny(rust_2018_idioms)]

pub mod islands;
pub mod render;
pub mod views;

#[cfg(feature = "ssr")]
pub mod data;
#[cfg(feature = "ssr")]
pub mod seo;

use leptos::prelude::*;
use leptos_meta::{HashedStylesheet, MetaTags};
pub use views::App;

/// HTML shell. SSR emits this; the cargo-leptos hydration script tag
/// auto-loads the per-island JS chunks. With islands mode, only marked
/// components hydrate — most of the page stays static HTML.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() islands=true />
                <HashedStylesheet options id="leptos" />
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
