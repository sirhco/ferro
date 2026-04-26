use ferro_editor::{render_blocks_html, Document};
use serde_json::Value;

pub fn render_blocks(value: Option<&Value>, media_base: &str) -> String {
    let Some(v) = value else {
        return String::new();
    };
    let Ok(doc) = serde_json::from_value::<Document>(v.clone()) else {
        return String::new();
    };
    render_blocks_html(&doc, media_base)
}

pub fn render_markdown_basic(src: &str) -> String {
    let mut out = String::new();
    for paragraph in src.split("\n\n") {
        let trimmed = paragraph.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push_str("<h1>");
            push_escaped(rest, &mut out);
            out.push_str("</h1>");
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push_str("<h2>");
            push_escaped(rest, &mut out);
            out.push_str("</h2>");
        } else if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push_str("<h3>");
            push_escaped(rest, &mut out);
            out.push_str("</h3>");
        } else if trimmed.lines().all(|l| l.trim_start().starts_with("- ")) {
            out.push_str("<ul>");
            for line in trimmed.lines() {
                let item = line.trim_start().trim_start_matches("- ");
                out.push_str("<li>");
                push_escaped(item, &mut out);
                out.push_str("</li>");
            }
            out.push_str("</ul>");
        } else {
            out.push_str("<p>");
            push_escaped(trimmed, &mut out);
            out.push_str("</p>");
        }
    }
    out
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

pub fn currency_format(price_cents: i64, currency: &str) -> String {
    let dollars = price_cents as f64 / 100.0;
    let symbol = match currency {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        _ => "",
    };
    if symbol.is_empty() {
        format!("{dollars:.2} {currency}")
    } else {
        format!("{symbol}{dollars:.2}")
    }
}

pub fn humanize_date(s: &str) -> String {
    s.split('T').next().unwrap_or(s).to_string()
}
