//! Roles + permissions editor.
//!
//! Lists roles with their permission set; supports create / rename /
//! delete / permission-toggle. Permission picker covers the common operator
//! grants — global Read / Write / Publish, plus the four `Manage*` flags and
//! `Admin`. Per-type or `Own` scopes still require editing the JSON directly
//! (or via REST `PATCH /api/v1/roles/{id}`); a richer picker can land later
//! once we introduce a type selector.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::layout::Shell;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct Role {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    permissions: Vec<Value>,
}

/// Operator-friendly flags emitted as fully-formed `Permission` JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PermFlag {
    ReadGlobal,
    WriteGlobal,
    PublishGlobal,
    ManageUsers,
    ManageSchema,
    ManagePlugins,
    Admin,
}

impl PermFlag {
    const ALL: [PermFlag; 7] = [
        PermFlag::ReadGlobal,
        PermFlag::WriteGlobal,
        PermFlag::PublishGlobal,
        PermFlag::ManageUsers,
        PermFlag::ManageSchema,
        PermFlag::ManagePlugins,
        PermFlag::Admin,
    ];

    fn label(self) -> &'static str {
        match self {
            PermFlag::ReadGlobal => "Read (global)",
            PermFlag::WriteGlobal => "Write (global)",
            PermFlag::PublishGlobal => "Publish (global)",
            PermFlag::ManageUsers => "Manage users",
            PermFlag::ManageSchema => "Manage schema",
            PermFlag::ManagePlugins => "Manage plugins",
            PermFlag::Admin => "Admin (super-user)",
        }
    }

    fn to_json(self) -> Value {
        match self {
            PermFlag::ReadGlobal => json!({"action": "read", "scope": "global"}),
            PermFlag::WriteGlobal => json!({"action": "write", "scope": "global"}),
            PermFlag::PublishGlobal => json!({"action": "publish", "scope": "global"}),
            PermFlag::ManageUsers => json!({"action": "manage_users"}),
            PermFlag::ManageSchema => json!({"action": "manage_schema"}),
            PermFlag::ManagePlugins => json!({"action": "manage_plugins"}),
            PermFlag::Admin => json!({"action": "admin"}),
        }
    }

    fn matches(self, perm: &Value) -> bool {
        match self {
            PermFlag::ReadGlobal => is_scoped(perm, "read", "global"),
            PermFlag::WriteGlobal => is_scoped(perm, "write", "global"),
            PermFlag::PublishGlobal => is_scoped(perm, "publish", "global"),
            PermFlag::ManageUsers => is_action(perm, "manage_users"),
            PermFlag::ManageSchema => is_action(perm, "manage_schema"),
            PermFlag::ManagePlugins => is_action(perm, "manage_plugins"),
            PermFlag::Admin => is_action(perm, "admin"),
        }
    }
}

fn is_action(perm: &Value, action: &str) -> bool {
    perm.get("action").and_then(|v| v.as_str()) == Some(action)
}

fn is_scoped(perm: &Value, action: &str, scope: &str) -> bool {
    is_action(perm, action) && perm.get("scope").and_then(|v| v.as_str()) == Some(scope)
}

