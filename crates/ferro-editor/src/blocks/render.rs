use super::model::{Block, Document};

pub fn render_html(doc: &Document, media_base_url: &str) -> String {
    let mut out = String::with_capacity(doc.len() * 64);
    for block in doc {
        render_block(block, media_base_url, &mut out);
    }
    out
}

fn render_block(block: &Block, media_base_url: &str, out: &mut String) {
    match block {
        Block::Paragraph { text } => {
            out.push_str("<p>");
            push_escaped(text, out);
            out.push_str("</p>");
        }
        Block::Heading { level, text } => {
            let lvl = (*level).clamp(1, 6);
            out.push_str(&format!("<h{lvl}>"));
            push_escaped(text, out);
            out.push_str(&format!("</h{lvl}>"));
        }
        Block::Quote { text, cite } => {
            out.push_str("<blockquote><p>");
            push_escaped(text, out);
            out.push_str("</p>");
            if let Some(c) = cite {
                if !c.is_empty() {
                    out.push_str("<cite>");
                    push_escaped(c, out);
                    out.push_str("</cite>");
                }
            }
            out.push_str("</blockquote>");
        }
        Block::Code { lang, code } => {
            out.push_str("<pre><code");
            if let Some(l) = lang {
                if !l.is_empty() {
                    out.push_str(" class=\"language-");
                    push_escaped(l, out);
                    out.push('"');
                }
            }
            out.push('>');
            push_escaped(code, out);
            out.push_str("</code></pre>");
        }
        Block::Image { media_id, alt } => {
            let base = media_base_url.trim_end_matches('/');
            out.push_str("<figure><img src=\"");
            push_escaped(base, out);
            out.push('/');
            push_escaped(media_id, out);
            out.push_str("\" alt=\"");
            if let Some(a) = alt {
                push_escaped(a, out);
            }
            out.push_str("\" /></figure>");
        }
        Block::List { ordered, items } => {
            out.push_str(if *ordered { "<ol>" } else { "<ul>" });
            for item in items {
                out.push_str("<li>");
                push_escaped(item, out);
                out.push_str("</li>");
            }
            out.push_str(if *ordered { "</ol>" } else { "</ul>" });
        }
        Block::Divider => out.push_str("<hr />"),
    }
}

fn push_escaped(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_paragraph_with_escaping() {
        let doc = vec![Block::Paragraph { text: "<b>hi</b>".into() }];
        assert_eq!(render_html(&doc, "/media"), "<p>&lt;b&gt;hi&lt;/b&gt;</p>");
    }

    #[test]
    fn renders_heading_with_clamped_level() {
        let doc = vec![Block::Heading { level: 9, text: "Big".into() }];
        assert_eq!(render_html(&doc, "/media"), "<h6>Big</h6>");
    }

    #[test]
    fn renders_image_with_media_url() {
        let doc = vec![Block::Image { media_id: "abc".into(), alt: Some("alt".into()) }];
        let html = render_html(&doc, "https://cdn.example.com/media/");
        assert!(html.contains("https://cdn.example.com/media/abc"));
        assert!(html.contains("alt=\"alt\""));
    }

    #[test]
    fn renders_ordered_list() {
        let doc = vec![Block::List { ordered: true, items: vec!["one".into(), "two".into()] }];
        let html = render_html(&doc, "/media");
        assert_eq!(html, "<ol><li>one</li><li>two</li></ol>");
    }

    #[test]
    fn renders_code_with_lang_class() {
        let doc = vec![Block::Code { lang: Some("rust".into()), code: "fn main(){}".into() }];
        let html = render_html(&doc, "/media");
        assert!(html.contains("class=\"language-rust\""));
        assert!(html.contains("fn main(){}"));
    }

    #[test]
    fn divider_renders() {
        let doc = vec![Block::Divider];
        assert_eq!(render_html(&doc, "/media"), "<hr />");
    }
}
