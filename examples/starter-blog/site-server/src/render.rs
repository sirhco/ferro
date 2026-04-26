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
    ferro_editor::markdown::render_markdown(src)
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
