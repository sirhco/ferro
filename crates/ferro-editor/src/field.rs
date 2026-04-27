use ferro_core::{FieldDef, FieldKind, FieldValue, RichFormat};
use leptos::prelude::*;

use crate::{
    blocks::{BlockEditor, Document},
    markdown::MarkdownEditor,
};

/// A generic field editor that dispatches on the field definition's kind.
#[component]
pub fn FieldEditor(
    #[prop(into)] def: Signal<FieldDef>,
    #[prop(into)] value: Signal<FieldValue>,
    #[prop(into)] on_change: Callback<FieldValue>,
) -> impl IntoView {
    move || {
        let def = def.get();
        let v = value.get();
        match &def.kind {
            FieldKind::Text { multiline, .. } => {
                view! {
                    <TextField def=def.clone() multiline=*multiline value=v.clone() on_change=on_change />
                }
                .into_any()
            }
            FieldKind::RichText { format: RichFormat::Markdown } => {
                let current = match v.clone() {
                    FieldValue::String(s) => s,
                    _ => String::new(),
                };
                let sig = RwSignal::new(current);
                Effect::new(move |_| {
                    on_change.run(FieldValue::String(sig.get()));
                });
                view! {
                    <MarkdownEditor value=sig on_change=Callback::new(move |s: String| sig.set(s)) />
                }
                .into_any()
            }
            FieldKind::Boolean => view! {
                <BoolField value=v.clone() on_change=on_change />
            }
            .into_any(),
            FieldKind::Number { int, min, max } => view! {
                <NumberField
                    integer=*int
                    min=*min
                    max=*max
                    value=v.clone()
                    on_change=on_change
                />
            }
            .into_any(),
            FieldKind::Date => view! {
                <DateField html_type="date" value=v.clone() on_change=on_change />
            }
            .into_any(),
            FieldKind::DateTime => view! {
                <DateField html_type="datetime-local" value=v.clone() on_change=on_change />
            }
            .into_any(),
            FieldKind::Enum { options } => view! {
                <EnumField options=options.clone() value=v.clone() on_change=on_change />
            }
            .into_any(),
            FieldKind::Slug { .. } => view! {
                <SlugField value=v.clone() on_change=on_change />
            }
            .into_any(),
            FieldKind::Json => view! {
                <JsonField value=v.clone() on_change=on_change />
            }
            .into_any(),
            FieldKind::Reference { to_type, multiple } => view! {
                <IdRefField
                    label=format!("ref → {to_type}")
                    multiple=*multiple
                    value=v.clone()
                    on_change=on_change
                />
            }
            .into_any(),
            FieldKind::Media { multiple, .. } => view! {
                <IdRefField
                    label="media".into()
                    multiple=*multiple
                    value=v.clone()
                    on_change=on_change
                />
            }
            .into_any(),
            FieldKind::RichText { format: RichFormat::Blocks } => {
                let initial: Document = match v.clone() {
                    FieldValue::Object(j) => serde_json::from_value(j).unwrap_or_default(),
                    FieldValue::Array(_) => Vec::new(),
                    _ => Vec::new(),
                };
                let sig = RwSignal::new(initial);
                Effect::new(move |_| {
                    let val = serde_json::to_value(sig.get())
                        .unwrap_or(serde_json::Value::Array(Vec::new()));
                    on_change.run(FieldValue::Object(val));
                });
                view! { <BlockEditor doc=sig /> }.into_any()
            }
            FieldKind::RichText { .. } => view! {
                <p class="ferro-editor-todo">
                    {format!("editor todo: rich-text format {:?}", def.kind)}
                </p>
            }
            .into_any(),
        }
    }
}

#[component]
fn TextField(
    def: FieldDef,
    multiline: bool,
    value: FieldValue,
    on_change: Callback<FieldValue>,
) -> impl IntoView {
    let current = match value {
        FieldValue::String(s) => s,
        _ => String::new(),
    };
    let placeholder = def.help.clone().unwrap_or_default();
    if multiline {
        view! {
            <textarea
                class="ferro-text-input"
                prop:value=current
                placeholder=placeholder
                on:input=move |ev| on_change.run(FieldValue::String(event_target_value(&ev)))
            />
        }
        .into_any()
    } else {
        view! {
            <input
                class="ferro-text-input"
                type="text"
                prop:value=current
                placeholder=placeholder
                on:input=move |ev| on_change.run(FieldValue::String(event_target_value(&ev)))
            />
        }
        .into_any()
    }
}

#[component]
fn BoolField(value: FieldValue, on_change: Callback<FieldValue>) -> impl IntoView {
    let checked = matches!(value, FieldValue::Bool(true));
    view! {
        <input
            type="checkbox"
            prop:checked=checked
            on:change=move |ev| on_change.run(FieldValue::Bool(event_target_checked(&ev)))
        />
    }
}

