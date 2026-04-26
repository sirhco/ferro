//! Ferro admin app.
//!
//! Routes are split via `lazy_route!` + `cargo leptos --split` so each page
//! ships an independent WASM chunk; brotli is applied post-build by the CLI.

#![deny(rust_2018_idioms)]

pub mod api;
pub mod app;
pub mod auth;
pub mod routes;
pub mod state;
pub mod util;

pub use app::{shell, App};

/// Bootstrap entry point invoked by the cargo-leptos hydration script tag.
/// Despite the name, the admin app runs in CSR mode: this just mounts the
/// `App` component into `<body>`. SSR served only the bootstrap shell, so
/// there's nothing to hydrate against.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
