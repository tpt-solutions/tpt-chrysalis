//! Multi-organ microfluidic linking model (spec.txt §2, Phase 2).
//!
//! A [`OrganLinkGraph`] is the simulated "blood flow" graph connecting several
//! Organ-on-a-Chip devices (e.g. Pancreas → Liver → Blood-Brain-Barrier),
//! extending the Phase 1 PDSG's actuator/sensor granularity up to the
//! chip level. Links are directed and must form an acyclic flow (no
//! recirculation loop); each link carries a rated maximum flow the gate
//! enforces per [`FlowPlan`].
//!
//! Biological compatibility is a hard, deterministic constraint: when an
//! incompatible organ pairing is carrying a reactive reagent, the graph sets
//! [`Var::CrossChipIncompatible`] in the evaluated environment, which the
//! organoid rule set (see [`crate::safety`]) aborts on — no AI or network in
//! this decision path.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The organ a chip models. Determines cross-chip biological compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganKind {
    Pancreas,
    Liver,
    BloodBrainBarrier,
    Gut,
    Kidney,
    Heart,
    Generic,
}

impl OrganKind {
    pub const fn name(self) -> &'static str {
        match self {
            OrganKind::Pancreas => "pancreas",
            OrganKind::Liver => "liver",
            OrganKind::BloodBrainBarrier => "blood_brain_barrier",
            OrganKind::Gut => "gut",
            OrganKind::Kidney => "kidney",
            OrganKind::Heart => "heart",
            OrganKind::Generic => "generic",
        }
    }

    /// True iff a flow carrying a reactive reagent from `a` into `b` is
    /// biologically incompatible (would cross-contaminate `b`). The
    /// Blood-Brain-Barrier is the most selective organ and must never receive
    /// effluent from the more metabolically aggressive organs.
    pub const fn incompatible_with(self, other: OrganKind) -> bool {
        use OrganKind::*;
        matches!(
            (self, other),
            (BloodBrainBarrier, Pancreas)
                | (Pancreas, BloodBrainBarrier)
                | (BloodBrainBarrier, Kidney)
                | (Kidney, BloodBrainBarrier)
                | (BloodBrainBarrier, Heart)
                | (Heart, BloodBrainBarrier)
        )
    }
}

/// One simulated Organ-on-a-Chip device in the linking graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chip {
    pub id: String,
    pub organ: OrganKind,
    /// Reagent identifiers this chip tolerates without cross-contamination
    /// risk. A reagent absent from this list is "reactive" for this chip.
    pub tolerated_reagents: Vec<String>,
}

/// A directed microfluidic connection between two chips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub from: String,
    pub to: String,
    /// Rated maximum flow in microlitres per minute for this connection.
    pub max_ul_per_min: f64,
    /// Whether a reactive (non-tolerant) reagent may cross this link. When
    /// false, an incompatible pairing carrying a reactive reagent is blocked.
    pub allows_reactive: bool,
}

impl Link {
    /// A stable identifier encoding the direction.
    pub fn id(&self) -> String {
        format!("{}->{}", self.from, self.to)
    }
}

/// A concrete assignment of flow rates (µl/min) over a graph's links.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowPlan {
    pub rates: BTreeMap<String, f64>,
}

impl FlowPlan {
    pub fn set(&mut self, link: &Link, ul_per_min: f64) -> &mut Self {
        self.rates.insert(link.id(), ul_per_min);
        self
    }

    pub fn rate_for(&self, link: &Link) -> Option<f64> {
        self.rates.get(&link.id()).copied()
    }

    /// A flow plan is itself "present" only when the evaluated environment
    /// flags active flow; this helper mirrors that into the gate variable.
    pub fn flow_var(&self) -> f64 {
        // The aggregate flow magnitude is not what the gate bounds — per-link
        // limits are checked by [`OrganLinkGraph::check_flow`]. This exists so
        // a protocol can declare that cross-chip flow is active at all.
        self.rates
            .values()
            .copied()
            .fold(0.0_f64, |acc, v| acc + v)
    }
}

/// Errors from graph construction / validation.
#[derive(Debug, Error, PartialEq)]
pub enum GraphError {
    #[error("duplicate chip id {0}")]
    DuplicateChip(String),
    #[error("link endpoint {0} does not name a chip in the graph")]
    UnknownChip(String),
    #[error("link {0}->{1} would create a recirculation cycle")]
    Cycle(String, String),
    #[error("flow on link {0} ({1} µl/min) exceeds rated maximum {2} µl/min")]
    FlowOverLimit(String, f64, f64),
}

