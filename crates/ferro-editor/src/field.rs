use ferro_core::{FieldDef, FieldKind, FieldValue, RichFormat};
use leptos::prelude::*;

use crate::markdown::MarkdownEditor;

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
                view! { <TextField def=def.clone() multiline=*multiline value=v.clone() on_change=on_change /> }
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
                view! { <MarkdownEditor value=sig on_change=Callback::new(move |s: String| sig.set(s)) /> }
                    .into_any()
            }
            FieldKind::Boolean => view! {
                <BoolField value=v.clone() on_change=on_change />
            }
            .into_any(),
            _ => view! { <p class="ferro-editor-todo">{format!("editor todo: {:?}", def.kind)}</p> }
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
