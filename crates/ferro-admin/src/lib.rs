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

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
