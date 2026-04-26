use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde_json::Value;

use crate::routes::layout::Shell;
use crate::state::{AdminState, TypeSummary};

const FIELD_PRESETS: &[(&str, &str)] = &[
    ("text", "Text"),
    ("richtext", "Rich text (Markdown)"),
    ("number", "Number"),
    ("boolean", "Boolean"),
    ("date", "Date"),
];

fn build_preset(kind: &str, slug: &str, name: &str) -> Value {
    let kind_obj = match kind {
        "text" => serde_json::json!({ "type": "text", "multiline": false }),
        "richtext" => serde_json::json!({ "type": "rich_text", "format": "markdown" }),
        "number" => serde_json::json!({ "type": "number", "int": false }),
        "boolean" => serde_json::json!({ "type": "boolean" }),
        "date" => serde_json::json!({ "type": "date" }),
        _ => serde_json::json!({ "type": "text" }),
    };
    serde_json::json!({
        "id": new_field_id(),
        "slug": slug,
        "name": name,
        "kind": kind_obj,
        "required": false,
        "localized": false,
        "unique": false,
        "hidden": false,
    })
}

fn new_field_id() -> String {
    // Crockford-shaped 26-char id; backend's `FieldId::from_str` accepts it.
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    #[cfg(feature = "hydrate")]
    {
        let mut out = String::with_capacity(26);
        for _ in 0..26 {
            let r = (js_sys::Math::random() * ALPHABET.len() as f64) as usize;
            out.push(ALPHABET[r.min(ALPHABET.len() - 1)] as char);
        }
        out
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = ALPHABET;
        String::from("00000000000000000000000000")
    }
}

