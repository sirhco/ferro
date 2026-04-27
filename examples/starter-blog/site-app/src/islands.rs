//! Interactive bits — these ship JS+WASM to the browser. Everything else is
//! server-only.
//!
//! Each `#[island]` becomes a hydration root. Leptos generates a tiny stub
//! script per page that loads only the islands referenced on that page,
//! cutting bundle size dramatically vs. full SPA hydration.

use leptos::prelude::*;

/// Light/dark theme toggle. Persists to localStorage; reads on mount so the
/// default matches OS preference (via prefers-color-scheme CSS var).
#[island]
pub fn ThemeToggle() -> impl IntoView {
    let dark = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(stored) = read_pref() {
            dark.set(stored == "dark");
            apply(stored == "dark");
        }
    });

    let toggle = move |_| {
        let next = !dark.get();
        dark.set(next);
        apply(next);
        write_pref(if next { "dark" } else { "light" });
    };

    view! {
        <button
            class="theme-toggle"
            type="button"
            on:click=toggle
            title="Toggle theme"
        >
            {move || if dark.get() { "☀" } else { "☾" }}
        </button>
    }
}

#[cfg(feature = "hydrate")]
fn read_pref() -> Option<String> {
    web_sys::window()?.local_storage().ok().flatten()?.get_item("ferro-site-theme").ok().flatten()
}

#[cfg(not(feature = "hydrate"))]
fn read_pref() -> Option<String> {
    None
}

#[cfg(feature = "hydrate")]
fn write_pref(v: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item("ferro-site-theme", v);
    }
}

#[cfg(not(feature = "hydrate"))]
fn write_pref(_: &str) {}

#[cfg(feature = "hydrate")]
fn apply(dark: bool) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(html) = doc.document_element() {
            let _ = html.set_attribute("data-theme", if dark { "dark" } else { "light" });
        }
    }
}

#[cfg(not(feature = "hydrate"))]
fn apply(_: bool) {}

/// Search box that filters the visible content list as the user types.
/// Pure client-side filter — operates on already-rendered cards via DOM
/// classList toggling. No server round-trip per keystroke.
#[island]
pub fn SearchFilter(#[prop(optional)] placeholder: Option<String>) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_else(|| "Filter…".into());
    let on_input = move |ev: leptos::ev::Event| {
        let q = event_target_value(&ev).to_lowercase();
        filter_cards(&q);
    };

    view! {
        <input
            type="search"
            class="search-filter"
            placeholder=placeholder
            on:input=on_input
        />
    }
}

#[cfg(feature = "hydrate")]
fn filter_cards(query: &str) {
    use wasm_bindgen::JsCast;
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(nodes) = doc.query_selector_all("[data-searchable]") else {
        return;
    };
    for i in 0..nodes.length() {
        let Some(node) = nodes.item(i) else { continue };
        let Ok(el) = node.dyn_into::<web_sys::HtmlElement>() else {
            continue;
        };
        let text = el.text_content().unwrap_or_default().to_lowercase();
        let matches = query.is_empty() || text.contains(query);
        let _ = el.style().set_property("display", if matches { "" } else { "none" });
    }
}

#[cfg(not(feature = "hydrate"))]
fn filter_cards(_: &str) {}
