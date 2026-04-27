use leptos::prelude::*;
use serde_json::Value;

use crate::{routes::layout::Shell, state::AdminState};

#[component]
pub fn MediaLibrary() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let items = RwSignal::new(Vec::<Value>::new());
    let busy = RwSignal::new(false);
    let load_err = RwSignal::new(String::new());
    let upload_status = RwSignal::new(String::new());
    let upload_err = RwSignal::new(false);
    let alt_text = RwSignal::new(String::new());
    let file_input_ref = NodeRef::<leptos::html::Input>::new();

    let load = move || {
        busy.set(true);
        load_err.set(String::new());
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::get::<Vec<Value>>("/api/v1/media").await {
                    Ok(v) => items.set(v),
                    Err(e) => load_err.set(e.message()),
                }
                busy.set(false);
            });
        }
    };

    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            load();
        }
    });

    let on_upload = move |_| {
        let alt = alt_text.get();
        upload_status.set(String::new());
        upload_err.set(false);
        #[cfg(feature = "hydrate")]
        {
            let Some(input) = file_input_ref.get_untracked() else { return };
            let Some(files) = input.files() else { return };
            let Some(file) = files.get(0) else {
                upload_err.set(true);
                upload_status.set("Pick a file.".into());
                return;
            };
            upload_status.set(format!("Uploading {}…", file.name()));
            wasm_bindgen_futures::spawn_local(async move {
                let alt_opt = if alt.is_empty() { None } else { Some(alt.as_str()) };
                match crate::api::upload_media(file.clone(), alt_opt).await {
                    Ok(_) => {
                        upload_err.set(false);
                        upload_status.set(format!("Uploaded {}.", file.name()));
                        alt_text.set(String::new());
                        if let Some(node) = file_input_ref.get_untracked() {
                            node.set_value("");
                        }
                        load();
                    }
                    Err(e) => {
                        upload_err.set(true);
                        upload_status.set(e.message());
                    }
                }
            });
        }
    };

    let delete = move |id: String, filename: String| {
        let st = state;
        #[cfg(feature = "hydrate")]
        {
            let confirm_msg = format!("Delete {filename}?");
            let win = web_sys::window();
            let ok = win
                .as_ref()
                .and_then(|w| w.confirm_with_message(&confirm_msg).ok())
                .unwrap_or(false);
            if !ok {
                return;
            }
            let load = load.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/media/{id}");
                match crate::api::delete::<Value>(&path).await {
                    Ok(_) => load(),
                    Err(e) => st.set_toast_err(e.message()),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (id, filename, st);
        }
    };

    view! {
        <Shell>
            <h2>"Media"</h2>
            <div class="ferro-card">
                <h3>"Upload"</h3>
                <label>
                    <span>"File"</span>
                    <input type="file" node_ref=file_input_ref />
                </label>
                <label>
                    <span>"Alt text (optional)"</span>
                    <input type="text" placeholder="alt text (images)" bind:value=alt_text />
                </label>
                <p>
                    <button class="ferro-primary" on:click=on_upload>"Upload"</button>
                </p>
                <p class=move || if upload_err.get() { "ferro-error" } else { "ferro-muted" }>
                    {move || upload_status.get()}
                </p>
            </div>
            <div class="ferro-card">
                {move || {
                    if busy.get() {
                        return view! { <p class="ferro-muted">"Loading…"</p> }.into_any();
                    }
                    let err = load_err.get();
                    if !err.is_empty() {
                        return view! { <p class="ferro-error">{err}</p> }.into_any();
                    }
                    if items.read().is_empty() {
                        return view! { <p class="ferro-muted">"No media yet."</p> }.into_any();
                    }
                    view! {
                        <div class="ferro-media-grid">
                            <For each=move || items.get()
                                 key=|m| m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
                                 let:m>
                                {{
                                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let filename = m.get("filename").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let mime = m.get("mime").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let alt = m.get("alt").and_then(|v| v.as_str()).map(String::from);
                                    let size = m.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let raw_url = format!("/api/v1/media/{id}/raw");
                                    let raw_url2 = raw_url.clone();
                                    let is_image = kind == "image";
                                    let id_for_delete = id.clone();
                                    let filename_for_delete = filename.clone();
                                    let delete = delete.clone();
                                    view! {
                                        <div class="ferro-media-tile">
                                            {if is_image {
                                                view! {
                                                    <img src=raw_url alt=alt.unwrap_or(filename.clone()) />
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="ferro-media-placeholder">{kind.clone()}</div>
                                                }.into_any()
                                            }}
                                            <div class="ferro-media-name">{filename.clone()}</div>
                                            <div class="ferro-muted">
                                                {format!("{:.1} KiB · {mime}", size as f64 / 1024.0)}
                                            </div>
                                            <div class="ferro-row" style="margin-top: auto; gap: .5rem;">
                                                <a class="ferro-ghost" href=raw_url2 target="_blank">"View"</a>
                                                <button class="ferro-danger"
                                                    on:click=move |_| delete(id_for_delete.clone(), filename_for_delete.clone())>
                                                    "Delete"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                }}
                            </For>
                        </div>
                    }.into_any()
                }}
            </div>
        </Shell>
    }
}
