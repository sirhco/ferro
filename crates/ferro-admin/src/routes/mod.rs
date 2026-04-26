pub mod content_edit;
pub mod content_list;
pub mod dashboard;
pub mod layout;
pub mod login;
pub mod media;
pub mod plugins;
pub mod schema;
pub mod settings;
pub mod users;

#[cfg(feature = "hydrate")]
use crate::api;
use crate::state::AdminState;

/// Hit `/me` + `/types` once the App has mounted. Populates `AdminState` so
/// the rest of the routes render immediately. Failure is silent — routes
/// detect a missing `user` and redirect to `/admin/login`.
#[cfg(feature = "hydrate")]
pub async fn bootstrap_after_mount(state: AdminState) {
    use crate::state::CurrentUser;
    use leptos::prelude::*;

    if api::get_token().is_none() {
        state.bootstrapped.set(true);
        return;
    }
    match api::get::<serde_json::Value>("/api/v1/auth/me").await {
        Ok(me) => {
            if let Ok(mut user) = serde_json::from_value::<CurrentUser>(me.clone()) {
                user.totp_enabled = me.get("totp_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                state.user.set(Some(user));
            }
        }
        Err(_) => {
            api::clear_tokens();
        }
    }
    if let Ok(types) = api::get::<Vec<crate::state::TypeSummary>>("/api/v1/types").await {
        state.types.set(types);
    }
    state.bootstrapped.set(true);
}

#[cfg(not(feature = "hydrate"))]
pub async fn bootstrap_after_mount(_state: AdminState) {}
