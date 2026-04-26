use leptos::prelude::*;

use super::model::{Block, BlockKind, Document};

#[component]
pub fn BlockEditor(#[prop(into)] doc: RwSignal<Document>) -> impl IntoView {
    let show_picker = RwSignal::new(false);
    let len = Memo::new(move |_| doc.with(|d| d.len()));

    let insert = move |kind: BlockKind| {
        doc.update(|blocks| blocks.push(Block::empty(kind)));
        show_picker.set(false);
    };

    let indices = move || {
        let n = len.get();
        let mut v: Vec<usize> = Vec::with_capacity(n);
        for i in 0..n {
            v.push(i);
        }
        v
    };

    view! {
        <div class="ferro-blocks">
            <For each=indices key=|i| *i let:i>
                <BlockRow idx=i doc=doc />
            </For>

            <div class="ferro-blocks-footer">
                <button
                    class="ferro-ghost"
                    on:click=move |_| show_picker.update(|v| *v = !*v)
                >
                    {move || if show_picker.get() { "Cancel" } else { "+ Add block" }}
                </button>
                <Show when=move || show_picker.get() fallback=|| view! { <span></span> }>
                    <div class="ferro-block-picker">
                        {BlockKind::all().iter().copied().map(|k| {
                            view! {
                                <button
                                    class="ferro-block-picker-item"
                                    on:click=move |_| insert(k)
                                >
                                    {k.label()}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[component]
fn BlockRow(idx: usize, doc: RwSignal<Document>) -> impl IntoView {
    let label = move || doc.with(|d| d.get(idx).map(|b| b.label().to_string()).unwrap_or_default());

    let move_up = move |_| {
        doc.update(|blocks| {
            if idx > 0 && idx < blocks.len() {
                blocks.swap(idx - 1, idx);
            }
        });
    };
    let move_down = move |_| {
        doc.update(|blocks| {
            if idx + 1 < blocks.len() {
                blocks.swap(idx, idx + 1);
            }
        });
    };
    let remove = move |_| {
        doc.update(|blocks| {
            if idx < blocks.len() {
                blocks.remove(idx);
            }
        });
    };

    view! {
        <div class="ferro-block-row">
            <div class="ferro-block-handle">
                <span class="ferro-block-kind">{label}</span>
                <button class="ferro-block-btn" title="Move up" on:click=move_up>"↑"</button>
                <button class="ferro-block-btn" title="Move down" on:click=move_down>"↓"</button>
                <button class="ferro-block-btn ferro-block-del" title="Remove" on:click=remove>"×"</button>
            </div>
            <div class="ferro-block-body">
                {move || render_block_inputs(idx, doc)}
            </div>
        </div>
    }
}

fn render_block_inputs(idx: usize, doc: RwSignal<Document>) -> AnyView {
    let current = doc.with(|blocks| blocks.get(idx).cloned());
    let Some(block) = current else {
        return view! { <span></span> }.into_any();
    };

    match block {
        Block::Paragraph { text } => view! {
            <textarea
                class="ferro-block-input"
                rows="2"
                prop:value=text
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    doc.update(|blocks| {
                        if let Some(Block::Paragraph { text }) = blocks.get_mut(idx) {
                            *text = v;
                        }
                    });
                }
            />
        }
        .into_any(),

        Block::Heading { level, text } => view! {
            <div class="ferro-block-heading">
                <select
                    class="ferro-block-level"
                    on:change=move |ev| {
                        let new_level: u8 = event_target_value(&ev).parse().unwrap_or(2);
                        doc.update(|blocks| {
                            if let Some(Block::Heading { level, .. }) = blocks.get_mut(idx) {
                                *level = new_level.clamp(1, 3);
                            }
                        });
                    }
                >
                    <option value="1" selected=level == 1>"H1"</option>
                    <option value="2" selected=level == 2>"H2"</option>
                    <option value="3" selected=level == 3>"H3"</option>
                </select>
                <input
                    class="ferro-block-input"
                    type="text"
                    prop:value=text
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Heading { text, .. }) = blocks.get_mut(idx) {
                                *text = v;
                            }
                        });
                    }
                />
            </div>
        }
        .into_any(),

        Block::Quote { text, cite } => view! {
            <div class="ferro-block-quote">
                <textarea
                    class="ferro-block-input"
                    rows="2"
                    prop:value=text
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Quote { text, .. }) = blocks.get_mut(idx) {
                                *text = v;
                            }
                        });
                    }
                />
                <input
                    class="ferro-block-input"
                    type="text"
                    placeholder="cite (optional)"
                    prop:value=cite.unwrap_or_default()
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Quote { cite, .. }) = blocks.get_mut(idx) {
                                *cite = if v.is_empty() { None } else { Some(v) };
                            }
                        });
                    }
                />
            </div>
        }
        .into_any(),

        Block::Code { lang, code } => view! {
            <div class="ferro-block-code">
                <input
                    class="ferro-block-input"
                    type="text"
                    placeholder="language (rust, ts, …)"
                    prop:value=lang.unwrap_or_default()
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Code { lang, .. }) = blocks.get_mut(idx) {
                                *lang = if v.is_empty() { None } else { Some(v) };
                            }
                        });
                    }
                />
                <textarea
                    class="ferro-block-input ferro-block-codearea"
                    rows="6"
                    prop:value=code
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Code { code, .. }) = blocks.get_mut(idx) {
                                *code = v;
                            }
                        });
                    }
                />
            </div>
        }
        .into_any(),

        Block::Image { media_id, alt } => view! {
            <div class="ferro-block-image">
                <input
                    class="ferro-block-input"
                    type="text"
                    placeholder="media id"
                    prop:value=media_id
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Image { media_id, .. }) = blocks.get_mut(idx) {
                                *media_id = v;
                            }
                        });
                    }
                />
                <input
                    class="ferro-block-input"
                    type="text"
                    placeholder="alt text"
                    prop:value=alt.unwrap_or_default()
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        doc.update(|blocks| {
                            if let Some(Block::Image { alt, .. }) = blocks.get_mut(idx) {
                                *alt = if v.is_empty() { None } else { Some(v) };
                            }
                        });
                    }
                />
            </div>
        }
        .into_any(),

        Block::List { ordered, items } => {
            let items_text = items.join("\n");
            view! {
                <div class="ferro-block-list">
                    <label class="ferro-block-list-toggle">
                        <input
                            type="checkbox"
                            prop:checked=ordered
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                doc.update(|blocks| {
                                    if let Some(Block::List { ordered, .. }) = blocks.get_mut(idx) {
                                        *ordered = checked;
                                    }
                                });
                            }
                        />
                        <span>"Ordered"</span>
                    </label>
                    <textarea
                        class="ferro-block-input"
                        rows="4"
                        placeholder="One item per line"
                        prop:value=items_text
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            let new_items: Vec<String> = v
                                .lines()
                                .map(|s| s.to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            doc.update(|blocks| {
                                if let Some(Block::List { items, .. }) = blocks.get_mut(idx) {
                                    *items = new_items;
                                }
                            });
                        }
                    />
                </div>
            }
            .into_any()
        }

        Block::Divider => view! {
            <hr class="ferro-block-divider-preview" />
        }
        .into_any(),
    }
}
