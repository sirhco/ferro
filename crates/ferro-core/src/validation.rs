use std::sync::OnceLock;

use regex::Regex;

use crate::error::CoreError;

static SLUG_RE: OnceLock<Regex> = OnceLock::new();

fn slug_regex() -> &'static Regex {
    SLUG_RE.get_or_init(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").expect("slug regex compiles"))
}

pub fn validate_slug(s: &str) -> Result<(), CoreError> {
    if s.is_empty() || s.len() > 120 || !slug_regex().is_match(s) {
        return Err(CoreError::InvalidSlug(s.to_string()));
    }
    Ok(())
}

/// Slugify a human label → kebab-case ASCII slug. Non-ASCII is dropped.
#[must_use]
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = true;
    for ch in input.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_round_trip() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  multi   spaces  "), "multi-spaces");
        assert_eq!(slugify("already-slug"), "already-slug");
    }

    #[test]
    fn accept_good_slug() {
        assert!(validate_slug("hello-world").is_ok());
        assert!(validate_slug("a").is_ok());
    }

    #[test]
    fn reject_bad_slug() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug("Hello").is_err());
        assert!(validate_slug("-leading").is_err());
        assert!(validate_slug("trailing-").is_err());
        assert!(validate_slug("double--dash").is_err());
    }
}
