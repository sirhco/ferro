use leptos::prelude::*;

#[component]
pub fn EditorToolbar() -> impl IntoView {
    view! {
        <div class="ferro-editor-toolbar">
            <button type="button">"B"</button>
            <button type="button"><i>"I"</i></button>
            <button type="button">"H1"</button>
            <button type="button">"H2"</button>
            <button type="button">"Link"</button>
        </div>
    }
}
