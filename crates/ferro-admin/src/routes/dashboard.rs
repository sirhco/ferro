use leptos::prelude::*;
use leptos_router::components::A;

use crate::routes::layout::Shell;
use crate::state::AdminState;

#[component]
pub fn Dashboard() -> impl IntoView {
    let state = expect_context::<AdminState>();
    let user = state.user;
    let types = state.types;

    view! {
        <Shell>
            {move || {
                let display = user.get().map(|u| u.display_name.unwrap_or(u.email));
                let count = types.get().len();
                view! {
                    <h2>{move || display.clone().unwrap_or_else(|| "Welcome.".into())}</h2>
                    <p class="ferro-muted">"Pick a destination from the side nav, or jump straight in:"</p>
                    <div class="ferro-quicklinks">
                        <A href="/admin/content">"Content"</A>
                        <A href="/admin/schema">"Schema"</A>
                        <A href="/admin/media">"Media"</A>
                        <A href="/admin/users">"Users"</A>
                        <A href="/admin/settings">"Settings"</A>
                    </div>
                    <p class="ferro-muted" style="margin-top: 1.5rem;">
                        {format!("{count} content type(s) registered.")}
                    </p>
                }
            }}
        </Shell>
    }
}
