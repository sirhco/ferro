use leptos::prelude::*;
use leptos::ev::SubmitEvent;

use crate::state::AdminState;

const MFA_TOKEN_KEY: &str = "ferro.admin.mfa_token";

#[component]
pub fn LoginPage() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let e = email.get();
        let p = password.get();
        error.set(String::new());
        busy.set(true);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "email": e, "password": p });
                let res = crate::api::post::<serde_json::Value, _>("/api/v1/auth/login", &body).await;
                match res {
                    Ok(data) => {
                        if data.get("mfa_required").and_then(|v| v.as_bool()).unwrap_or(false) {
                            if let Some(tok) = data.get("mfa_token").and_then(|v| v.as_str()) {
                                if let Some(s) = web_sys::window()
                                    .and_then(|w| w.session_storage_or_local(MFA_TOKEN_KEY, tok))
                                {
                                    drop(s);
                                }
                            }
                            crate::util::navigate_to("/admin/mfa");
                        } else {
                            let access = data.get("token").and_then(|v| v.as_str());
                            let refresh = data.get("refresh_token").and_then(|v| v.as_str());
                            crate::api::set_tokens(access, refresh);
                            // Reload triggers bootstrap → /me lands → admin renders.
                            crate::util::navigate_to("/admin");
                        }
                    }
                    Err(err) => {
                        error.set(err.message());
                    }
                }
                busy.set(false);
            });
        }
        let _ = state;
    };

    view! {
        <main class="ferro-auth-shell">
            <form class="ferro-auth-card" on:submit=on_submit>
                <h1>"Ferro Admin"</h1>
                <label>
                    <span>"Email"</span>
                    <input type="email" autocomplete="username" required
                        prop:value=move || email.get()
                        on:input=move |ev| email.set(event_target_value(&ev)) />
                </label>
                <label>
                    <span>"Password"</span>
                    <input type="password" autocomplete="current-password" required
                        prop:value=move || password.get()
                        on:input=move |ev| password.set(event_target_value(&ev)) />
                </label>
                <button class="ferro-primary" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "Signing in…" } else { "Log in" }}
                </button>
                <p class="ferro-error">{move || error.get()}</p>
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
    let code = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let c = code.get();
        error.set(String::new());
        busy.set(true);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let mfa_token = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(MFA_TOKEN_KEY).ok().flatten())
                    .unwrap_or_default();
                let body = serde_json::json!({ "mfa_token": mfa_token, "code": c });
                let res = crate::api::post::<serde_json::Value, _>(
                    "/api/v1/auth/totp/login",
                    &body,
                )
                .await;
                match res {
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
                    Err(err) => {
                        error.set(err.message());
                    }
                }
                busy.set(false);
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
                        autocomplete="one-time-code" required
                        prop:value=move || code.get()
                        on:input=move |ev| code.set(event_target_value(&ev)) />
                </label>
                <button class="ferro-primary" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "Verifying…" } else { "Verify" }}
                </button>
                <button class="ferro-ghost" type="button"
                    on:click=move |_| crate::util::navigate_to("/admin/login")>
                    "Cancel"
                </button>
                <p class="ferro-error">{move || error.get()}</p>
            </form>
        </main>
    }
}

#[cfg(feature = "hydrate")]
trait WindowExt {
    fn session_storage_or_local(&self, key: &str, value: &str) -> Option<()>;
}

#[cfg(feature = "hydrate")]
impl WindowExt for web_sys::Window {
    fn session_storage_or_local(&self, key: &str, value: &str) -> Option<()> {
        let storage = self.local_storage().ok().flatten()?;
        storage.set_item(key, value).ok()?;
        Some(())
    }
}
