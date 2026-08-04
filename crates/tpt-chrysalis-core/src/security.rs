//! Capability-token security for chrysalis physical actions.
//!
//! The unforgeable substrate is `tpt-archon-bridge`'s [`Capability`]: only a
//! [`CapabilityIssuer`] can mint one, and enforcement points check liveness
//! against the issuer (so revocation is real). This module adds what a
//! physical-action token needs on top:
//!
//! - an [`ActionId`] per hardware operation, mapped to an archon
//!   `Resource::Channel` id;
//! - a short, enforced expiry (archon capabilities have none);
//! - an explicit action-scope so one token cannot be replayed for another
//!   action.

use alloc::rc::Rc;
use alloc::string::{String, ToString};
use core::cell::RefCell;

use serde::{Deserialize, Serialize};
use tpt_archon_bridge::capability::{
    Capability, CapabilityIssuer, Resource, Right, SharedIssuer,
};

use crate::ledger::PhysicalAction;
#[cfg(test)]
use crate::DEFAULT_SCOPE;

/// The set of actions a token can authorize. Discriminants are stable channel
/// ids in the archon IPC space (see ADR 0001). Ids 1–4 are physical actions
/// that map to HAL channels and reach the kernel `MessageRouter`; ids 5+ are
/// *logical* API actions that authorize service-level operations only and
/// never map to a physical channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u64)]
pub enum ActionId {
    /// `dispense(reagent, volume, target)`
    Dispense = 1,
    /// `open_valve(valve, open)`
    OpenValve = 2,
    /// `set_temperature(setpoint_milli_c)`
    SetTemperature = 3,
    /// `read_sensor(channel)`
    ReadSensor = 4,
    /// Logical: read the PDSG / audit state via the API.
    ReadState = 5,
    /// Logical: submit a protocol intent via the API.
    SubmitProtocol = 6,
}

impl ActionId {
    /// The archon channel id this action maps to.
    pub const fn channel_id(self) -> u64 {
        self as u64
    }

    /// The capability [`Right`] an action requires (actuation = write,
    /// observation = read).
    pub const fn right(self) -> Right {
        match self {
            ActionId::ReadSensor | ActionId::ReadState => Right::Read,
            _ => Right::Write,
        }
    }

    /// Maps a ledger [`PhysicalAction`] to its [`ActionId`].
    pub fn from_action(action: &PhysicalAction) -> Option<ActionId> {
        match action {
            PhysicalAction::Dispense { .. } => Some(ActionId::Dispense),
            PhysicalAction::SetValve { .. } => Some(ActionId::OpenValve),
            PhysicalAction::SetTemperature { .. } => Some(ActionId::SetTemperature),
            PhysicalAction::ReadSensor { .. } => Some(ActionId::ReadSensor),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            ActionId::Dispense => "dispense",
            ActionId::OpenValve => "open_valve",
            ActionId::SetTemperature => "set_temperature",
            ActionId::ReadSensor => "read_sensor",
            ActionId::ReadState => "read_state",
            ActionId::SubmitProtocol => "submit_protocol",
        }
    }
}

/// A short-lived, action-scoped token binding an archon [`Capability`] to a
/// physical action. The archon `Capability` is unforgeable; the expiry and
/// action scope are enforced here.
#[derive(Debug, Clone)]
pub struct ActionToken {
    /// The underlying archon capability (validated against the issuer).
    pub capability: Capability,
    /// The physical action this token authorizes.
    pub action: ActionId,
    /// The bearer identity recorded in the audit ledger.
    pub actor: String,
    /// The equipment scope this token is valid for.
    pub scope: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

impl ActionToken {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms > self.expires_at_ms
    }
}

/// Why a token was rejected at the enforcement point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// The archon issuer does not authorize this capability (forged, revoked,
    /// or from a different issuer).
    Unauthorized,
    /// The token authorizes a different action.
    WrongAction,
    /// The token was issued for a different equipment scope.
    Unscoped,
    /// `now > expires_at`.
    Expired,
}

impl core::fmt::Display for TokenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Issues, verifies, and revokes [`ActionToken`]s. Owns the archon
/// [`CapabilityIssuer`] so liveness checks see every revocation.
#[derive(Debug, Clone)]
pub struct TokenAuthority {
    issuer: SharedIssuer,
    scope: String,
}