#[component]
fn NumberField(
    integer: bool,
    min: Option<f64>,
    max: Option<f64>,
    value: FieldValue,
    on_change: Callback<FieldValue>,
) -> impl IntoView {
    let current = match value {
        FieldValue::Number(n) => n.to_string(),
        _ => String::new(),
    };
    let step = if integer { "1".to_string() } else { "any".to_string() };
    let min_attr = min.map(|m| m.to_string());
    let max_attr = max.map(|m| m.to_string());
    view! {
        <input
            class="ferro-number-input"
            type="number"
            prop:value=current
            step=step
            min=min_attr
            max=max_attr
            on:input=move |ev| {
                let raw = event_target_value(&ev);
                match raw.parse::<f64>() {
                    Ok(n) => on_change.run(FieldValue::Number(n)),
                    Err(_) if raw.is_empty() => on_change.run(FieldValue::Null),
                    Err(_) => {}
                }
            }
        />
    }
}

#[component]
fn DateField(
    html_type: &'static str,
    value: FieldValue,
    on_change: Callback<FieldValue>,
) -> impl IntoView {
    let current = match value {
        FieldValue::String(s) => s,
        _ => String::new(),
    };
    view! {
        <input
            class="ferro-date-input"
            type=html_type
            prop:value=current
            on:input=move |ev| {
                let raw = event_target_value(&ev);
                if raw.is_empty() {
                    on_change.run(FieldValue::Null);
                } else {
                    on_change.run(FieldValue::String(raw));
                }
            }
        />
    }
}

#[component]
fn EnumField(
    options: Vec<String>,
    value: FieldValue,
    on_change: Callback<FieldValue>,
) -> impl IntoView {
    let current = match value {
        FieldValue::String(s) => s,
        _ => String::new(),
    };
    view! {
        <select
            class="ferro-enum-input"
            on:change=move |ev| on_change.run(FieldValue::String(event_target_value(&ev)))
        >
            <option value="" selected=current.is_empty()>"— choose —"</option>
            {options
                .into_iter()
                .map(|opt| {
                    let selected = opt == current;
                    let label = opt.clone();
                    view! {
                        <option value=opt selected=selected>{label}</option>
                    }
                })
                .collect_view()}
        </select>
    }
}

#[component]
fn SlugField(value: FieldValue, on_change: Callback<FieldValue>) -> impl IntoView {
    let current = match value {
        FieldValue::String(s) => s,
        _ => String::new(),
    };
    view! {
        <input
            class="ferro-slug-input"
            type="text"
            prop:value=current
            pattern="[a-z0-9]+(-[a-z0-9]+)*"
            on:input=move |ev| on_change.run(FieldValue::String(event_target_value(&ev)))
        />
    }
}

#[component]
fn JsonField(value: FieldValue, on_change: Callback<FieldValue>) -> impl IntoView {
    let current = match &value {
        FieldValue::Object(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
        FieldValue::String(s) => s.clone(),
        FieldValue::Null => String::new(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    let invalid = RwSignal::new(false);
    view! {
        <div class="ferro-json-input">
            <textarea
                class="ferro-text-input ferro-json-textarea"
                prop:value=current
                on:input=move |ev| {
                    let raw = event_target_value(&ev);
                    if raw.trim().is_empty() {
                        invalid.set(false);
                        on_change.run(FieldValue::Null);
                        return;
                    }
                    match serde_json::from_str::<serde_json::Value>(&raw) {
                        Ok(v) => {
                            invalid.set(false);
                            on_change.run(FieldValue::Object(v));
                        }
                        Err(_) => invalid.set(true),
                    }
                }
            />
            <span class="ferro-json-status" class:invalid=move || invalid.get()>
                {move || if invalid.get() { "invalid json" } else { "ok" }}
            </span>
        </div>
    }
}

#[component]
fn IdRefField(
    label: String,
    multiple: bool,
    value: FieldValue,
    on_change: Callback<FieldValue>,
) -> impl IntoView {
    let current = match &value {
        FieldValue::String(s) => s.clone(),
        FieldValue::Array(items) => items
            .iter()
            .filter_map(|v| match v {
                FieldValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    };
    let placeholder = if multiple { "comma-separated ids".to_string() } else { "id".to_string() };
    view! {
        <label class="ferro-idref">
            <span class="ferro-idref-label">{label}</span>
            <input
                class="ferro-text-input"
                type="text"
                prop:value=current
                placeholder=placeholder
                on:input=move |ev| {
                    let raw = event_target_value(&ev);
                    if multiple {
                        let items: Vec<FieldValue> = raw
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| FieldValue::String(s.to_string()))
                            .collect();
                        on_change.run(FieldValue::Array(items));
                    } else if raw.is_empty() {
                        on_change.run(FieldValue::Null);
                    } else {
                        on_change.run(FieldValue::String(raw));
                    }
                }
            />
        </label>
    }
}
