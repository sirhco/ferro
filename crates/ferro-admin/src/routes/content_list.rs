use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[server(endpoint = "/_srv/content_list")]
pub async fn list_content(type_slug: String) -> Result<Vec<ContentRow>, ServerFnError> {
    // Wired in by CLI via server-side context.
    Ok(vec![])
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ContentRow {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub status: String,
}

#[component]
pub fn ContentList() -> impl IntoView {
    let params = use_params_map();
    let type_slug = move || params.read().get("type_slug").unwrap_or_default();

    let rows = Resource::new(type_slug, |slug| async move {
        list_content(slug).await.unwrap_or_default()
    });

    view! {
        <section class="ferro-content-list">
            <h1>{move || format!("Content — {}", type_slug())}</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                <table>
                    <thead>
                        <tr><th>"Title"</th><th>"Slug"</th><th>"Status"</th></tr>
                    </thead>
                    <tbody>
                        <For each=move || rows.get().unwrap_or_default()
                             key=|r| r.id.clone()
                             let:row>
                            <tr>
                                <td><a href=format!("/admin/content/{}/{}", type_slug(), row.id)>{row.title.clone()}</a></td>
                                <td>{row.slug}</td>
                                <td>{row.status}</td>
                            </tr>
                        </For>
                    </tbody>
                </table>
            </Suspense>
        </section>
    }
}