impl TokenAuthority {
    /// Creates an authority issuing tokens for `scope` (default
    /// [`DEFAULT_SCOPE`]).
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            issuer: Rc::new(RefCell::new(CapabilityIssuer::new())),
            scope: scope.into(),
        }
    }

    /// The shared issuer handle the enforcement layer (kernel `MessageRouter`,
    /// gateway) uses so live revocation is seen.
    pub fn issuer(&self) -> SharedIssuer {
        self.issuer.clone()
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Mints a token authorizing `action`, expiring at `expires_at_ms`.
    pub fn issue(&mut self, action: ActionId, actor: &str, issued_at_ms: u64, expires_at_ms: u64) -> ActionToken {
        let resource = Resource::Channel(action.channel_id());
        let capability = self
            .issuer
            .borrow_mut()
            .mint(resource, action.right());
        ActionToken {
            capability,
            action,
            actor: actor.to_string(),
            scope: self.scope.clone(),
            issued_at_ms,
            expires_at_ms,
        }
    }

    /// The enforcement check. Every physical command must pass through this
    /// before reaching the HAL. Structural checks (action scope, expiry) run
    /// first so a rejection reports *why*; the capability liveness check
    /// against the issuer runs last (it is the unforgeability/revocation
    /// guarantee).
    pub fn verify(&self, token: &ActionToken, action: ActionId, now_ms: u64) -> Result<(), TokenError> {
        if token.action != action {
            return Err(TokenError::WrongAction);
        }
        if token.scope != self.scope {
            return Err(TokenError::Unscoped);
        }
        if token.is_expired(now_ms) {
            return Err(TokenError::Expired);
        }
        if !self.issuer.borrow().authorizes(
            &token.capability,
            Resource::Channel(action.channel_id()),
            action.right(),
        ) {
            return Err(TokenError::Unauthorized);
        }
        Ok(())
    }

    /// Revokes a previously issued token; subsequent [`verify`](Self::verify)
    /// fails with [`TokenError::Unauthorized`].
    pub fn revoke(&mut self, token: &ActionToken) {
        self.issuer.borrow_mut().revoke(&token.capability);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        1_000
    }

    #[test]
    fn issued_token_verifies() {
        let mut auth = TokenAuthority::new(DEFAULT_SCOPE);
        let t = auth.issue(ActionId::Dispense, "alice", now(), now() + 100);
        assert_eq!(auth.verify(&t, ActionId::Dispense, now() + 50), Ok(()));
    }

    #[test]
    fn expired_token_is_rejected() {
        let mut auth = TokenAuthority::new(DEFAULT_SCOPE);
        let t = auth.issue(ActionId::Dispense, "alice", now(), now() + 10);
        assert_eq!(
            auth.verify(&t, ActionId::Dispense, now() + 11),
            Err(TokenError::Expired)
        );
    }

    #[test]
    fn unscoped_token_is_rejected() {
        let a = TokenAuthority::new(DEFAULT_SCOPE);
        let mut b = TokenAuthority::new("equipment/other-rig");
        let t = b.issue(ActionId::Dispense, "bob", now(), now() + 100);
        assert_eq!(
            a.verify(&t, ActionId::Dispense, now()),
            Err(TokenError::Unscoped)
        );
    }

    #[test]
    fn wrong_action_is_rejected() {
        let mut auth = TokenAuthority::new(DEFAULT_SCOPE);
        let t = auth.issue(ActionId::Dispense, "alice", now(), now() + 100);
        assert_eq!(
            auth.verify(&t, ActionId::SetTemperature, now()),
            Err(TokenError::WrongAction)
        );
    }

    #[test]
    fn forged_capability_from_another_issuer_is_rejected() {
        let auth = TokenAuthority::new(DEFAULT_SCOPE);
        let mut foreign = TokenAuthority::new(DEFAULT_SCOPE);
        let forged = foreign.issue(ActionId::Dispense, "mallory", now(), now() + 100);
        // The chrysalis token is structurally identical, but its underlying
        // archon capability was minted by a different issuer, which this
        // authority must reject.
        assert_eq!(
            auth.verify(&forged, ActionId::Dispense, now()),
            Err(TokenError::Unauthorized)
        );
    }

    #[test]
    fn revoked_token_is_rejected() {
        let mut auth = TokenAuthority::new(DEFAULT_SCOPE);
        let t = auth.issue(ActionId::Dispense, "alice", now(), now() + 100);
        auth.revoke(&t);
        assert_eq!(
            auth.verify(&t, ActionId::Dispense, now()),
            Err(TokenError::Unauthorized)
        );
    }

    #[test]
    fn action_ids_map_to_stable_channels() {
        assert_eq!(ActionId::Dispense.channel_id(), 1);
        assert_eq!(ActionId::OpenValve.channel_id(), 2);
        assert_eq!(ActionId::SetTemperature.channel_id(), 3);
        assert_eq!(ActionId::ReadSensor.channel_id(), 4);
        assert_eq!(ActionId::ReadSensor.right(), Right::Read);
        assert_eq!(ActionId::Dispense.right(), Right::Write);
    }
}
