//! The advisory role (`tpt-eve` symbolic/causal reasoning, Phase 3).
//!
//! Per ADR 0002 the advisor is **advisory only**: its output is attached to a
//! [`ProtocolProposal`] as `rationale` and can never gate, relax, or bypass the
//! [`tpt_chrysalis_safety`] check. The trait is the defined interop surface;
//! [`EveAdvisor`] is a stand-in until the real `tpt-eve` crate is wired in.

use alloc::string::String;

use crate::interop::{Goal, Trial};

/// Higher-level experiment-design reasoning (hypothesis generation, cross-domain
/// abstraction). Output is free-text context for the learner — never a decision.
pub trait Advisor {
    fn advise(&self, goal: &Goal, history: &[Trial]) -> String;
}

/// A stand-in for `tpt-eve`'s symbolic/causal layer. Produces deterministic,
/// non-gating advisory text from the trial history.
#[derive(Debug, Default)]
pub struct EveAdvisor;

impl Advisor for EveAdvisor {
    fn advise(&self, goal: &Goal, history: &[Trial]) -> String {
        if history.is_empty() {
            return format!(
                "Goal '{}': no trials yet; seed the loop broadly before trusting a model.",
                goal.id
            );
        }
        let best = history
            .iter()
            .max_by(|a, b| {
                a.observed
                    .partial_cmp(&b.observed)
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
            .map(|t| t.observed);
        format!(
            "Goal '{}': {} trials observed; best objective so far {:.3}. Prefer proposals that explore unvisited regions before exploiting.",
            goal.id,
            history.len(),
            best.unwrap_or(f64::NAN)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interop::ProtocolProposal;

    #[test]
    fn advisor_is_non_gating_text() {
        let a = EveAdvisor;
        let g = Goal {
            id: "g1".into(),
            maximize: true,
            description: "maximize viability".into(),
        };
        let advice = a.advise(&g, &[]);
        assert!(advice.contains("no trials yet"));
        let history = vec![Trial {
            proposal: ProtocolProposal {
                id: "p".into(),
                steps: vec![],
                rationale: String::new(),
            },
            observed: 0.7,
        }];
        assert!(a.advise(&g, &history).contains("best objective so far 0.700"));
    }
}
