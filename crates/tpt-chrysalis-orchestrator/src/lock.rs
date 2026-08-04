//! Resource locking (spec.txt §5A): prevents simulated hardware collisions
//! and reagent cross-contamination. The locking logic is real regardless of
//! the backend — a second protocol touching the same reagent/well/equipment
//! is refused until the first releases it.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

/// A lockable resource. Canonical id strings keep the set small and testable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub String);

impl ResourceId {
    pub fn reagent(name: &str) -> Self {
        Self(format!("reagent:{name}"))
    }

    pub fn well(well: &str) -> Self {
        Self(format!("well:{well}"))
    }

    pub fn equipment(equipment: &str) -> Self {
        Self(format!("equipment:{equipment}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockError {
    /// The resource is already held by another in-flight protocol.
    AlreadyHeld,
}

impl core::fmt::Display for LockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// A simple, testable, all-or-nothing lock manager.
#[derive(Debug, Clone, Default)]
pub struct LockManager {
    held: BTreeSet<ResourceId>,
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            held: BTreeSet::new(),
        }
    }

    /// Acquires a single resource. Idempotent for the same caller only in the
    /// sense that a resource already held here errors.
    pub fn acquire(&mut self, id: &ResourceId) -> Result<(), LockError> {
        if !self.held.insert(id.clone()) {
            return Err(LockError::AlreadyHeld);
        }
        Ok(())
    }

    pub fn release(&mut self, id: &ResourceId) {
        self.held.remove(id);
    }

    pub fn is_held(&self, id: &ResourceId) -> bool {
        self.held.contains(id)
    }

    /// Acquires a set atomically: either every resource is acquired, or none
    /// is (rolls back acquisitions made so far on failure).
    pub fn acquire_all(&mut self, ids: &[ResourceId]) -> Result<(), LockError> {
        let mut acquired = Vec::new();
        for id in ids {
            match self.acquire(id) {
                Ok(()) => acquired.push(id.clone()),
                Err(e) => {
                    for a in acquired.iter().rev() {
                        self.release(a);
                    }
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn release_all(&mut self, ids: &[ResourceId]) {
        for id in ids {
            self.release(id);
        }
    }

    pub fn held_count(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release() {
        let mut lm = LockManager::new();
        let r = ResourceId::reagent("A");
        assert!(!lm.is_held(&r));
        lm.acquire(&r).unwrap();
        assert!(lm.is_held(&r));
        assert_eq!(lm.acquire(&r), Err(LockError::AlreadyHeld));
        lm.release(&r);
        assert!(!lm.is_held(&r));
    }

    #[test]
    fn acquire_all_is_atomic() {
        let mut lm = LockManager::new();
        let a = ResourceId::well("w1");
        let b = ResourceId::reagent("B");
        lm.acquire(&a).unwrap();
        // b alone would succeed; with a already held the batch must fail and
        // roll back everything (so b stays free).
        assert_eq!(lm.acquire_all(&[b.clone(), a.clone()]), Err(LockError::AlreadyHeld));
        assert!(!lm.is_held(&b));
        assert!(lm.is_held(&a));
    }
}
