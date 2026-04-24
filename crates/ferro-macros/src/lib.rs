//! Proc-macros for Ferro.
//!
//! `#[derive(ContentType)]` lets a Rust struct declare its schema once and
//! emit a `ContentTypeDef` via `MyType::ferro_content_type()`. Wiring this
//! into the schema registry is the job of the consuming crate.

use darling::FromDeriveInput;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(ferro), supports(struct_named))]
struct ContentTypeAttrs {
    ident: syn::Ident,
    #[darling(default)]
    slug: Option<String>,
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    singleton: bool,
}

#[proc_macro_derive(ContentType, attributes(ferro))]
pub fn derive_content_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = match ContentTypeAttrs::from_derive_input(&input) {
        Ok(v) => v,
        Err(e) => return e.write_errors().into(),
    };
    let ty_ident = &attrs.ident;
    let slug = attrs
        .slug
        .unwrap_or_else(|| to_kebab(&ty_ident.to_string()));
    let name = attrs.name.unwrap_or_else(|| ty_ident.to_string());
    let singleton = attrs.singleton;

    let expanded = quote! {
        impl #ty_ident {
            /// Compile-time-derived schema stub. The consuming crate should
            /// merge this with runtime `FieldDef` metadata (required/help/etc.).
            pub fn ferro_content_type_meta() -> (&'static str, &'static str, bool) {
                (#slug, #name, #singleton)
            }
        }
    };
    expanded.into()
}

fn to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
