//! Link-token verification to detect browser clients.

use std::time::Duration;

use moka::sync::Cache;

use crate::config::{DEFAULT_STATE_CAPACITY, DEFAULT_STATE_IDLE_SECONDS};

/// Check if a presented token matches the active token.
pub fn token_is_valid(presented: Option<&str>, current: &str) -> bool {
    presented == Some(current)
}

/// Verifies client networks based on link-token challenge responses.
#[derive(Debug)]
pub struct LinkTokenVerifier {
    /// The currently active challenge token.
    token: String,
    /// Client-network keys that have presented a valid token.
    verified: Cache<String, ()>,
}

impl LinkTokenVerifier {
    /// Create a verifier with the given active challenge `token`.
    pub fn new(token: impl Into<String>) -> Self {
        Self::with_limits(
            token,
            DEFAULT_STATE_CAPACITY,
            Duration::from_secs(DEFAULT_STATE_IDLE_SECONDS),
        )
    }

    pub fn with_limits(token: impl Into<String>, max_entries: u64, idle_timeout: Duration) -> Self {
        Self {
            token: token.into(),
            verified: Cache::builder()
                .max_capacity(max_entries.max(1))
                .time_to_idle(idle_timeout)
                .build(),
        }
    }

    /// The currently active challenge token (embedded in the client CSS URL).
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Record a ping for a network if the token is valid.
    pub fn ping(&self, network_key: &str, presented: Option<&str>) -> bool {
        if !token_is_valid(presented, &self.token) {
            return false;
        }
        self.verified.insert(network_key.to_string(), ());
        true
    }

    /// Whether `network_key` is a verified browser (has a valid ping).
    pub fn is_verified(&self, network_key: &str) -> bool {
        self.verified.contains_key(network_key)
    }

    /// Check if a request from a network is suspicious (unverified).
    pub fn is_suspicious(&self, network_key: &str, presented: Option<&str>) -> bool {
        if self.is_verified(network_key) {
            return false;
        }
        if self.ping(network_key, presented) {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_active_token_is_valid() {
        assert!(token_is_valid(Some("abc123"), "abc123"));
        assert!(!token_is_valid(Some("nope"), "abc123"));
        assert!(!token_is_valid(None, "abc123"));
    }

    #[test]
    fn unverified_network_is_suspicious() {
        let verifier = LinkTokenVerifier::new("secret");
        assert!(verifier.is_suspicious("203.0.113.0/32", None));
        assert!(!verifier.is_verified("203.0.113.0/32"));
    }

    #[test]
    fn valid_token_verifies_and_clears_suspicion() {
        let verifier = LinkTokenVerifier::new("secret");
        assert!(!verifier.is_suspicious("203.0.113.0/32", Some("secret")));
        assert!(verifier.is_verified("203.0.113.0/32"));
        assert!(!verifier.is_suspicious("203.0.113.0/32", None));
    }

    #[test]
    fn invalid_token_does_not_verify() {
        let verifier = LinkTokenVerifier::new("secret");
        assert!(!verifier.ping("203.0.113.0/32", Some("wrong")));
        assert!(!verifier.is_verified("203.0.113.0/32"));
        assert!(verifier.is_suspicious("203.0.113.0/32", Some("wrong")));
    }
}
