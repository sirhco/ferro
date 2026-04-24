use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Meta, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{path, SsrMode};

use crate::routes;

/// HTML shell rendered on the server. `cargo leptos` injects the hydration
/// bootstrap + split `<link>` preloads into `<head>`.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() islands=true />
                <MetaTags />
                <Stylesheet id="leptos" href="/pkg/ferro_admin.css" />
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

    view! {
        <Meta name="color-scheme" content="light dark" />
        <Router>
            <main id="ferro-main">
                <Routes fallback=|| view! { <p>"404"</p> }>
                    <Route path=path!("/admin/login") view=routes::login::LoginPage />
                    <Route path=path!("/admin") view=routes::dashboard::Dashboard ssr=SsrMode::InOrder />
                    <Route path=path!("/admin/content/:type_slug") view=routes::content_list::ContentList />
                    <Route path=path!("/admin/content/:type_slug/:id") view=routes::content_edit::ContentEdit />
                    <Route path=path!("/admin/schema") view=routes::schema::SchemaBuilder />
                    <Route path=path!("/admin/media") view=routes::media::MediaLibrary />
                    <Route path=path!("/admin/users") view=routes::users::UsersPage />
                    <Route path=path!("/admin/plugins") view=routes::plugins::PluginsPage />
                    <Route path=path!("/admin/settings") view=routes::settings::SettingsPage />
                </Routes>
            </main>
        </Router>
    }
}
