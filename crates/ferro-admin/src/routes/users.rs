//! User management.
//!
//! Lists every user, lets operators with `ManageUsers` create / edit / delete
//! and assign roles. Roles are loaded alongside users so the editor's
//! role-picker shows human-readable names. Password rotation lands in the
//! same edit form (leave blank to keep the existing hash).

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::layout::Shell;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct User {
    id: String,
    email: String,
    handle: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    active: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct Role {
    id: String,
    name: String,
}

#[component]
pub fn UsersPage() -> impl IntoView {
    let users = RwSignal::new(Vec::<User>::new());
    let roles = RwSignal::new(Vec::<Role>::new());
    let busy = RwSignal::new(true);
    let error = RwSignal::new(String::new());
    let banner = RwSignal::new(String::new());

    let editing_id = RwSignal::new(String::new()); // "" = new
    let f_email = RwSignal::new(String::new());
    let f_handle = RwSignal::new(String::new());
    let f_display = RwSignal::new(String::new());
    let f_password = RwSignal::new(String::new());
    let f_roles: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let f_active = RwSignal::new(true);
    let form_open = RwSignal::new(false);

    let load = move || {
        #[cfg(feature = "hydrate")]
        {
            busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let users_fut = crate::api::get::<Vec<User>>("/api/v1/users");
                let roles_fut = crate::api::get::<Vec<Role>>("/api/v1/roles");
                let users_res = users_fut.await;
                let roles_res = roles_fut.await;

                match users_res {
                    Ok(v) => {
                        users.set(v);
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
                if let Ok(rs) = roles_res {
                    roles.set(rs);
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
        f_email.set(String::new());
        f_handle.set(String::new());
        f_display.set(String::new());
        f_password.set(String::new());
        f_roles.set(Vec::new());
        f_active.set(true);
        form_open.set(true);
    };

    let open_edit = move |u: User| {
        editing_id.set(u.id.clone());
        f_email.set(u.email.clone());
        f_handle.set(u.handle.clone());
        f_display.set(u.display_name.clone().unwrap_or_default());
        f_password.set(String::new());
        f_roles.set(u.roles.clone());
        f_active.set(u.active);
        form_open.set(true);
    };

    let toggle_role = move |role_id: String, on: bool| {
        f_roles.update(|rs| {
            rs.retain(|r| r != &role_id);
            if on {
                rs.push(role_id);
            }
        });
    };

    let save = move |_| {
        let id = editing_id.get();
        let email = f_email.get();
        let handle = f_handle.get();
        let display = f_display.get();
        let display_opt = if display.trim().is_empty() { None } else { Some(display) };
        let password = f_password.get();
        let role_ids = f_roles.get();
        let active = f_active.get();

        if email.trim().is_empty() || handle.trim().is_empty() {
            banner.set("Email and handle are required.".into());
            return;
        }

        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = if id.is_empty() {
                    let body = json!({
                        "email": email,
                        "handle": handle,
                        "display_name": display_opt,
                        "password": if password.is_empty() { None } else { Some(password) },
                        "roles": role_ids,
                        "active": active,
                    });
                    crate::api::post::<User, _>("/api/v1/users", &body).await
                } else {
                    let mut body = json!({
                        "email": email,
                        "handle": handle,
                        "display_name": display_opt,
                        "roles": role_ids,
                        "active": active,
                    });
                    if !password.is_empty() {
                        body["password"] = json!(password);
                    }
                    let path = format!("/api/v1/users/{id}");
                    crate::api::patch::<User, _>(&path, &body).await
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

    let cancel = move |_| form_open.set(false);

    let delete_user = move |id: String| {
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/api/v1/users/{id}");
                match crate::api::delete::<Value>(&path).await {
                    Ok(_) => {
                        banner.set("User deleted.".into());
                        load();
                    }
                    Err(e) => banner.set(format!("Delete failed: {}", e.message())),
                }
            });
        }
    };

    view! {
        <Shell>
            <h2>"Users"</h2>
            <div class="ferro-card">
                <button class="ferro-btn" on:click=open_new>"New user"</button>
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
                    if users.read().is_empty() {
                        return view! { <p class="ferro-muted">"No users."</p> }.into_any();
                    }
                    view! {
                        <table>
                            <thead>
                                <tr>
                                    <th>"Email"</th>
                                    <th>"Handle"</th>
                                    <th>"Roles"</th>
                                    <th>"Active"</th>
                                    <th></th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each=move || users.get()
                                     key=|u| u.id.clone()
                                     let:u>
                                    {{
                                        let user_for_edit = u.clone();
                                        let user_for_delete = u.id.clone();
                                        let role_names = role_names_for(&u.roles, &roles.read());
                                        view! {
                                            <tr>
                                                <td>{u.email.clone()}</td>
                                                <td>{u.handle.clone()}</td>
                                                <td>{role_names}</td>
                                                <td>
                                                    {if u.active {
                                                        view! { <span class="ferro-ok">"yes"</span> }.into_any()
                                                    } else {
                                                        view! { <span class="ferro-error">"no"</span> }.into_any()
                                                    }}
                                                </td>
                                                <td>
                                                    <button class="ferro-btn"
                                                        on:click=move |_| open_edit(user_for_edit.clone())>
                                                        "Edit"
                                                    </button>
                                                    " "
                                                    <button class="ferro-ghost"
                                                        on:click=move |_| delete_user(user_for_delete.clone())>
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
                let title = if editing_id.get().is_empty() { "New user" } else { "Edit user" };
                let role_list = roles.get();
                view! {
                    <div class="ferro-card">
                        <h3>{title}</h3>
                        <label>"Email "
                            <input type="email"
                                prop:value=move || f_email.get()
                                on:input=move |ev| f_email.set(event_target_value(&ev)) />
                        </label><br/>
                        <label>"Handle "
                            <input type="text"
                                prop:value=move || f_handle.get()
                                on:input=move |ev| f_handle.set(event_target_value(&ev)) />
                        </label><br/>
                        <label>"Display name "
                            <input type="text"
                                prop:value=move || f_display.get()
                                on:input=move |ev| f_display.set(event_target_value(&ev)) />
                        </label><br/>
                        <label>"Password "
                            <input type="password"
                                placeholder="leave blank to keep current"
                                prop:value=move || f_password.get()
                                on:input=move |ev| f_password.set(event_target_value(&ev)) />
                        </label><br/>
                        <label class="ferro-checkbox">
                            <input type="checkbox"
                                prop:checked=move || f_active.get()
                                on:change=move |ev| f_active.set(event_target_checked(&ev)) />
                            "Active"
                        </label>
                        <fieldset>
                            <legend>"Roles"</legend>
                            {if role_list.is_empty() {
                                view! { <p class="ferro-muted">"No roles defined. Create one in Roles first."</p> }.into_any()
                            } else {
                                role_list.into_iter().map(|r| {
                                    let role_id = r.id.clone();
                                    let role_id_for_check = role_id.clone();
                                    let role_id_for_toggle = role_id.clone();
                                    let checked = move || f_roles.get().iter().any(|x| x == &role_id_for_check);
                                    view! {
                                        <label class="ferro-checkbox">
                                            <input type="checkbox"
                                                prop:checked=checked
                                                on:change=move |ev| {
                                                    let on = event_target_checked(&ev);
                                                    toggle_role(role_id_for_toggle.clone(), on);
                                                } />
                                            {r.name.clone()}
                                        </label>
                                    }
                                }).collect::<Vec<_>>().into_any()
                            }}
                        </fieldset>
                        <button class="ferro-btn" on:click=save>"Save"</button>
                        " "
                        <button class="ferro-ghost" on:click=cancel>"Cancel"</button>
                    </div>
                }.into_any()
            }}
        </Shell>
    }
}

fn role_names_for(role_ids: &[String], roles: &[Role]) -> String {
    if role_ids.is_empty() {
        return "—".into();
    }
    let mut names: Vec<String> = role_ids
        .iter()
        .map(|id| {
            roles.iter().find(|r| &r.id == id).map(|r| r.name.clone()).unwrap_or_else(|| id.clone())
        })
        .collect();
    names.sort();
    names.join(", ")
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
