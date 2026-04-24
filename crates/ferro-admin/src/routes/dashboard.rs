use leptos::prelude::*;

#[component]
pub fn Dashboard() -> impl IntoView {
    view! {
        <section class="ferro-dashboard">
            <h1>"Ferro"</h1>
            <p>"Welcome back. This is the admin dashboard."</p>
            <nav class="ferro-quicklinks">
                <a href="/admin/content">"Content"</a>
                <a href="/admin/schema">"Schema"</a>
                <a href="/admin/media">"Media"</a>
                <a href="/admin/users">"Users"</a>
                <a href="/admin/plugins">"Plugins"</a>
                <a href="/admin/settings">"Settings"</a>
            </nav>
        </section>
    }
}
