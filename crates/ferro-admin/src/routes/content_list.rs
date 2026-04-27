use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_navigate, use_params_map},
};
use serde_json::Value;

use crate::{routes::layout::Shell, state::AdminState};

#[component]
pub fn ContentList() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let types = state.types;
    let params = use_params_map();

    let selected = RwSignal::new(String::new());
    Effect::new(move |_| {
        let url_slug = crate::util::param(&params.read(), "type_slug");
        if !url_slug.is_empty() {
            selected.set(url_slug);
            return;
        }
        if let Some(first) = types.read().first() {
            selected.set(first.slug.clone());
        }
    });

    let items = RwSignal::new(Vec::<Value>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    let load: Callback<String> = Callback::new(move |slug: String| {
        if slug.is_empty() {
            return;
        }
        busy.set(true);
        error.set(String::new());
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/content/{}?per_page=200", encode(&slug));
                match crate::api::get::<Value>(&path).await {
                    Ok(page) => {
                        let arr = page
                            .get("items")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        items.set(arr);
                    }
                    Err(e) => error.set(e.message()),
                }
                busy.set(false);
            });
        }
    });

    Effect::new(move |_| {
        let s = selected.get();
        if !s.is_empty() {
            load.run(s);
        }
    });

    view! {
        <Shell>
            <h2>"Content"</h2>
            <TypePicker types=types selected=selected />
            <ContentTable
                state=state
                selected=selected
                items=items
                busy=busy
                error=error
                load=load
            />
        </Shell>
    }
}

#[component]
fn TypePicker(
    types: RwSignal<Vec<crate::state::TypeSummary>>,
    selected: RwSignal<String>,
) -> impl IntoView {
    let on_change = move |ev| {
        let new_slug = event_target_value(&ev);
        let nav = use_navigate();
        nav(&format!("/admin/content/{new_slug}"), Default::default());
    };

    view! {
        {move || {
            if types.read().is_empty() {
                return view! {
                    <p class="ferro-muted">
                        "No content types yet. Create one in "
                        <A href="/admin/schema">"Schema"</A>
                        "."
                    </p>
                }.into_any();
            }
            let cur = selected.get();
            let new_href = format!("/admin/content/{cur}/new");
            view! {
                <div class="ferro-row" style="gap: .75rem; margin-bottom: 1rem;">
                    <label class="ferro-grow">
                        <span>"Type"</span>
                        <select on:change=on_change.clone()>
                            <For each=move || types.get()
                                 key=|t| t.id.clone()
                                 let:t>
                                <option value=t.slug.clone()
                                    selected=move || selected.get() == t.slug>
                                    {t.name.clone()}
                                </option>
                            </For>
                        </select>
                    </label>
                    <a class="ferro-primary" href=new_href>"New entry"</a>
                </div>
            }.into_any()
        }}
    }
}

#[component]
fn ContentTable(
    state: AdminState,
    selected: RwSignal<String>,
    items: RwSignal<Vec<Value>>,
    busy: RwSignal<bool>,
    error: RwSignal<String>,
    load: Callback<String>,
) -> impl IntoView {
    let publish = Callback::new(move |args: (String, String)| {
        let (type_slug, slug) = args;
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let path =
                    format!("/api/v1/content/{}/{}/publish", encode(&type_slug), encode(&slug));
                match crate::api::post_empty::<Value>(&path).await {
                    Ok(_) => {
                        state.set_toast_ok(format!("Published {slug}."));
                        load.run(type_slug);
                    }
                    Err(e) => state.set_toast_err(e.message()),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (type_slug, slug, load);
        }
    });

    let delete = Callback::new(move |args: (String, String)| {
        let (type_slug, slug) = args;
        #[cfg(feature = "hydrate")]
        {
            let confirm_msg = format!("Delete {slug}?");
            let ok = web_sys::window()
                .and_then(|w| w.confirm_with_message(&confirm_msg).ok())
                .unwrap_or(false);
            if !ok {
                return;
            }
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/content/{}/{}", encode(&type_slug), encode(&slug));
                match crate::api::delete::<Value>(&path).await {
                    Ok(_) => {
                        state.set_toast_ok(format!("Deleted {slug}."));
                        load.run(type_slug);
                    }
                    Err(e) => state.set_toast_err(e.message()),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (type_slug, slug, load);
        }
    });

    view! {
        <div class="ferro-card">
            {move || {
                if busy.get() {
                    return view! { <p class="ferro-muted">"Loading…"</p> }.into_any();
                }
                let err = error.get();
                if !err.is_empty() {
                    return view! { <p class="ferro-error">{err}</p> }.into_any();
                }
                if items.read().is_empty() {
                    return view! { <p class="ferro-muted">"No entries yet."</p> }.into_any();
                }
                let cur_type = selected.get();
                view! {
                    <table>
                        <thead>
                            <tr>
                                <th>"Slug"</th>
                                <th>"Status"</th>
                                <th>"Updated"</th>
                                <th></th>
                            </tr>
                        </thead>
                        <tbody>
                            <For each=move || items.get()
                                 key=|c| c.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
                                 let:c>
                                {{
                                    let slug = c.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let status = c.get("status").and_then(|v| v.as_str()).unwrap_or("draft").to_string();
                                    let updated = c.get("updated_at").and_then(|v| v.as_str()).map(crate::util::format_dt).unwrap_or_default();
                                    let cur_t = cur_type.clone();
                                    let edit_href = format!("/admin/content/{cur_t}/edit/{slug}");
                                    let cur_pub = cur_type.clone();
                                    let cur_del = cur_type.clone();
                                    let slug_pub = slug.clone();
                                    let slug_del = slug.clone();
                                    view! {
                                        <tr>
                                            <td><a href=edit_href>{slug.clone()}</a></td>
                                            <td><span class="ferro-pill">{status}</span></td>
                                            <td class="ferro-muted">{updated}</td>
                                            <td>
                                                <button class="ferro-ghost"
                                                    on:click=move |_| publish.run((cur_pub.clone(), slug_pub.clone()))>
                                                    "Publish"
                                                </button>
                                                " "
                                                <button class="ferro-danger"
                                                    on:click=move |_| delete.run((cur_del.clone(), slug_del.clone()))>
                                                    "Delete"
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }}
                            </For>
                        </tbody>
                    </table>
                }.into_any()
            }}
        </div>
    }
}

#[cfg(feature = "hydrate")]
fn encode(s: &str) -> String {
    js_sys::encode_uri_component(s).as_string().unwrap_or_else(|| s.to_string())
}
#[cfg(not(feature = "hydrate"))]
fn encode(s: &str) -> String {
    s.to_string()
}
