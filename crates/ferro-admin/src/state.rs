//! Cross-route reactive state. Signals provided once at the App root and
//! read by individual routes; matches the `state` object in the legacy SPA
//! one-for-one (current user, content types, toast).

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CurrentUser {
    pub id: String,
    pub email: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub roles: Vec<Value>,
    #[serde(default)]
    pub totp_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeSummary {
    pub id: String,
    pub site_id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<Value>,
    #[serde(default)]
    pub default_locale: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Ok,
    Err,
}

/// Bundle of cross-cutting reactive signals. Cloned into every component via
/// `expect_context::<AdminState>()`.
#[derive(Clone, Copy)]
pub struct AdminState {
    pub user: RwSignal<Option<CurrentUser>>,
    pub types: RwSignal<Vec<TypeSummary>>,
    pub toast: RwSignal<Option<Toast>>,
    /// Set to `true` after the initial `/me` + `/types` fetch resolves
    /// (success or 401). Used by routes to render their content vs. a
    /// loading spinner.
    pub bootstrapped: RwSignal<bool>,
}

impl AdminState {
    pub fn new() -> Self {
        Self {
            user: RwSignal::new(None),
            types: RwSignal::new(Vec::new()),
            toast: RwSignal::new(None),
            bootstrapped: RwSignal::new(false),
        }
    }

    pub fn set_toast_ok(&self, msg: impl Into<String>) {
        self.toast.set(Some(Toast { message: msg.into(), kind: ToastKind::Ok }));
    }

    pub fn set_toast_err(&self, msg: impl Into<String>) {
        self.toast.set(Some(Toast { message: msg.into(), kind: ToastKind::Err }));
    }
}

impl Default for AdminState {
    fn default() -> Self {
        Self::new()
    }
}
