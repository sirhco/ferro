use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use serde_json::Value;

use crate::routes::layout::Shell;
use crate::state::AdminState;

#[component]
pub fn SettingsPage() -> impl IntoView {
    let state = expect_context::<AdminState>();

    view! {
        <Shell>
            <h2>"Settings"</h2>
            <div class="ferro-card">
                <h3>"Change password"</h3>
                <ChangePasswordForm />
            </div>
            <div class="ferro-card">
                <h3>"Two-factor authentication"</h3>
                <TotpPanel />
            </div>
            <div class="ferro-card">
                <h3>"Session"</h3>
                <p>
                    "Logged in as "
                    <strong>{move || state.user.get().map(|u| u.email).unwrap_or_default()}</strong>
                    ". Token stored in browser localStorage."
                </p>
            </div>
        </Shell>
    }
}

#[component]
fn ChangePasswordForm() -> impl IntoView {
    let cur = RwSignal::new(String::new());
    let next = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());
    let status_err = RwSignal::new(false);

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let c = cur.get();
        let n = next.get();
        status.set("Updating…".into());
        status_err.set(false);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "current_password": c,
                    "new_password": n,
                });
                match crate::api::post::<Value, _>("/api/v1/auth/change-password", &body).await {
                    Ok(_) => {
                        cur.set(String::new());
                        next.set(String::new());
                        status_err.set(false);
                        status.set("Password updated.".into());
                    }
                    Err(e) => {
                        status_err.set(true);
                        status.set(e.message());
                    }
                }
            });
        }
    };

    view! {
        <form on:submit=on_submit>
            <label>
                <span>"Current password"</span>
                <input type="password" autocomplete="current-password" required
                    prop:value=move || cur.get()
                    on:input=move |ev| cur.set(event_target_value(&ev)) />
            </label>
            <label>
                <span>"New password (min 8 chars)"</span>
                <input type="password" autocomplete="new-password" required minlength="8"
                    prop:value=move || next.get()
                    on:input=move |ev| next.set(event_target_value(&ev)) />
            </label>
            <p>
                <button class="ferro-primary" type="submit">"Update"</button>
            </p>
            <p class=move || if status_err.get() { "ferro-error" } else { "ferro-ok" }>
                {move || status.get()}
            </p>
        </form>
    }
}

#[component]
fn TotpPanel() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let user = state.user;

    // Steps: idle → setup_done(secret, uri) → enabled.
    let secret = RwSignal::new(String::new());
    let uri = RwSignal::new(String::new());
    let code = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());
    let status_err = RwSignal::new(false);

    let begin_setup = move |_| {
        status.set("Generating secret…".into());
        status_err.set(false);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                match crate::api::post_empty::<Value>("/api/v1/auth/totp/setup").await {
                    Ok(v) => {
                        if let Some(s) = v.get("secret").and_then(|x| x.as_str()) {
                            secret.set(s.into());
                        }
                        if let Some(u) = v.get("otpauth_uri").and_then(|x| x.as_str()) {
                            uri.set(u.into());
                        }
                        status.set(String::new());
                    }
                    Err(e) => {
                        status_err.set(true);
                        status.set(e.message());
                        if e.status() == Some(400)
                            && e.message().to_lowercase().contains("already enabled")
                        {
                            // Server says it's on; flip local state.
                            state.user.update(|u| {
                                if let Some(u) = u.as_mut() {
                                    u.totp_enabled = true;
                                }
                            });
                        }
                    }
                }
            });
        }
    };

    let confirm_enable = move |_| {
        let s = secret.get();
        let c = code.get();
        status.set("Verifying…".into());
        status_err.set(false);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "secret": s, "code": c });
                match crate::api::post::<Value, _>("/api/v1/auth/totp/enable", &body).await {
                    Ok(_) => {
                        state.user.update(|u| {
                            if let Some(u) = u.as_mut() {
                                u.totp_enabled = true;
                            }
                        });
                        secret.set(String::new());
                        code.set(String::new());
                        status_err.set(false);
                        status.set("TOTP enabled. New logins now require a code.".into());
                    }
                    Err(e) => {
                        status_err.set(true);
                        status.set(e.message());
                    }
                }
            });
        }
    };

    let confirm_disable = move |_| {
        let c = code.get();
        status.set("Disabling…".into());
        status_err.set(false);
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "code": c });
                match crate::api::post::<Value, _>("/api/v1/auth/totp/disable", &body).await {
                    Ok(_) => {
                        state.user.update(|u| {
                            if let Some(u) = u.as_mut() {
                                u.totp_enabled = false;
                            }
                        });
                        code.set(String::new());
                        status_err.set(false);
                        status.set("TOTP disabled.".into());
                    }
                    Err(e) => {
                        status_err.set(true);
                        status.set(e.message());
                    }
                }
            });
        }
    };

    view! {
        {move || {
            let enabled = user.read().as_ref().map(|u| u.totp_enabled).unwrap_or(false);
            if enabled {
                return view! {
                    <div>
                        <p class="ferro-ok">"TOTP is enabled."</p>
                        <label>
                            <span>"Enter a current 6-digit code to disable"</span>
                            <input type="text" inputmode="numeric" maxlength="6"
                                prop:value=move || code.get()
                                on:input=move |ev| code.set(event_target_value(&ev)) />
                        </label>
                        <p>
                            <button class="ferro-danger" on:click=confirm_disable>"Disable"</button>
                        </p>
                    </div>
                }.into_any();
            }
            if !secret.get().is_empty() {
                return view! {
                    <div>
                        <p>"Scan with an authenticator app or paste the secret manually:"</p>
                        <p><code style="word-break: break-all;">{move || secret.get()}</code></p>
                        <details>
                            <summary>"otpauth URI"</summary>
                            <pre style="white-space: pre-wrap;">{move || uri.get()}</pre>
                        </details>
                        <label>
                            <span>"6-digit code from your app"</span>
                            <input type="text" inputmode="numeric" maxlength="6"
                                prop:value=move || code.get()
                                on:input=move |ev| code.set(event_target_value(&ev)) />
                        </label>
                        <p>
                            <button class="ferro-primary" on:click=confirm_enable>"Enable"</button>
                            " "
                            <button class="ferro-ghost"
                                on:click=move |_| { secret.set(String::new()); code.set(String::new()); }>
                                "Cancel"
                            </button>
                        </p>
                    </div>
                }.into_any();
            }
            view! {
                <div>
                    <p class="ferro-muted">
                        "Time-based one-time passwords (Google Authenticator, 1Password, Authy)."
                    </p>
                    <button class="ferro-primary" on:click=begin_setup>"Set up TOTP"</button>
                </div>
            }.into_any()
        }}
        <p class=move || if status_err.get() { "ferro-error" } else { "ferro-ok" }>
            {move || status.get()}
        </p>
    }
}
