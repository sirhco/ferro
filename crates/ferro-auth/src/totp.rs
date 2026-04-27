//! RFC 6238 TOTP (Time-based One-Time Password).
//!
//! Defaults match what mainstream authenticator apps (Google Authenticator,
//! Authy, 1Password, Bitwarden) accept out of the box: 6 digits, 30-second
//! step, HMAC-SHA1, drift window of ±1 step.
//!
//! Secrets are kept as base32 strings — that's the on-the-wire form for QR
//! codes and `otpauth://` URIs, and it's what we persist.

use base32::Alphabet;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use time::OffsetDateTime;

const STEP_SECS: u64 = 30;
const DIGITS: u32 = 6;
const DEFAULT_WINDOW: i64 = 1;

/// Generate 160 bits of random secret and encode as base32 (RFC 4648, no
/// padding) — the `secret=` value of an `otpauth://` URI.
#[must_use]
pub fn generate_secret() -> String {
    let mut buf = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut buf);
    base32::encode(Alphabet::Rfc4648 { padding: false }, &buf)
}

/// Build the standard `otpauth://totp/<issuer>:<account>?secret=…&issuer=…`
/// URI. Authenticator apps render a QR code from this.
#[must_use]
pub fn otpauth_uri(secret: &str, account: &str, issuer: &str) -> String {
    let label = format!("{}:{}", url_encode(issuer), url_encode(account),);
    format!(
        "otpauth://totp/{label}?secret={secret}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        url_encode(issuer)
    )
}

/// Generate the 6-digit code valid at `at` for the given base32 secret.
/// Returns `None` if the secret can't be decoded.
#[must_use]
pub fn generate(secret_b32: &str, at: OffsetDateTime) -> Option<String> {
    let key = base32::decode(Alphabet::Rfc4648 { padding: false }, secret_b32)?;
    let counter = step_counter(at);
    Some(format_code(&key, counter))
}

/// Verify a user-supplied code against `secret_b32`. Accepts ±`DEFAULT_WINDOW`
/// steps to tolerate clock drift (and for tests that need to pre-mint codes).
#[must_use]
pub fn verify(secret_b32: &str, code: &str, at: OffsetDateTime) -> bool {
    verify_with_window(secret_b32, code, at, DEFAULT_WINDOW)
}

/// Variant of [`verify`] that lets the caller widen the drift window. The
/// REST layer uses `1` (matches Google Authenticator's tolerance); test
/// vectors pin to `0` to assert exact-step behavior.
#[must_use]
pub fn verify_with_window(secret_b32: &str, code: &str, at: OffsetDateTime, window: i64) -> bool {
    let Some(key) = base32::decode(Alphabet::Rfc4648 { padding: false }, secret_b32) else {
        return false;
    };
    let center = step_counter(at) as i64;
    for delta in -window..=window {
        let counter = (center + delta).max(0) as u64;
        if format_code(&key, counter).as_str() == code {
            return true;
        }
    }
    false
}

fn step_counter(at: OffsetDateTime) -> u64 {
    let secs = at.unix_timestamp().max(0) as u64;
    secs / STEP_SECS
}

/// HMAC-SHA1 over the 8-byte counter, then dynamic-truncate per RFC 4226 §5.4.
fn format_code(key: &[u8], counter: u64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("hmac key length is always valid");
    mac.update(&counter.to_be_bytes());
    let hash = mac.finalize().into_bytes();
    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let bin = ((u32::from(hash[offset] & 0x7f)) << 24)
        | ((u32::from(hash[offset + 1])) << 16)
        | ((u32::from(hash[offset + 2])) << 8)
        | u32::from(hash[offset + 3]);
    let modulus = 10u32.pow(DIGITS);
    format!("{:0width$}", bin % modulus, width = DIGITS as usize)
}

/// Minimal percent-encoder for the URI label/issuer fields. Stays narrow on
/// purpose — a full URL crate would drag in transitive deps for one helper.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 reference: secret "12345678901234567890" (ASCII), counter
    /// 59 / step 1 → code 287082.
    #[test]
    fn rfc_6238_reference_vector() {
        let secret_ascii = b"12345678901234567890";
        let secret_b32 = base32::encode(Alphabet::Rfc4648 { padding: false }, secret_ascii);
        let at = OffsetDateTime::from_unix_timestamp(59).unwrap();
        assert_eq!(generate(&secret_b32, at).unwrap(), "287082");
    }

    #[test]
    fn verify_accepts_drift_within_window() {
        let secret = generate_secret();
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let code = generate(&secret, now).unwrap();
        // Same step
        assert!(verify(&secret, &code, now));
        // One step earlier — within window.
        let before = now - time::Duration::seconds(STEP_SECS as i64);
        assert!(verify(&secret, &code, before));
        // Two steps later — outside window.
        let after = now + time::Duration::seconds(STEP_SECS as i64 * 2);
        assert!(!verify(&secret, &code, after));
    }

    #[test]
    fn verify_rejects_wrong_code() {
        let secret = generate_secret();
        let now = OffsetDateTime::now_utc();
        assert!(!verify(&secret, "000000", now));
    }

    #[test]
    fn otpauth_uri_format() {
        let uri = otpauth_uri("JBSWY3DPEHPK3PXP", "alice@example.com", "Ferro CMS");
        assert!(uri.starts_with("otpauth://totp/Ferro%20CMS:alice%40example.com?"));
        assert!(uri.contains("secret=JBSWY3DPEHPK3PXP"));
        assert!(uri.contains("issuer=Ferro%20CMS"));
    }
}