#[component]
pub fn SchemaList() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let types = state.types;

    let refresh = move || {
        let st = state;
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(t) = crate::api::get::<Vec<TypeSummary>>("/api/v1/types").await {
                    st.types.set(t);
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = st;
        }
    };

    let delete = move |slug: String| {
        let st = state;
        let refresh = refresh.clone();
        #[cfg(feature = "hydrate")]
        {
            let confirm_msg = format!(
                "Delete type {slug}? Existing content stays but is orphaned."
            );
            let win = web_sys::window();
            let ok = win
                .as_ref()
                .and_then(|w| w.confirm_with_message(&confirm_msg).ok())
                .unwrap_or(false);
            if !ok {
                return;
            }
            let s = slug.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/types/{}", encode(&s));
                match crate::api::delete::<Value>(&path).await {
                    Ok(_) => {
                        st.set_toast_ok(format!("Deleted {s}."));
                        refresh();
                    }
                    Err(e) => st.set_toast_err(e.message()),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (slug, st, refresh);
        }
    };

    view! {
        <Shell>
            <h2>"Content Types"</h2>
            <div class="ferro-row" style="margin-bottom: 1rem;">
                <a class="ferro-primary" href="/admin/schema/new">"New type"</a>
            </div>
            <div class="ferro-card">
                {move || {
                    let list = types.get();
                    if list.is_empty() {
                        return view! { <p class="ferro-muted">"No types defined yet."</p> }.into_any();
                    }
                    view! {
                        <table>
                            <thead>
                                <tr>
                                    <th>"Name"</th>
                                    <th>"Slug"</th>
                                    <th>"Fields"</th>
                                    <th></th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each=move || types.get()
                                     key=|t| t.id.clone()
                                     let:t>
                                    {{
                                        let edit_href = format!("/admin/schema/edit/{}", t.slug);
                                        let fields = t.fields.iter()
                                            .filter_map(|f| f.get("slug").and_then(|s| s.as_str()).map(String::from))
                                            .collect::<Vec<_>>().join(", ");
                                        let delete = delete.clone();
                                        let slug = t.slug.clone();
                                        view! {
                                            <tr>
                                                <td>{t.name.clone()}</td>
                                                <td><code>{t.slug.clone()}</code></td>
                                                <td>{fields}</td>
                                                <td>
                                                    <a class="ferro-ghost" href=edit_href>"Edit"</a>
                                                    " "
                                                    <button class="ferro-danger"
                                                        on:click=move |_| delete(slug.clone())>
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
            <p class="ferro-muted">
                "Editing a type runs the schema migrator across existing content. "
                "Field shape: see "
                <a href="/api/docs" target="_blank">"API docs"</a>
                " for the full "
                <code>"FieldDef"</code>
                " JSON contract."
            </p>
        </Shell>
    }
}

#[component]
pub fn SchemaEdit() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let params = use_params_map();
    let edit_slug = Memo::new(move |_| crate::util::param(&params.read(), "slug"));
    let is_new = Memo::new(move |_| edit_slug.get().is_empty());

    let slug_in = RwSignal::new(String::new());
    let name_in = RwSignal::new(String::new());
    let desc_in = RwSignal::new(String::new());
    let fields_text = RwSignal::new(String::from("[]"));
    let preset_kind = RwSignal::new(String::from("text"));
    let preset_slug = RwSignal::new(String::new());
    let preset_name = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let existing = RwSignal::new(None::<Value>);

    Effect::new(move |_| {
        let s = edit_slug.get();
        if s.is_empty() {
            return;
        }
        if let Some(t) = state.types.read().iter().find(|t| t.slug == s).cloned() {
            slug_in.set(t.slug.clone());
            name_in.set(t.name.clone());
            desc_in.set(t.description.clone().unwrap_or_default());
            fields_text.set(serde_json::to_string_pretty(&t.fields).unwrap_or_default());
            existing.set(Some(serde_json::to_value(&t).unwrap_or(Value::Null)));
        }
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/types/{}", encode(&s));
                if let Ok(v) = crate::api::get::<Value>(&path).await {
                    if let Some(name) = v.get("name").and_then(|x| x.as_str()) {
                        name_in.set(name.into());
                    }
                    if let Some(slug) = v.get("slug").and_then(|x| x.as_str()) {
                        slug_in.set(slug.into());
                    }
                    if let Some(desc) = v.get("description").and_then(|x| x.as_str()) {
                        desc_in.set(desc.into());
                    }
                    if let Some(fields) = v.get("fields") {
                        fields_text.set(serde_json::to_string_pretty(fields).unwrap_or_default());
                    }
                    existing.set(Some(v));
                }
            });
        }
    });

    let add_field = move |_| {
        let kind = preset_kind.get();
        let slug = preset_slug.get();
        let name = preset_name.get();
        if slug.is_empty() || name.is_empty() {
            state.set_toast_err("Field slug + name required.");
            return;
        }
        let mut arr: Vec<Value> = match serde_json::from_str(&fields_text.get()) {
            Ok(a) => a,
            Err(_) => {
                state.set_toast_err("Fields JSON invalid; fix it before adding.");
                return;
            }
        };
        arr.push(build_preset(&kind, &slug, &name));
        fields_text.set(serde_json::to_string_pretty(&arr).unwrap_or_default());
        preset_slug.set(String::new());
        preset_name.set(String::new());
    };

    let on_save = move |_| {
        error.set(String::new());
        let fields: Result<Vec<Value>, _> = serde_json::from_str(&fields_text.get());
        let Ok(fields) = fields else {
            error.set("Fields JSON invalid.".into());
            return;
        };
        let now = current_iso();
        let st = state;
        let new_flag = is_new.get();
        let cur_slug = edit_slug.get();
        let slug_v = slug_in.get();
        let name_v = name_in.get();
        let desc_opt = {
            let d = desc_in.get();
            if d.is_empty() { None } else { Some(d) }
        };
        let existing_val = existing.get();
        busy.set(true);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let body = if new_flag {
                    let site_id = st
                        .types
                        .read()
                        .first()
                        .map(|t| t.site_id.clone())
                        .unwrap_or_default();
                    serde_json::json!({
                        "id": new_field_id(),
                        "site_id": site_id,
                        "slug": slug_v,
                        "name": name_v,
                        "description": desc_opt,
                        "fields": fields,
                        "singleton": false,
                        "title_field": null,
                        "slug_field": null,
                        "created_at": now,
                        "updated_at": now,
                    })
                } else {
                    let mut base = existing_val.unwrap_or_else(|| serde_json::json!({}));
                    if let Value::Object(ref mut m) = base {
                        m.insert("slug".into(), Value::String(slug_v));
                        m.insert("name".into(), Value::String(name_v));
                        m.insert(
                            "description".into(),
                            match desc_opt {
                                Some(d) => Value::String(d),
                                None => Value::Null,
                            },
                        );
                        m.insert("fields".into(), Value::Array(fields));
                        m.insert("updated_at".into(), Value::String(now));
                    }
                    base
                };
                let res = if new_flag {
                    crate::api::post::<Value, _>("/api/v1/types", &body).await
                } else {
                    let path = format!("/api/v1/types/{}", encode(&cur_slug));
                    crate::api::patch::<Value, _>(&path, &body).await
                };
                match res {
                    Ok(resp) => {
                        if !new_flag {
                            let migrated = resp.get("rows_migrated").and_then(|v| v.as_u64()).unwrap_or(0);
                            let slug = body.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                            st.set_toast_ok(format!(
                                "Saved {slug} · {migrated} row(s) migrated."
                            ));
                        } else {
                            let slug = body.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                            st.set_toast_ok(format!("Created {slug}."));
                        }
                        if let Ok(t) = crate::api::get::<Vec<TypeSummary>>("/api/v1/types").await {
                            st.types.set(t);
                        }
                        crate::util::navigate_to("/admin/schema");
                    }
                    Err(e) => error.set(e.message()),
                }
                busy.set(false);
            });
        }
    };

    view! {
        <Shell>
            <h2>{move || if is_new.get() {
                "New content type".to_string()
            } else {
                format!("Edit · {}", edit_slug.get())
            }}</h2>
            <div class="ferro-card">
                <label>
                    <span>"Slug"</span>
                    <input type="text" placeholder="post" bind:value=slug_in />
                </label>
                <label>
                    <span>"Name"</span>
                    <input type="text" placeholder="Post" bind:value=name_in />
                </label>
                <label>
                    <span>"Description (optional)"</span>
                    <input type="text" bind:value=desc_in />
                </label>
                <h3>"Fields"</h3>
                <p class="ferro-muted">"JSON array of FieldDef. Use the quick-add below for common types."</p>
                <textarea bind:value=fields_text />
                <div class="ferro-row" style="gap: .5rem; margin-top: .5rem;">
                    <select bind:value=preset_kind>
                        <For each=move || FIELD_PRESETS.iter().copied()
                             key=|(id, _)| id.to_string()
                             let:p>
                            <option value=p.0>{p.1}</option>
                        </For>
                    </select>
                    <input type="text" placeholder="field slug" bind:value=preset_slug />
                    <input type="text" placeholder="Field name" bind:value=preset_name />
                    <button class="ferro-ghost" on:click=add_field>"Add field"</button>
                </div>
                <p class="ferro-error">{move || error.get()}</p>
                <div class="ferro-row" style="gap: .5rem; margin-top: 1rem;">
                    <button class="ferro-primary" on:click=on_save disabled=move || busy.get()>
                        {move || if is_new.get() { "Create" } else { "Save" }}
                    </button>
                    <a class="ferro-ghost" href="/admin/schema">"Cancel"</a>
                </div>
            </div>
        </Shell>
    }
}

fn current_iso() -> String {
    #[cfg(feature = "hydrate")]
    {
        let date = js_sys::Date::new_0();
        date.to_iso_string().as_string().unwrap_or_default()
    }
    #[cfg(not(feature = "hydrate"))]
    {
        String::new()
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
