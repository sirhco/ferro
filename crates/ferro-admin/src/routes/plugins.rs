use leptos::prelude::*;

use crate::routes::layout::Shell;

#[component]
pub fn PluginsPage() -> impl IntoView {
    view! {
        <Shell>
            <h2>"Plugins"</h2>
            <div class="ferro-card">
                <p class="ferro-muted">
                    "Install, configure, and grant capabilities to WASM plugins. \
                     The plugin host MVP loads in-process Rust hooks today; a \
                     wasmtime-backed loader is on the roadmap."
                </p>
                <p class="ferro-muted">
                    "Outbound webhooks can be configured directly in "
                    <code>"ferro.toml"</code>
                    " — see the [[webhooks]] section."
                </p>
            </div>
        </Shell>
    }
}
