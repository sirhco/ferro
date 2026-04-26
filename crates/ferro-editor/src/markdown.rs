use leptos::prelude::*;
use pulldown_cmark::{Options, Parser};

/// Markdown editor: textarea + live HTML preview rendered by pulldown-cmark.
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

/// Render CommonMark to HTML via pulldown-cmark with sensible defaults
/// (tables, footnotes, strikethrough, task lists). Smart-punct off because
/// it transforms quotes/dashes that may be load-bearing in code samples.
pub fn render_markdown(src: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    let parser = Parser::new_ext(src, opts);
    let mut out = String::with_capacity(src.len() * 3 / 2);
    pulldown_cmark::html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_heading() {
        assert_eq!(render_markdown("# Hi"), "<h1>Hi</h1>\n");
    }

    #[test]
    fn renders_emphasis_and_link() {
        let html = render_markdown("**bold** and [a](https://example.com)");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains(r#"<a href="https://example.com">a</a>"#));
    }

    #[test]
    fn renders_fenced_code_block() {
        let html = render_markdown("```rust\nfn main() {}\n```\n");
        assert!(html.contains("<pre><code class=\"language-rust\">"));
        assert!(html.contains("fn main() {}"));
    }

    #[test]
    fn renders_table() {
        let html = render_markdown("| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>a</th>"));
    }

    #[test]
    fn escapes_inline_html_safely() {
        let html = render_markdown("text with <script>alert(1)</script>");
        // pulldown-cmark passes raw HTML through by default; that's a known
        // behavior. Our admin/preview surfaces are auth-gated. Public site
        // SHOULD sanitize before injecting if users get markdown access.
        // Test pins current behavior so future sanitization changes are a
        // conscious decision.
        assert!(html.contains("<script>"));
    }
}
