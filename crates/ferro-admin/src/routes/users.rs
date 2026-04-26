use leptos::prelude::*;
use serde_json::Value;

use crate::routes::layout::Shell;

#[component]
pub fn UsersPage() -> impl IntoView {
    let users = RwSignal::new(Vec::<Value>::new());
    let busy = RwSignal::new(true);
    let error = RwSignal::new(String::new());

    Effect::new(move |prev: Option<()>| {
        if prev.is_some() {
            return;
        }
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::get::<Vec<Value>>("/api/v1/users").await {
                    Ok(v) => users.set(v),
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
    });

    view! {
        <Shell>
            <h2>"Users"</h2>
            <div class="ferro-card">
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
                                </tr>
                            </thead>
                            <tbody>
                                <For each=move || users.get()
                                     key=|u| u.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
                                     let:u>
                                    {{
                                        let email = u.get("email").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let handle = u.get("handle").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let roles = u.get("roles").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                                        let active = u.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
                                        view! {
                                            <tr>
                                                <td>{email}</td>
                                                <td>{handle}</td>
                                                <td>{roles}</td>
                                                <td>
                                                    {if active {
                                                        view! { <span class="ferro-ok">"yes"</span> }.into_any()
                                                    } else {
                                                        view! { <span class="ferro-error">"no"</span> }.into_any()
                                                    }}
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
