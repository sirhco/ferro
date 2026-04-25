use std::sync::Arc;

use ferro_core::User;
use ferro_storage::Repository;
use time::{Duration, OffsetDateTime};

use crate::error::{AuthError, AuthResult};
use crate::password::{hash_password, verify_password};
use crate::session::{default_ttl, new_token, Session, SessionStore};

pub struct AuthService {
    pub repo: Arc<dyn Repository>,
    pub sessions: Arc<dyn SessionStore>,
}

impl AuthService {
    pub fn new(repo: Arc<dyn Repository>, sessions: Arc<dyn SessionStore>) -> Self {
        Self { repo, sessions }
    }

    /// Register a new user. Panics (in debug) if password is weaker than the min policy.
    pub async fn register(
        &self,
        email: &str,
        handle: &str,
        display_name: Option<String>,
        password: &str,
    ) -> AuthResult<User> {
        debug_assert!(password.len() >= 8, "password policy enforced at form layer");
        let hash = hash_password(password)?;
        let user = User {
            id: ferro_core::UserId::new(),
            email: email.to_string(),
            handle: handle.to_string(),
            display_name,
            password_hash: Some(hash),
            roles: Vec::new(),
            active: true,
            created_at: OffsetDateTime::now_utc(),
            last_login: None,
            password_changed_at: None,
        };
        let user = self.repo.users().upsert(user).await?;
        Ok(user)
    }

    pub async fn login(
        &self,
        email: &str,
        password: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> AuthResult<(User, Session)> {
        let Some(user) = self.repo.users().by_email(email).await? else {
            return Err(AuthError::InvalidCredentials);
        };
        if !user.active {
            return Err(AuthError::AccountDisabled);
        }
        let Some(hash) = user.password_hash.as_deref() else {
            return Err(AuthError::InvalidCredentials);
        };
        if !verify_password(password, hash)? {
            return Err(AuthError::InvalidCredentials);
        }

        let now = OffsetDateTime::now_utc();
        let ttl = Duration::seconds(default_ttl().as_secs() as i64);
        let session = Session {
            token: new_token(),
            user_id: user.id,
            created_at: now,
            expires_at: now + ttl,
            ip,
            user_agent,
        };
        self.sessions.put(session.clone()).await?;
        Ok((user, session))
    }

    pub async fn logout(&self, token: &str) -> AuthResult<()> {
        self.sessions.delete(token).await
    }

    pub async fn resolve_session(&self, token: &str) -> AuthResult<(Session, User)> {
        let session = self.sessions.get(token).await?.ok_or(AuthError::SessionNotFound)?;
        let user = self
            .repo
            .users()
            .get(session.user_id)
            .await?
            .ok_or(AuthError::SessionNotFound)?;
        Ok((session, user))
    }
}
