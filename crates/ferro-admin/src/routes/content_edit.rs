use std::collections::HashMap;

use ferro_core::{FieldDef, FieldValue};
use ferro_editor::FieldEditor;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde_json::{Map as JsonMap, Value};

use crate::{
    routes::layout::Shell,
    state::{AdminState, TypeSummary},
};

#[component]
pub fn ContentEdit() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let params = use_params_map();

    let type_slug = Memo::new(move |_| crate::util::param(&params.read(), "type_slug"));
    let entry_slug = Memo::new(move |_| crate::util::param(&params.read(), "slug"));
    let is_new = Memo::new(move |_| entry_slug.get().is_empty());

    let slug_input = RwSignal::new(String::new());
    let form = RwSignal::new(HashMap::<String, FieldValue>::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let versions = RwSignal::new(Vec::<Value>::new());
    let versions_err = RwSignal::new(String::new());

    let resolved_fields = Signal::derive(move || -> Vec<FieldDef> {
        let ts = type_slug.get();
        let summaries = state.types.read();
        let Some(ty) = summaries.iter().find(|t: &&TypeSummary| t.slug == ts) else {
            return Vec::new();
        };
        ty.fields
            .iter()
            .filter_map(|v| serde_json::from_value::<FieldDef>(v.clone()).ok())
            .collect()
    });

    let load = move |ts: String, slug: String| {
        if slug.is_empty() {
            return;
        }
        #[cfg(feature = "hydrate")]
        {
            let ts2 = ts.clone();
            let slug2 = slug.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/content/{}/{}", encode(&ts2), encode(&slug2));
                match crate::api::get::<Value>(&path).await {
                    Ok(c) => {
                        if let Some(s) = c.get("slug").and_then(|v| v.as_str()) {
                            slug_input.set(s.to_string());
                        }
                        let data = c.get("data").cloned().unwrap_or(Value::Null);
                        let mut map = HashMap::<String, FieldValue>::new();
                        if let Value::Object(obj) = data {
                            for (k, v) in obj {
                                if let Ok(fv) = serde_json::from_value::<FieldValue>(v.clone()) {
                                    map.insert(k, fv);
                                } else {
                                    map.insert(k, FieldValue::Object(v));
                                }
                            }
                        }
                        form.set(map);
                    }
                    Err(e) => error.set(e.message()),
                }
                let vp = format!("/api/v1/content/{}/{}/versions", encode(&ts), encode(&slug));
                match crate::api::get::<Vec<Value>>(&vp).await {
                    Ok(vs) => versions.set(vs),
                    Err(e) => versions_err.set(e.message()),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (ts, slug);
        }
    };

    Effect::new(move |_| {
        let ts = type_slug.get();
        let s = entry_slug.get();
        if !s.is_empty() {
            load(ts, s);
        }
    });

    let serialize_form = move || -> Value {
        let mut out = JsonMap::new();
        for (k, v) in form.get().into_iter() {
            let val = serde_json::to_value(&v).unwrap_or(Value::Null);
            out.insert(k, val);
        }
        Value::Object(out)
    };

    let on_save = move |_| {
        error.set(String::new());
        let parsed = serialize_form();
        busy.set(true);
        let st = state;
        let ts = type_slug.get();
        let new_flag = is_new.get();
        let cur_slug = entry_slug.get();
        let new_slug = slug_input.get();
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                if new_flag {
                    let ty_id_locale = expect_context::<AdminState>()
                        .types
                        .read()
                        .iter()
                        .find(|t| t.slug == ts)
                        .map(|t| (t.id.clone(), t.default_locale.clone()));
                    let Some((type_id, locale)) = ty_id_locale else {
                        error.set("Unknown content type".into());
                        busy.set(false);
                        return;
                    };
                    let body = serde_json::json!({
                        "type_id": type_id,
                        "slug": new_slug,
                        "locale": locale.unwrap_or_else(|| "en".into()),
                        "data": parsed,
                    });
                    let path = format!("/api/v1/content/{}", encode(&ts));
                    match crate::api::post::<Value, _>(&path, &body).await {
                        Ok(_) => {
                            st.set_toast_ok("Created.");
                            crate::util::navigate_to(&format!("/admin/content/{ts}"));
                        }
                        Err(e) => error.set(e.message()),
                    }
                } else {
                    let mut patch = JsonMap::new();
                    if !new_slug.is_empty() && new_slug != cur_slug {
                        patch.insert("slug".into(), Value::String(new_slug));
                    }
                    patch.insert("data".into(), parsed);
                    let path = format!("/api/v1/content/{}/{}", encode(&ts), encode(&cur_slug));
                    match crate::api::patch::<Value, _>(&path, &Value::Object(patch)).await {
                        Ok(_) => {
                            st.set_toast_ok("Saved.");
                        }
                        Err(e) => error.set(e.message()),
                    }
                }
                busy.set(false);
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (parsed, st, ts, new_flag, cur_slug, new_slug);
        }
    };

    let on_cancel = move |_| {
        let ts = type_slug.get();
        crate::util::navigate_to(&format!("/admin/content/{ts}"));
    };

    let restore = move |version_id: String, captured: String| {
        let st = state;
        let ts = type_slug.get();
        let slug = entry_slug.get();
        #[cfg(feature = "hydrate")]
        {
            let confirm_msg = format!(
                "Restore version captured at {captured}? Current state will be snapshotted first."
            );
            let win = web_sys::window();
            let ok = win
                .as_ref()
                .and_then(|w| w.confirm_with_message(&confirm_msg).ok())
                .unwrap_or(false);
            if !ok {
                return;
            }
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!(
                    "/api/v1/content/{}/{}/versions/{}/restore",
                    encode(&ts),
                    encode(&slug),
                    encode(&version_id)
                );
                match crate::api::post_empty::<Value>(&path).await {
                    Ok(_) => {
                        st.set_toast_ok("Restored.");
                        crate::util::navigate_to(&format!("/admin/content/{ts}"));
                    }
                    Err(e) => st.set_toast_err(e.message()),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (version_id, captured, st);
        }
    };

    let preview_url = move || {
        let ts = type_slug.get();
        let slug = entry_slug.get();
        if slug.is_empty() {
            String::new()
        } else {
            format!("/preview/{ts}/{slug}")
        }
    };

    // Wire SSE → reload preview iframe whenever a content event for this slug fires.
    #[cfg(feature = "hydrate")]
    {
        Effect::new(move |_| {
            let ts = type_slug.get();
            let slug = entry_slug.get();
            if ts.is_empty() || slug.is_empty() {
                return;
            }
            install_preview_reloader(&ts, &slug);
        });
    }

    view! {
        <Shell>
            <h2>{move || {
                let ts = type_slug.get();
                if is_new.get() {
                    format!("New entry · {ts}")
                } else {
                    format!("Edit · {ts} · {}", entry_slug.get())
                }
            }}</h2>

            {move || {
                if is_new.get() {
                    return view! { <span></span> }.into_any();
                }
                view! {
                    <div class="ferro-card">
                        <h3>"Versions"</h3>
                        {move || {
                            let err = versions_err.get();
                            if !err.is_empty() {
                                return view! { <p class="ferro-error">{err}</p> }.into_any();
                            }
                            if versions.read().is_empty() {
                                return view! { <p class="ferro-muted">"No prior versions yet."</p> }.into_any();
                            }
                            view! {
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Captured"</th>
                                            <th>"Status"</th>
                                            <th>"Slug"</th>
                                            <th></th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <For each=move || versions.get()
                                             key=|v| v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string()
                                             let:v>
                                            {{
                                                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                                let captured = v.get("captured_at").and_then(|x| x.as_str()).map(crate::util::format_dt).unwrap_or_default();
                                                let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("draft").to_string();
                                                let vslug = v.get("slug").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                                let restore = restore.clone();
                                                let captured_for_btn = captured.clone();
                                                view! {
                                                    <tr>
                                                        <td class="ferro-muted">{captured}</td>
                                                        <td><span class="ferro-pill">{status}</span></td>
                                                        <td>{vslug}</td>
                                                        <td>
                                                            <button class="ferro-ghost"
                                                                on:click=move |_| restore(id.clone(), captured_for_btn.clone())>
                                                                "Restore"
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
                }.into_any()
            }}

            <div class="ferro-content-edit-grid">
                <div class="ferro-card ferro-content-edit-form">
                    <label>
                        <span>"Slug"</span>
                        <input type="text" bind:value=slug_input />
                    </label>

                    <For
                        each=move || resolved_fields.get()
                        key=|f| f.slug.clone()
                        let:fdef
                    >
                        {{
                            let slug = fdef.slug.clone();
                            let name = fdef.name.clone();
                            let help = fdef.help.clone();
                            let def_sig = Signal::derive(move || fdef.clone());
                            let slug_for_value = slug.clone();
                            let value_sig = Signal::derive(move || {
                                form.with(|m| m.get(&slug_for_value).cloned().unwrap_or(FieldValue::Null))
                            });
                            let slug_for_change = slug.clone();
                            let on_change = Callback::new(move |v: FieldValue| {
                                let s = slug_for_change.clone();
                                form.update(|m| { m.insert(s, v); });
                            });
                            view! {
                                <label class="ferro-field">
                                    <span class="ferro-field-name">{name}</span>
                                    {help.map(|h| view! { <small class="ferro-muted">{h}</small> })}
                                    <FieldEditor def=def_sig value=value_sig on_change=on_change />
                                </label>
                            }
                        }}
                    </For>

                    <p class="ferro-error">{move || error.get()}</p>
                    <div class="ferro-row" style="gap: .5rem; margin-top: 1rem;">
                        <button class="ferro-primary" on:click=on_save disabled=move || busy.get()>
                            {move || if is_new.get() { "Create" } else { "Save" }}
                        </button>
                        <button class="ferro-ghost" on:click=on_cancel>"Cancel"</button>
                    </div>
                </div>

                {move || {
                    let url = preview_url();
                    if url.is_empty() {
                        view! { <span></span> }.into_any()
                    } else {
                        view! {
                            <div class="ferro-card ferro-content-edit-preview">
                                <div class="ferro-row" style="justify-content: space-between; margin-bottom: .5rem;">
                                    <h3 style="margin:0;">"Preview"</h3>
                                    <a class="ferro-muted" href=url.clone() target="_blank" rel="noopener">"open ↗"</a>
                                </div>
                                <iframe id="ferro-preview-iframe" class="ferro-preview-iframe" src=url />
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </Shell>
    }
}

#[cfg(feature = "hydrate")]
fn install_preview_reloader(type_slug: &str, slug: &str) {
    use wasm_bindgen::{closure::Closure, JsCast};

    let url = format!("/api/v1/events?type={}", encode(type_slug));
    let Ok(es) = web_sys::EventSource::new(&url) else {
        return;
    };
    let slug = slug.to_string();
    let onmessage = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
        let data = evt.data();
        let s = data.as_string().unwrap_or_default();
        if s.contains(&slug) {
            if let Some(win) = web_sys::window() {
                if let Some(doc) = win.document() {
                    if let Some(el) = doc.get_element_by_id("ferro-preview-iframe") {
                        if let Ok(iframe) = el.dyn_into::<web_sys::HtmlIFrameElement>() {
                            if let Some(content_win) = iframe.content_window() {
                                let _ = content_win.location().reload();
                            }
                        }
                    }
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    es.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
}

#[cfg(feature = "hydrate")]
fn encode(s: &str) -> String {
    js_sys::encode_uri_component(s).as_string().unwrap_or_else(|| s.to_string())
}
#[cfg(not(feature = "hydrate"))]
fn encode(s: &str) -> String {
    s.to_string()
}
