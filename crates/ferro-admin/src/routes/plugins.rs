use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::routes::layout::Shell;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct PluginInfo {
    name: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    declared: Vec<String>,
    #[serde(default)]
    granted: Vec<String>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    enabled: bool,
}

#[component]
pub fn PluginsPage() -> impl IntoView {
    let plugins = RwSignal::new(Vec::<PluginInfo>::new());
    let busy = RwSignal::new(true);
    let error = RwSignal::new(String::new());
    let banner = RwSignal::new(String::new());

    let load = move || {
        #[cfg(feature = "hydrate")]
        {
            busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::get::<Vec<PluginInfo>>("/api/v1/plugins").await {
                    Ok(v) => {
                        plugins.set(v);
                        error.set(String::new());
                    }
                    Err(e) => {
                        let mut msg = e.message();
                        if e.status() == Some(403) {
                            msg.push_str(" (your role doesn't have ManagePlugins)");
                        } else if e.status() == Some(503) {
                            msg = "Plugin host not initialized — check ferro.toml [plugins].".into();
                        }
                        error.set(msg);
                    }
                }
                busy.set(false);
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            busy.set(false);
        }
    };

    Effect::new(move |prev: Option<()>| {
        if prev.is_some() {
            return;
        }
        load();
    });

    let reload_all = move |_| {
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::post_empty::<serde_json::Value>("/api/v1/plugins/_all/reload")
                    .await
                {
                    Ok(_) => banner.set("Reloaded plugin directory.".into()),
                    Err(e) => banner.set(format!("Reload failed: {}", e.message())),
                }
                load();
            });
        }
    };

    let toggle_enabled = move |name: String, on: bool| {
        #[cfg(feature = "hydrate")]
        {
            let path = format!("/api/v1/plugins/{}/enabled", name);
            wasm_bindgen_futures::spawn_local(async move {
                #[derive(Serialize)]
                struct Body { enabled: bool }
                match crate::api::post::<PluginInfo, _>(&path, &Body { enabled: on }).await {
                    Ok(info) => {
                        plugins.update(|list| {
                            if let Some(p) = list.iter_mut().find(|p| p.name == info.name) {
                                *p = info;
                            }
                        });
                    }
                    Err(e) => banner.set(format!("Toggle failed: {}", e.message())),
                }
            });
        }
    };

    view! {
        <Shell>
            <h2>"Plugins"</h2>
            <div class="ferro-card">
                <p class="ferro-muted">
                    "Capabilities are granted via "
                    <code>"ferro.toml"</code>
                    " under "
                    <code>"[[plugins.grants]]"</code>
                    ". Edits made through the API are session-scoped and lost on restart."
                </p>
                <button class="ferro-btn" on:click=reload_all>"Reload directory"</button>
                {move || {
                    let b = banner.get();
                    if b.is_empty() { view! { <span></span> }.into_any() }
                    else { view! { <p class="ferro-ok">{b}</p> }.into_any() }
                }}
                {move || {
                    if busy.get() {
                        return view! { <p class="ferro-muted">"Loading…"</p> }.into_any();
                    }
                    let err = error.get();
                    if !err.is_empty() {
                        return view! { <p class="ferro-error">{err}</p> }.into_any();
                    }
                    if plugins.read().is_empty() {
                        return view! {
                            <p class="ferro-muted">
                                "No plugins installed. Drop a "
                                <code>"<name>/{plugin.toml, plugin.wasm}"</code>
                                " under the configured "
                                <code>"plugins.dir"</code>
                                " and click Reload."
                            </p>
                        }.into_any();
                    }
                    view! {
                        <table>
                            <thead>
                                <tr>
                                    <th>"Name"</th>
                                    <th>"Version"</th>
                                    <th>"Hooks"</th>
                                    <th>"Declared caps"</th>
                                    <th>"Granted caps"</th>
                                    <th>"Status"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each=move || plugins.get()
                                     key=|p| p.name.clone()
                                     let:p>
                                    {{
                                        let name = p.name.clone();
                                        let toggle_name = name.clone();
                                        let enabled = p.enabled;
                                        view! {
                                            <tr>
                                                <td>{p.name.clone()}</td>
                                                <td>{p.version.clone()}</td>
                                                <td>{join_or_dash(&p.hooks)}</td>
                                                <td>{join_or_dash(&p.declared)}</td>
                                                <td>{join_or_dash(&p.granted)}</td>
                                                <td>
                                                    <button class="ferro-btn"
                                                        on:click=move |_| toggle_enabled(toggle_name.clone(), !enabled)>
                                                        {if enabled { "Disable" } else { "Enable" }}
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
        </Shell>
    }
}

fn join_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items.join(", ")
    }
}
