use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ferro_core::UserId;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::error::{AuthError, AuthResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user_id: UserId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl Session {
    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        now >= self.expires_at
    }
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn put(&self, session: Session) -> AuthResult<()>;
    async fn get(&self, token: &str) -> AuthResult<Option<Session>>;
    async fn delete(&self, token: &str) -> AuthResult<()>;
    async fn purge_expired(&self) -> AuthResult<u64>;
}

pub fn new_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    // URL-safe hex is fine; not decoded, just compared.
    let mut s = String::with_capacity(64);
    for b in buf {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

pub fn default_ttl() -> Duration {
    Duration::from_secs(60 * 60 * 24 * 14) // 14 days
}

/// In-memory session store. For single-process dev. Swap for a Redis/DB impl in prod.
#[derive(Debug, Default, Clone)]
pub struct MemorySessionStore {
    inner: Arc<RwLock<HashMap<String, Session>>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn put(&self, session: Session) -> AuthResult<()> {
        self.inner.write().await.insert(session.token.clone(), session);
        Ok(())
    }
    async fn get(&self, token: &str) -> AuthResult<Option<Session>> {
        let guard = self.inner.read().await;
        let Some(s) = guard.get(token).cloned() else {
            return Ok(None);
        };
        if s.is_expired(OffsetDateTime::now_utc()) {
            return Err(AuthError::SessionExpired);
        }
        Ok(Some(s))
    }
    async fn delete(&self, token: &str) -> AuthResult<()> {
        self.inner.write().await.remove(token);
        Ok(())
    }
    async fn purge_expired(&self) -> AuthResult<u64> {
        let now = OffsetDateTime::now_utc();
        let mut g = self.inner.write().await;
        let before = g.len();
        g.retain(|_, s| !s.is_expired(now));
        Ok((before - g.len()) as u64)
    }
}
