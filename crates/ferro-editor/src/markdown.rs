use leptos::prelude::*;

/// Minimal Markdown editor: textarea + live preview. Swap for a full block
/// editor (ProseMirror/TipTap via wasm-bindgen) once plumbing stabilises.
#[component]
pub fn MarkdownEditor(
    #[prop(into)] value: Signal<String>,
    #[prop(into)] on_change: Callback<String>,
) -> impl IntoView {
    let preview = move || render_markdown(&value.get());

    view! {
        <div class="ferro-markdown-editor">
            <textarea
                class="ferro-markdown-source"
                on:input=move |ev| on_change.run(event_target_value(&ev))
                prop:value=move || value.get()
                rows="16"
            />
            <div class="ferro-markdown-preview" inner_html=preview />
        </div>
    }
}

/// Naive renderer. Real impl lives in `ferro-editor/markdown_render.rs`
/// (pulldown-cmark) once we're ready to ship markdown on the server.
pub fn render_markdown(src: &str) -> String {
    // Safe escape + <p>-wrap. Placeholder.
    let mut out = String::from("<p>");
    for ch in src.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out.push_str("</p>");
    out
}