#[component]
pub fn RolesPage() -> impl IntoView {
    let roles = RwSignal::new(Vec::<Role>::new());
    let busy = RwSignal::new(true);
    let error = RwSignal::new(String::new());
    let banner = RwSignal::new(String::new());

    let editing_id = RwSignal::new(String::new()); // "" = new
    let form_name = RwSignal::new(String::new());
    let form_desc = RwSignal::new(String::new());
    let form_flags: RwSignal<Vec<PermFlag>> = RwSignal::new(Vec::new());
    let form_open = RwSignal::new(false);

    let load = move || {
        #[cfg(feature = "hydrate")]
        {
            busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::get::<Vec<Role>>("/api/v1/roles").await {
                    Ok(v) => {
                        roles.set(v);
                        error.set(String::new());
                    }
                    Err(e) => {
                        let mut msg = e.message();
                        if e.status() == Some(403) {
                            msg.push_str(" (your role doesn't have ManageUsers)");
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

    let open_new = move |_| {
        editing_id.set(String::new());
        form_name.set(String::new());
        form_desc.set(String::new());
        form_flags.set(Vec::new());
        form_open.set(true);
    };

    let open_edit = move |role: Role| {
        let flags: Vec<PermFlag> = PermFlag::ALL
            .iter()
            .copied()
            .filter(|f| role.permissions.iter().any(|p| f.matches(p)))
            .collect();
        editing_id.set(role.id.clone());
        form_name.set(role.name.clone());
        form_desc.set(role.description.clone().unwrap_or_default());
        form_flags.set(flags);
        form_open.set(true);
    };

    let toggle_flag = move |flag: PermFlag, on: bool| {
        form_flags.update(|fs| {
            fs.retain(|f| *f != flag);
            if on {
                fs.push(flag);
            }
        });
    };

    let save = move |_| {
        let id = editing_id.get();
        let name = form_name.get();
        if name.trim().is_empty() {
            banner.set("Name is required.".into());
            return;
        }
        let desc = form_desc.get();
        let desc_opt = if desc.trim().is_empty() { None } else { Some(desc) };
        let perms: Vec<Value> = form_flags.get().into_iter().map(|f| f.to_json()).collect();

        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = if id.is_empty() {
                    let body = json!({
                        "name": name,
                        "description": desc_opt,
                        "permissions": perms,
                    });
                    crate::api::post::<Role, _>("/api/v1/roles", &body).await
                } else {
                    let body = json!({
                        "name": name,
                        "description": desc_opt,
                        "permissions": perms,
                    });
                    let path = format!("/api/v1/roles/{id}");
                    crate::api::patch::<Role, _>(&path, &body).await
                };
                match result {
                    Ok(_) => {
                        banner.set("Saved.".into());
                        form_open.set(false);
                        load();
                    }
                    Err(e) => banner.set(format!("Save failed: {}", e.message())),
                }
            });
        }
    };

    let cancel = move |_| {
        form_open.set(false);
    };

    let delete_role = move |id: String| {
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/roles/{id}");
                match crate::api::delete::<Value>(&path).await {
                    Ok(_) => {
                        banner.set("Role deleted.".into());
                        load();
                    }
                    Err(e) => banner.set(format!("Delete failed: {}", e.message())),
                }
            });
        }
    };

    view! {
        <Shell>
            <h2>"Roles"</h2>
            <div class="ferro-card">
                <button class="ferro-btn" on:click=open_new>"New role"</button>
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
                    if roles.read().is_empty() {
                        return view! { <p class="ferro-muted">"No roles yet."</p> }.into_any();
                    }
                    view! {
                        <table>
                            <thead>
                                <tr>
                                    <th>"Name"</th>
                                    <th>"Description"</th>
                                    <th>"Permissions"</th>
                                    <th></th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each=move || roles.get()
                                     key=|r| r.id.clone()
                                     let:role>
                                    {{
                                        let role_for_edit = role.clone();
                                        let role_for_delete = role.id.clone();
                                        let perms = summarise_perms(&role.permissions);
                                        view! {
                                            <tr>
                                                <td>{role.name.clone()}</td>
                                                <td>{role.description.clone().unwrap_or_default()}</td>
                                                <td>{perms}</td>
                                                <td>
                                                    <button class="ferro-btn"
                                                        on:click=move |_| open_edit(role_for_edit.clone())>
                                                        "Edit"
                                                    </button>
                                                    " "
                                                    <button class="ferro-ghost"
                                                        on:click=move |_| delete_role(role_for_delete.clone())>
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

            {move || {
                if !form_open.get() {
                    return view! { <span></span> }.into_any();
                }
                let title = if editing_id.get().is_empty() { "New role" } else { "Edit role" };
                view! {
                    <div class="ferro-card">
                        <h3>{title}</h3>
                        <label>
                            "Name "
                            <input type="text"
                                prop:value=move || form_name.get()
                                on:input=move |ev| form_name.set(event_target_value(&ev)) />
                        </label>
                        <br/>
                        <label>
                            "Description "
                            <input type="text"
                                prop:value=move || form_desc.get()
                                on:input=move |ev| form_desc.set(event_target_value(&ev)) />
                        </label>
                        <fieldset>
                            <legend>"Permissions"</legend>
                            {PermFlag::ALL.iter().copied().map(|flag| {
                                let checked = move || form_flags.get().iter().any(|f| *f == flag);
                                view! {
                                    <label class="ferro-checkbox">
                                        <input type="checkbox"
                                            prop:checked=checked
                                            on:change=move |ev| {
                                                let on = event_target_checked(&ev);
                                                toggle_flag(flag, on);
                                            } />
                                        {flag.label()}
                                    </label>
                                }
                            }).collect::<Vec<_>>()}
                        </fieldset>
                        <p class="ferro-muted">
                            "Per-type or Own-scoped permissions can be edited via "
                            <code>"PATCH /api/v1/roles/{id}"</code>
                            "."
                        </p>
                        <button class="ferro-btn" on:click=save>"Save"</button>
                        " "
                        <button class="ferro-ghost" on:click=cancel>"Cancel"</button>
                    </div>
                }.into_any()
            }}
        </Shell>
    }
}

fn summarise_perms(perms: &[Value]) -> String {
    if perms.is_empty() {
        return "—".into();
    }
    let mut parts: Vec<String> = perms
        .iter()
        .map(|p| {
            let action = p.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            match p.get("scope").and_then(|v| v.as_str()) {
                Some(scope) => format!("{action}:{scope}"),
                None => action.to_string(),
            }
        })
        .collect();
    parts.sort();
    parts.dedup();
    parts.join(", ")
}

#[cfg(feature = "hydrate")]
fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}

#[cfg(not(feature = "hydrate"))]
fn event_target_checked(_ev: &leptos::ev::Event) -> bool {
    false
}
