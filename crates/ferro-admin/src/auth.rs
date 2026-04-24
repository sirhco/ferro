use serde::{Deserialize, Serialize};

/// Lightweight representation of the authenticated user sent from server to
/// client via a `Resource`. Password hashes never cross this boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthedUser {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub roles: Vec<String>,
}
