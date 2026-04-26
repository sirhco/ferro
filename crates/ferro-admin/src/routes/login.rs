use leptos::ev::SubmitEvent;
use leptos::prelude::*;

const MFA_TOKEN_KEY: &str = "ferro.admin.mfa_token";

#[component]
pub fn LoginPage() -> impl IntoView {
    let email_ref = NodeRef::<leptos::html::Input>::new();
    let password_ref = NodeRef::<leptos::html::Input>::new();
    let error_ref = NodeRef::<leptos::html::P>::new();

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        #[cfg(feature = "hydrate")]
        {
            let email = email_ref
                .get()
                .map(|el| el.value())
                .unwrap_or_default();
            let password = password_ref
                .get()
                .map(|el| el.value())
                .unwrap_or_default();
            let err_node = error_ref.get();
            if let Some(p) = err_node.as_ref() {
                p.set_text_content(Some(""));
            }
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "email": email, "password": password });
                match crate::api::post::<serde_json::Value, _>("/api/v1/auth/login", &body).await {
                    Ok(data) => {
                        if data.get("mfa_required").and_then(|v| v.as_bool()).unwrap_or(false) {
                            if let Some(tok) = data.get("mfa_token").and_then(|v| v.as_str()) {
                                if let Some(s) = web_sys::window()
                                    .and_then(|w| w.local_storage().ok().flatten())
                                {
                                    let _ = s.set_item(MFA_TOKEN_KEY, tok);
                                }
                            }
                            crate::util::navigate_to("/admin/mfa");
                        } else {
                            let access = data.get("token").and_then(|v| v.as_str());
                            let refresh = data.get("refresh_token").and_then(|v| v.as_str());
                            crate::api::set_tokens(access, refresh);
                            crate::util::navigate_to("/admin");
                        }
                    }
                    Err(e) => {
                        if let Some(p) = err_node {
                            p.set_text_content(Some(&e.message()));
                        }
                    }
                }
            });
        }
    };

    view! {
        <main class="ferro-auth-shell">
            <form class="ferro-auth-card" on:submit=on_submit>
                <h1>"Ferro Admin"</h1>
                <label>
                    <span>"Email"</span>
                    <input type="email" autocomplete="username" required node_ref=email_ref />
                </label>
                <label>
                    <span>"Password"</span>
                    <input type="password" autocomplete="current-password" required node_ref=password_ref />
                </label>
                <button class="ferro-primary" type="submit">"Log in"</button>
                <p class="ferro-error" node_ref=error_ref></p>
                <p class="ferro-muted">
                    "No account? Ask an operator to run "
                    <code>"ferro admin create-user --with-admin"</code>
                    "."
                </p>
            </form>
        </main>
    }
}

#[component]
pub fn MfaPage() -> impl IntoView {
    let code_ref = NodeRef::<leptos::html::Input>::new();
    let error_ref = NodeRef::<leptos::html::P>::new();

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        #[cfg(feature = "hydrate")]
        {
            let code = code_ref.get().map(|el| el.value()).unwrap_or_default();
            let err_node = error_ref.get();
            if let Some(p) = err_node.as_ref() {
                p.set_text_content(Some(""));
            }
            wasm_bindgen_futures::spawn_local(async move {
                let mfa_token = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(MFA_TOKEN_KEY).ok().flatten())
                    .unwrap_or_default();
                let body = serde_json::json!({ "mfa_token": mfa_token, "code": code });
                match crate::api::post::<serde_json::Value, _>("/api/v1/auth/totp/login", &body).await {
                    Ok(data) => {
                        let access = data.get("token").and_then(|v| v.as_str());
                        let refresh = data.get("refresh_token").and_then(|v| v.as_str());
                        crate::api::set_tokens(access, refresh);
                        if let Some(s) = web_sys::window()
                            .and_then(|w| w.local_storage().ok().flatten())
                        {
                            let _ = s.remove_item(MFA_TOKEN_KEY);
                        }
                        crate::util::navigate_to("/admin");
                    }
                    Err(e) => {
                        if let Some(p) = err_node {
                            p.set_text_content(Some(&e.message()));
                        }
                    }
                }
            });
        }
    };

    view! {
        <main class="ferro-auth-shell">
            <form class="ferro-auth-card" on:submit=on_submit>
                <h1>"Two-factor code"</h1>
                <p class="ferro-muted">"Enter the 6-digit code from your authenticator app."</p>
                <label>
                    <span>"Code"</span>
                    <input type="text" inputmode="numeric" maxlength="6"
                        autocomplete="one-time-code" required node_ref=code_ref />
                </label>
                <button class="ferro-primary" type="submit">"Verify"</button>
                <button class="ferro-ghost" type="button"
                    on:click=move |_| crate::util::navigate_to("/admin/login")>
                    "Cancel"
                </button>
                <p class="ferro-error" node_ref=error_ref></p>
            </form>
        </main>
    }
}
