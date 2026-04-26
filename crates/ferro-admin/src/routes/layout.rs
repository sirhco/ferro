//! Shared chrome: header + side nav + toast. Wraps each authenticated
//! route's body via `<Shell>{children}</Shell>` so the nav stays visible
//! across navigation.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use crate::state::{AdminState, ToastKind};

/// Render `body` inside the standard admin chrome. If no user is in state
/// after bootstrap, redirect to /admin/login. While bootstrap is pending
/// the body shows a "Loading…" sentinel.
#[component]
pub fn Shell(children: ChildrenFn) -> impl IntoView {
    let state = expect_context::<AdminState>();
    let bootstrapped = state.bootstrapped;
    let user = state.user;

    Effect::new(move |_| {
        if bootstrapped.get() && user.read().is_none() {
            crate::util::navigate_to("/admin/login");
        }
    });

    let user_label = move || match user.get() {
        Some(u) => format!("{} · {}", u.email, u.handle),
        None => String::new(),
    };

    let logout = move |_| {
        #[cfg(feature = "hydrate")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let refresh = crate::api::get_refresh();
                if let Some(rt) = refresh {
                    let _ = crate::api::post::<serde_json::Value, _>(
                        "/api/v1/auth/logout",
                        &serde_json::json!({ "refresh_token": rt }),
                    )
                    .await;
                }
                crate::api::clear_tokens();
                crate::util::navigate_to("/admin/login");
            });
        }
    };

    view! {
        <div class="ferro-shell">
            <header class="ferro-header">
                <h1>"Ferro"</h1>
                <span class="ferro-user-pill">{user_label}</span>
                <button class="ferro-ghost" on:click=logout>"Log out"</button>
            </header>
            <div class="ferro-body">
                <Nav />
                <section class="ferro-view">
                    {move || {
                        if !bootstrapped.get() {
                            view! { <p class="ferro-muted">"Loading…"</p> }.into_any()
                        } else if user.read().is_none() {
                            view! { <p class="ferro-muted">"Redirecting to sign in…"</p> }.into_any()
                        } else {
                            children().into_any()
                        }
                    }}
                </section>
            </div>
            <ToastView />
        </div>
    }
}

#[component]
fn Nav() -> impl IntoView {
    let location = use_location();
    let active = move |prefix: &str| {
        let path = location.pathname.get();
        if prefix == "/admin" {
            path == "/admin"
        } else {
            path == prefix || path.starts_with(&format!("{prefix}/"))
        }
    };

    let cls = move |prefix: &'static str| {
        if active(prefix) {
            "ferro-nav-link ferro-nav-active"
        } else {
            "ferro-nav-link"
        }
    };

    view! {
        <nav class="ferro-nav">
            <A href="/admin" attr:class=move || cls("/admin")>"Dashboard"</A>
            <A href="/admin/content" attr:class=move || cls("/admin/content")>"Content"</A>
            <A href="/admin/schema" attr:class=move || cls("/admin/schema")>"Schema"</A>
            <A href="/admin/media" attr:class=move || cls("/admin/media")>"Media"</A>
            <A href="/admin/users" attr:class=move || cls("/admin/users")>"Users"</A>
            <A href="/admin/roles" attr:class=move || cls("/admin/roles")>"Roles"</A>
            <A href="/admin/plugins" attr:class=move || cls("/admin/plugins")>"Plugins"</A>
            <A href="/admin/settings" attr:class=move || cls("/admin/settings")>"Settings"</A>
        </nav>
    }
}

#[component]
fn ToastView() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let toast = state.toast;

    Effect::new(move |_| {
        if toast.read().is_some() {
            #[cfg(feature = "hydrate")]
            {
                let toast = toast;
                let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    toast.set(None);
                }) as Box<dyn Fn()>);
                if let Some(window) = web_sys::window() {
                    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.as_ref().unchecked_ref(),
                        2500,
                    );
                }
                cb.forget();
            }
        }
    });

    view! {
        {move || toast.get().map(|t| {
            let cls = match t.kind {
                ToastKind::Ok => "ferro-toast ferro-toast-ok",
                ToastKind::Err => "ferro-toast ferro-toast-err",
            };
            view! { <div class=cls>{t.message}</div> }
        })}
    }
}

#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;
