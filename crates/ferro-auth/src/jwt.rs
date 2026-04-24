use ferro_core::UserId;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{AuthError, AuthResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub iss: String,
    pub exp: i64,
    pub iat: i64,
    pub roles: Vec<String>,
}

impl JwtClaims {
    pub fn user_id(&self) -> AuthResult<UserId> {
        self.sub
            .parse()
            .map_err(|_| AuthError::Hash("invalid sub claim".into()))
    }
}

pub struct JwtManager {
    issuer: String,
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
}

impl JwtManager {
    /// HMAC secret. For prod, swap to RS256/ES256 with a private key.
    pub fn hs256(issuer: impl Into<String>, secret: &[u8]) -> Self {
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        let issuer = issuer.into();
        validation.set_issuer(&[issuer.clone()]);
        Self {
            issuer,
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation,
        }
    }

    pub fn mint(&self, user: UserId, roles: Vec<String>, ttl_secs: i64) -> AuthResult<String> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let claims = JwtClaims {
            sub: user.to_string(),
            iss: self.issuer.clone(),
            exp: now + ttl_secs,
            iat: now,
            roles,
        };
        Ok(encode(&Header::default(), &claims, &self.encoding)?)
    }

    pub fn verify(&self, token: &str) -> AuthResult<JwtClaims> {
        Ok(decode::<JwtClaims>(token, &self.decoding, &self.validation)?.claims)
    }
}
