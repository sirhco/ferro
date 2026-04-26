use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::routes;
use crate::state::AdminState;

/// HTML shell rendered on the server. `cargo leptos` injects the hydration
/// bootstrap into `<head>`.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() />
                <MetaTags />
                <Stylesheet id="leptos" href="/pkg/ferro_admin.css" />
                <link rel="icon" type="image/svg+xml" href="/pkg/favicon.svg" />
                <Title text="Ferro Admin" />
            </head>
            <body class="ferro-admin-body">
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let admin_state = AdminState::new();
    provide_context(admin_state);

    // Run once after hydration to populate `user` + `types` from the API.
    Effect::new(move |_| {
        bootstrap(admin_state);
    });

    view! {
        <Router>
            <Routes fallback=|| view! { <p class="ferro-empty">"404"</p> }>
                <Route path=path!("/admin/login") view=routes::login::LoginPage />
                <Route path=path!("/admin/mfa") view=routes::login::MfaPage />
                <Route path=path!("/admin") view=routes::dashboard::Dashboard />
                <Route path=path!("/admin/content") view=routes::content_list::ContentList />
                <Route path=path!("/admin/content/:type_slug") view=routes::content_list::ContentList />
                <Route path=path!("/admin/content/:type_slug/new") view=routes::content_edit::ContentEdit />
                <Route path=path!("/admin/content/:type_slug/edit/:slug") view=routes::content_edit::ContentEdit />
                <Route path=path!("/admin/schema") view=routes::schema::SchemaList />
                <Route path=path!("/admin/schema/new") view=routes::schema::SchemaEdit />
                <Route path=path!("/admin/schema/edit/:slug") view=routes::schema::SchemaEdit />
                <Route path=path!("/admin/media") view=routes::media::MediaLibrary />
                <Route path=path!("/admin/users") view=routes::users::UsersPage />
                <Route path=path!("/admin/plugins") view=routes::plugins::PluginsPage />
                <Route path=path!("/admin/settings") view=routes::settings::SettingsPage />
            </Routes>
        </Router>
    }
}

fn bootstrap(_state: AdminState) {
    #[cfg(feature = "hydrate")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            crate::routes::bootstrap_after_mount(_state).await;
        });
    }
}
