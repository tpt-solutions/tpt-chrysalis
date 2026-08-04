//! Name → id registries mapping protocol strings (reagent "A", well "w3",
//! valve "v1") onto the compact HAL id types.

use tpt_chrysalis_hal::{ReagentId, ValveId, WellId};

/// Maps reagent names to [`ReagentId`]s (A/B/C → 0/1/2).
#[derive(Debug, Clone, Copy)]
pub struct ReagentRegistry;

impl ReagentRegistry {
    pub fn id(name: &str) -> Option<ReagentId> {
        match name {
            "A" => Some(ReagentId(0)),
            "B" => Some(ReagentId(1)),
            "C" => Some(ReagentId(2)),
            _ => None,
        }
    }
}

/// Maps well names (`w1`…`w96`) to [`WellId`]s.
#[derive(Debug, Clone, Copy)]
pub struct WellRegistry;

impl WellRegistry {
    pub fn id(name: &str) -> Option<WellId> {
        let n = name.strip_prefix('w')?;
        let idx: u16 = n.parse().ok()?;
        if (1..=96).contains(&idx) {
            Some(WellId(idx))
        } else {
            None
        }
    }
}

/// Maps valve names (`v1`…) to [`ValveId`]s.
#[derive(Debug, Clone, Copy)]
pub struct ValveRegistry;

impl ValveRegistry {
    pub fn id(name: &str) -> Option<ValveId> {
        let n = name.strip_prefix('v')?;
        let idx: u8 = n.parse().ok()?;
        if idx >= 1 {
            Some(ValveId(idx))
        } else {
            None
        }
    }
}

/// Convenience facade for the compiler/gateway.
#[derive(Debug, Clone, Copy, Default)]
pub struct TargetRegistry;

impl TargetRegistry {
    pub fn well(name: &str) -> Option<WellId> {
        WellRegistry::id(name)
    }

    pub fn valve(name: &str) -> Option<ValveId> {
        ValveRegistry::id(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reagent_names_map() {
        assert_eq!(ReagentRegistry::id("A"), Some(ReagentId(0)));
        assert_eq!(ReagentRegistry::id("D"), None);
    }

    #[test]
    fn well_names_map() {
        assert_eq!(WellRegistry::id("w1"), Some(WellId(1)));
        assert_eq!(WellRegistry::id("w96"), Some(WellId(96)));
        assert_eq!(WellRegistry::id("w97"), None);
        assert_eq!(WellRegistry::id("not-a-well"), None);
        let _ = String::from("registry smoke");
    }
}
