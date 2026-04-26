//! Rich-text + field editor components. Each is an island — imported into
//! admin pages and hydrated independently so editing an article does not pull
//! in the schema-builder bundle.

#![deny(rust_2018_idioms)]

pub mod blocks;
pub mod field;
pub mod markdown;
pub mod toolbar;

pub use blocks::{render_html as render_blocks_html, Block, BlockEditor, Document};
pub use field::FieldEditor;
pub use markdown::MarkdownEditor;
