use ferro_core::{ContentTypeId, Permission, Role, Scope, UserId};

use crate::error::{AuthError, AuthResult};

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

impl AuthContext {
    pub fn has_permission(&self, want: &Permission) -> bool {
        self.roles.iter().any(|r| r.permissions.iter().any(|p| perm_covers(p, want)))
    }
}

fn perm_covers(granted: &Permission, wanted: &Permission) -> bool {
    if matches!(granted, Permission::Admin) {
        return true;
    }
    match (granted, wanted) {
        (Permission::Read(a), Permission::Read(b)) => scope_covers(a, b),
        (Permission::Write(a), Permission::Write(b)) => scope_covers(a, b),
        (Permission::Publish(a), Permission::Publish(b)) => scope_covers(a, b),
        (Permission::Write(a), Permission::Read(b)) => scope_covers(a, b),
        (Permission::Publish(a), Permission::Read(b) | Permission::Write(b)) => scope_covers(a, b),
        (Permission::ManageUsers, Permission::ManageUsers)
        | (Permission::ManageSchema, Permission::ManageSchema)
        | (Permission::ManagePlugins, Permission::ManagePlugins) => true,
        _ => false,
    }
}

fn scope_covers(granted: &Scope, wanted: &Scope) -> bool {
    match (granted, wanted) {
        (Scope::Global, _) => true,
        (Scope::Type { id: a }, Scope::Type { id: b }) => a == b,
        (Scope::Own, Scope::Own) => true,
        _ => false,
    }
}

pub fn authorize(ctx: &AuthContext, want: Permission) -> AuthResult<()> {
    if ctx.has_permission(&want) {
        Ok(())
    } else {
        Err(AuthError::Forbidden)
    }
}

#[must_use]
pub fn require_read_type(id: ContentTypeId) -> Permission {
    Permission::Read(Scope::Type { id })
}
