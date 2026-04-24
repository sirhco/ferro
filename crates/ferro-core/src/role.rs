use serde::{Deserialize, Serialize};

use crate::id::{ContentTypeId, RoleId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Permission {
    Read(Scope),
    Write(Scope),
    Publish(Scope),
    ManageUsers,
    ManageSchema,
    ManagePlugins,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum Scope {
    Global,
    Type { id: ContentTypeId },
    Own,
}
