//! Bearer-credential issuance for the API (spec §7: "capability token as
//! bearer credential").
//!
//! archon's [`Capability`] is deliberately unforgeable — its revocation
//! handle is a private `serial` that no external code can fabricate. Because
//! the archon bridge does not (and should not) serialize that secret, the
//! capability stays *inside* the service boundary. What crosses the wire is an
//! opaque bearer value the API mints and keeps in a registry, bound to the
//! underlying [`ActionToken`]. Presenting it is possession of a token; the
//! authority's `verify` (scope, expiry, action, capability liveness) is still
//! the enforcement gate, so revocation is live.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tpt_chrysalis_core::{ActionId, ActionToken, TokenAuthority, TokenError};

/// The bearer credential a client presents in `Authorization: Bearer <token>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApiToken {
    pub token: String,
    pub action: ActionId,
    pub actor: String,
    pub scope: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

/// Why a presented bearer credential was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiAuthError {
    /// No `Authorization: Bearer` header was presented.
    Missing,
    /// The bearer value was not issued by this service.
    Unknown,
    /// The token is valid but authorizes a different action.
    WrongAction,
    /// The token was issued for a different equipment scope.
    Unscoped,
    /// `now > expires_at`.
    Expired,
    /// The underlying capability was revoked or is otherwise invalid.
    Unauthorized,
}

impl core::fmt::Display for ApiAuthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl From<TokenError> for ApiAuthError {
    fn from(e: TokenError) -> Self {
        match e {
            TokenError::Unauthorized => ApiAuthError::Unauthorized,
            TokenError::WrongAction => ApiAuthError::WrongAction,
            TokenError::Unscoped => ApiAuthError::Unscoped,
            TokenError::Expired => ApiAuthError::Expired,
        }
    }
}

/// Opaque bearer values so one cannot be guessed from a neighbouring serial.
fn bearer_value(serial: u64, actor: &str, action: ActionId, issued_at_ms: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serial.to_le_bytes());
    hasher.update(actor.as_bytes());
    hasher.update(action.channel_id().to_le_bytes());
    hasher.update(issued_at_ms.to_le_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(4 + digest.len() * 2);
    out.push_str("tok-");
    for byte in digest {
        use core::fmt::Write as _;
        let _ = core::write!(out, "{byte:02x}");
    }
    out
}

/// Issues, authenticates, and revokes API bearer credentials. Single-threaded
/// by design, matching the archon kernel's concurrency model.
#[derive(Debug)]
pub struct ApiTokenManager {
    authority: TokenAuthority,
    registry: BTreeMap<String, ActionToken>,
    revoked: BTreeSet<String>,
    next_serial: u64,
}

impl ApiTokenManager {
    pub fn new(scope: &str) -> Self {
        Self {
            authority: TokenAuthority::new(scope.to_string()),
            registry: BTreeMap::new(),
            revoked: BTreeSet::new(),
            next_serial: 0,
        }
    }

    pub fn scope(&self) -> &str {
        self.authority.scope()
    }

    /// Mints a bearer credential authorizing `action`, live for `ttl_ms`.
    pub fn mint(&mut self, actor: &str, action: ActionId, issued_at_ms: u64, ttl_ms: u64) -> ApiToken {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.saturating_add(1);
        let token = self.authority.issue(action, actor, issued_at_ms, issued_at_ms.saturating_add(ttl_ms));
        let bearer = bearer_value(serial, actor, action, issued_at_ms);
        let api_token = ApiToken {
            token: bearer.clone(),
            action,
            actor: actor.to_string(),
            scope: self.authority.scope().to_string(),
            issued_at_ms,
            expires_at_ms: token.expires_at_ms,
        };
        self.registry.insert(bearer, token);
        api_token
    }

    /// Authenticates a presented bearer value against the authority's
    /// enforcement gate (scope, expiry, action, capability liveness).
    pub fn authenticate(
        &self,
        bearer: &str,
        action: ActionId,
        now_ms: u64,
    ) -> Result<&ActionToken, ApiAuthError> {
        let token = self.registry.get(bearer).ok_or(ApiAuthError::Unknown)?;
        if self.revoked.contains(bearer) {
            return Err(ApiAuthError::Unauthorized);
        }
        self.authority.verify(token, action, now_ms)?;
        Ok(token)
    }

    /// Revokes a bearer credential; subsequent `authenticate` calls for it
    /// fail (capability liveness is shared with the gateway).
    pub fn revoke(&mut self, bearer: &str) -> bool {
        if !self.registry.contains_key(bearer) {
            return false;
        }
        if let Some(token) = self.registry.get(bearer) {
            self.authority.revoke(token);
        }
        self.revoked.insert(bearer.to_string());
        true
    }

    /// Number of live bearer credentials.
    pub fn live_count(&self) -> usize {
        self.registry.len().saturating_sub(self.revoked.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        1_000
    }

    #[test]
    fn minted_bearer_authenticates() {
        let mut m = ApiTokenManager::new("equipment/sim-liquid-handler-0");
        let t = m.mint("alice", ActionId::SubmitProtocol, now(), 5_000);
        assert!(m.authenticate(&t.token, ActionId::SubmitProtocol, now() + 500).is_ok());
        assert_eq!(m.live_count(), 1);
    }

    #[test]
    fn wrong_action_and_expiry_rejected() {
        let mut m = ApiTokenManager::new("equipment/sim-liquid-handler-0");
        let t = m.mint("alice", ActionId::ReadState, now(), 10);
        assert_eq!(
            m.authenticate(&t.token, ActionId::SubmitProtocol, now()).unwrap_err(),
            ApiAuthError::WrongAction
        );
        assert_eq!(
            m.authenticate(&t.token, ActionId::ReadState, now() + 11).unwrap_err(),
            ApiAuthError::Expired
        );
    }

    #[test]
    fn unknown_and_revoked_rejected() {
        let mut m = ApiTokenManager::new("equipment/sim-liquid-handler-0");
        let t = m.mint("bob", ActionId::ReadState, now(), 5_000);
        assert_eq!(
            m.authenticate("tok-garbage", ActionId::ReadState, now()).unwrap_err(),
            ApiAuthError::Unknown
        );
        assert!(m.revoke(&t.token));
        assert_eq!(m.live_count(), 0);
        assert_eq!(
            m.authenticate(&t.token, ActionId::ReadState, now()).unwrap_err(),
            ApiAuthError::Unauthorized
        );
    }

    #[test]
    fn bearer_values_are_not_sequential() {
        let mut m = ApiTokenManager::new("equipment/sim-liquid-handler-0");
        let a = m.mint("alice", ActionId::ReadState, now(), 5_000);
        let b = m.mint("alice", ActionId::ReadState, now(), 5_000);
        assert_ne!(a.token, b.token);
        assert_ne!(b.token, "tok-00000001");
        assert!(a.token.starts_with("tok-"));
        assert_eq!(a.token.len(), 4 + 64);
    }
}
