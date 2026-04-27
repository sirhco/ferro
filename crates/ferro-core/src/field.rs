use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use time::OffsetDateTime;

use crate::{error::CoreError, id::FieldId};

/// Logical field kinds supported by the editor + schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Text { multiline: bool, max: Option<usize> },
    RichText { format: RichFormat },
    Number { int: bool, min: Option<f64>, max: Option<f64> },
    Boolean,
    Date,
    DateTime,
    Enum { options: Vec<String> },
    Reference { to_type: String, multiple: bool },
    Media { multiple: bool, accept: Vec<String> },
    Json,
    Slug { source_field: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RichFormat {
    Markdown,
    ProseMirror,
    Html,
    /// Native Ferro block document — `Vec<Block>` serialized as JSON.
    Blocks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub id: FieldId,
    pub slug: String,
    pub name: String,
    pub help: Option<String>,
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub localized: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub hidden: bool,
}

/// Runtime field values — what gets stored in `Content.data`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    #[serde(with = "time::serde::rfc3339::option")]
    DateTime(Option<OffsetDateTime>),
    Array(Vec<FieldValue>),
    Object(Json),
}

impl FieldValue {
    pub fn validate_against(&self, def: &FieldDef) -> Result<(), CoreError> {
        use FieldKind as K;
        use FieldValue as V;

        if let V::Null = self {
            if def.required {
                return Err(CoreError::Validation(format!("field `{}` is required", def.slug)));
            }
            return Ok(());
        }

        match (&def.kind, self) {
            (K::Text { max, .. }, V::String(s)) => {
                if let Some(max) = max {
                    if s.len() > *max {
                        return Err(CoreError::Validation(format!(
                            "field `{}` exceeds {max} bytes",
                            def.slug
                        )));
                    }
                }
                Ok(())
            }
            (K::RichText { format: RichFormat::Blocks }, V::Object(_) | V::Array(_)) => Ok(()),
            (K::RichText { .. } | K::Slug { .. }, V::String(_)) => Ok(()),
            (K::Number { int, min, max }, V::Number(n)) => {
                if *int && n.fract() != 0.0 {
                    return Err(CoreError::Validation(format!(
                        "field `{}` must be integer",
                        def.slug
                    )));
                }
                if let Some(m) = min {
                    if n < m {
                        return Err(CoreError::Validation(format!(
                            "field `{}` below min",
                            def.slug
                        )));
                    }
                }
                if let Some(m) = max {
                    if n > m {
                        return Err(CoreError::Validation(format!(
                            "field `{}` above max",
                            def.slug
                        )));
                    }
                }
                Ok(())
            }
            (K::Boolean, V::Bool(_)) => Ok(()),
            (K::Date | K::DateTime, V::DateTime(_) | V::String(_)) => Ok(()),
            (K::Enum { options }, V::String(s)) => {
                if options.iter().any(|o| o == s) {
                    Ok(())
                } else {
                    Err(CoreError::Validation(format!("field `{}`: `{s}` not in enum", def.slug)))
                }
            }
            (K::Reference { multiple, .. }, v) => match (multiple, v) {
                (true, V::Array(_)) | (false, V::String(_)) => Ok(()),
                _ => Err(CoreError::Validation(format!(
                    "field `{}`: reference shape mismatch",
                    def.slug
                ))),
            },
            (K::Media { multiple, .. }, v) => match (multiple, v) {
                (true, V::Array(_)) | (false, V::String(_)) => Ok(()),
                _ => Err(CoreError::Validation(format!(
                    "field `{}`: media shape mismatch",
                    def.slug
                ))),
            },
            (K::Json, _) => Ok(()),
            _ => Err(CoreError::Validation(format!("field `{}`: type mismatch", def.slug))),
        }
    }
}
