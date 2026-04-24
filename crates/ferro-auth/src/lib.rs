//! Baked-in authentication: argon2 password hashing, sessions, JWT, RBAC.

#![deny(rust_2018_idioms, unreachable_pub)]

pub mod error;
pub mod jwt;
pub mod password;
pub mod policy;
pub mod service;
pub mod session;

pub use error::{AuthError, AuthResult};
pub use jwt::{JwtClaims, JwtManager};
pub use password::{hash_password, verify_password};
pub use policy::{authorize, AuthContext};
pub use service::AuthService;
pub use session::{Session, SessionStore};