/// The simulated multi-organ "blood flow" graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrganLinkGraph {
    chips: BTreeMap<String, Chip>,
    links: Vec<Link>,
}

impl OrganLinkGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_chip(&mut self, chip: Chip) -> Result<(), GraphError> {
        if self.chips.contains_key(&chip.id) {
            return Err(GraphError::DuplicateChip(chip.id.clone()));
        }
        self.chips.insert(chip.id.clone(), chip);
        Ok(())
    }

    pub fn add_link(&mut self, link: Link) -> Result<(), GraphError> {
        if !self.chips.contains_key(&link.from) {
            return Err(GraphError::UnknownChip(link.from.clone()));
        }
        if !self.chips.contains_key(&link.to) {
            return Err(GraphError::UnknownChip(link.to.clone()));
        }
        self.links.push(link);
        Ok(())
    }

    pub fn chip(&self, id: &str) -> Option<&Chip> {
        self.chips.get(id)
    }

    pub fn chips(&self) -> impl Iterator<Item = &Chip> {
        self.chips.values()
    }

    pub fn links(&self) -> &[Link] {
        &self.links
    }

    pub fn link(&self, id: &str) -> Option<&Link> {
        self.links.iter().find(|l| l.id() == id)
    }

    /// Validates structural integrity: every endpoint resolves and the directed
    /// link set is acyclic (no recirculation loop).
    pub fn validate(&self) -> Result<(), GraphError> {
        // Adjacency for cycle detection.
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for link in &self.links {
            adj.entry(link.from.as_str())
                .or_default()
                .push(link.to.as_str());
        }
        // White/gray/black DFS for cycle detection.
        let mut color: BTreeMap<&str, u8> = BTreeMap::new();
        for start in self.chips.keys().map(String::as_str) {
            if color.get(start).copied().unwrap_or(0) != 0 {
                continue;
            }
            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            color.insert(start, 1);
            while let Some((node, idx)) = stack.last_mut() {
                let node = *node;
                let neighbors = adj.get(node).map(Vec::as_slice).unwrap_or(&[]);
                if *idx < neighbors.len() {
                    let nxt = neighbors[*idx];
                    *idx += 1;
                    match color.get(nxt).copied().unwrap_or(0) {
                        0 => {
                            color.insert(nxt, 1);
                            stack.push((nxt, 0));
                        }
                        1 => return Err(GraphError::Cycle(node.to_string(), nxt.to_string())),
                        _ => {}
                    }
                } else {
                    color.insert(node, 2);
                    stack.pop();
                }
            }
        }
        Ok(())
    }

    /// The chips in a valid topological (downstream) order. Errors if the graph
    /// has not been validated/is cyclic.
    pub fn topological_order(&self) -> Result<Vec<String>, GraphError> {
        self.validate()?;
        let mut indeg: BTreeMap<&str, usize> = BTreeMap::new();
        for c in self.chips.keys() {
            indeg.entry(c.as_str()).or_insert(0);
        }
        for link in &self.links {
            *indeg.entry(link.to.as_str()).or_insert(0) += 1;
        }
        let mut queue: Vec<&str> = indeg
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&n, _)| n)
            .collect();
        let mut out: Vec<String> = Vec::new();
        while let Some(node) = queue.pop() {
            out.push(node.to_string());
            for link in &self.links {
                if link.from == node {
                    let d = indeg.get_mut(link.to.as_str()).expect("present");
                    *d -= 1;
                    if *d == 0 {
                        queue.push(link.to.as_str());
                    }
                }
            }
        }
        if out.len() != self.chips.len() {
            return Err(GraphError::Cycle("graph".into(), "graph".into()));
        }
        Ok(out)
    }

    /// Enforces per-link rated flow limits. Returns the first over-limit link.
    pub fn check_flow(&self, plan: &FlowPlan) -> Result<(), GraphError> {
        for link in &self.links {
            if let Some(rate) = plan.rate_for(link) {
                if rate > link.max_ul_per_min {
                    return Err(GraphError::FlowOverLimit(
                        link.id(),
                        rate,
                        link.max_ul_per_min,
                    ));
                }
            }
        }
        Ok(())
    }

    /// True iff `reagent` is reactive for `chip` (not in its tolerated set).
    pub fn is_reactive(&self, chip_id: &str, reagent: &str) -> bool {
        match self.chips.get(chip_id) {
            Some(c) => !c.tolerated_reagents.iter().any(|t| t == reagent),
            None => true,
        }
    }

    /// Builds the gate environment flag for cross-chip contamination. Sets
    /// [`Var::CrossChipIncompatible`] to 1.0 exactly when some active link is
    /// an incompatible pairing *and* carries a reactive reagent, else 0.0.
    pub fn contamination_env(&self, active_reagents: &[String]) -> f64 {
        for link in &self.links {
            let from = match self.chips.get(&link.from) {
                Some(c) => c,
                None => continue,
            };
            let to = match self.chips.get(&link.to) {
                Some(c) => c,
                None => continue,
            };
            let incompatible_pairing = from.organ.incompatible_with(to.organ);
            if !incompatible_pairing {
                continue;
            }
            if link.allows_reactive {
                continue;
            }
            let carries_reactive = active_reagents
                .iter()
                .any(|r| self.is_reactive(&link.from, r) || self.is_reactive(&link.to, r));
            if carries_reactive {
                return 1.0;
            }
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pancreas_liver_bbb() -> OrganLinkGraph {
        let mut g = OrganLinkGraph::new();
        g.add_chip(Chip {
            id: "pancreas".into(),
            organ: OrganKind::Pancreas,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_chip(Chip {
            id: "liver".into(),
            organ: OrganKind::Liver,
            tolerated_reagents: vec!["A".into(), "B".into()],
        })
        .unwrap();
        g.add_chip(Chip {
            id: "bbb".into(),
            organ: OrganKind::BloodBrainBarrier,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_link(Link {
            from: "pancreas".into(),
            to: "liver".into(),
            max_ul_per_min: 50.0,
            allows_reactive: true,
        })
        .unwrap();
        g.add_link(Link {
            from: "liver".into(),
            to: "bbb".into(),
            max_ul_per_min: 20.0,
            allows_reactive: false,
        })
        .unwrap();
        g
    }

    #[test]
    fn valid_graph_is_acyclic() {
        assert!(pancreas_liver_bbb().validate().is_ok());
        assert_eq!(pancreas_liver_bbb().topological_order().unwrap().len(), 3);
    }

    #[test]
    fn cycle_is_detected() {
        let mut g = pancreas_liver_bbb();
        g.add_link(Link {
            from: "bbb".into(),
            to: "pancreas".into(),
            max_ul_per_min: 10.0,
            allows_reactive: false,
        })
        .unwrap();
        assert!(matches!(g.validate(), Err(GraphError::Cycle(_, _))));
    }

    #[test]
    fn flow_over_limit_is_rejected() {
        let g = pancreas_liver_bbb();
        let mut plan = FlowPlan::default();
        let liver_bbb = g.link("liver->bbb").unwrap();
        plan.set(liver_bbb, 30.0); // > 20
        assert!(matches!(
            g.check_flow(&plan),
            Err(GraphError::FlowOverLimit(_, 30.0, 20.0))
        ));
    }

    #[test]
    fn contamination_flagged_for_incompatible_reactive_link() {
        // A directly incompatible pairing (Pancreas -> Blood-Brain-Barrier),
        // without the Liver conditioning step, carrying a reactive reagent.
        let mut g = OrganLinkGraph::new();
        g.add_chip(Chip {
            id: "pancreas".into(),
            organ: OrganKind::Pancreas,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_chip(Chip {
            id: "bbb".into(),
            organ: OrganKind::BloodBrainBarrier,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_link(Link {
            from: "pancreas".into(),
            to: "bbb".into(),
            max_ul_per_min: 20.0,
            allows_reactive: false,
        })
        .unwrap();
        // Reagent B is reactive for both ends of an incompatible pairing.
        assert_eq!(g.contamination_env(&["B".into()]), 1.0);
        // Reagent A is tolerated everywhere on that path.
        assert_eq!(g.contamination_env(&["A".into()]), 0.0);
    }

    #[test]
    fn unknown_endpoint_rejected() {
        let mut g = OrganLinkGraph::new();
        g.add_chip(Chip {
            id: "x".into(),
            organ: OrganKind::Generic,
            tolerated_reagents: vec![],
        })
        .unwrap();
        assert!(matches!(
            g.add_link(Link {
                from: "x".into(),
                to: "ghost".into(),
                max_ul_per_min: 1.0,
                allows_reactive: false,
            }),
            Err(GraphError::UnknownChip(_))
        ));
    }
}
