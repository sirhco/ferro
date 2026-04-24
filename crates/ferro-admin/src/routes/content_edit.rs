use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[component]
pub fn ContentEdit() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let type_slug = move || params.read().get("type_slug").unwrap_or_default();

    view! {
        <section class="ferro-content-edit">
            <h1>{move || format!("Editing {} / {}", type_slug(), id())}</h1>
            <p>"Field editor components live in the ferro-editor crate and hydrate as islands."</p>
        </section>
    }
}
