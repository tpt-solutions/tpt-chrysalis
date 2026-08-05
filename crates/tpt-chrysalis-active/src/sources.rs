//! Active-learning proposal sources (the `tpt-anima` role, Phase 3).
//!
//! - [`QuadraticFitSource`] — a model-based learner: it fits a quadratic
//!   surrogate to the trial history (least squares) and proposes the predicted
//!   optimum, which is exactly "the next protocol that maximally reduces
//!   uncertainty" in a 1-D smooth landscape. This is the stand-in for
//!   `tpt-anima`'s Bayesian-optimization loop.
//! - [`GridSearchSource`] — the exhaustive baseline the benchmark compares
//!   against (spec.txt §5C's 10x–100x claim).
//!
//! Both implement [`crate::interop::ProposalSource`] so the real `tpt-anima`
//! crate can replace them behind the same trait.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::interop::{Goal, ProposalSource, ProtocolProposal, ProposedStep, Trial};
use crate::objective::Objective;

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Solves a 3x3 linear system `A theta = b` via Cramer's rule. Returns None if
/// singular.
fn solve_3x3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if d.abs() < 1e-12 {
        return None;
    }
    let col = |c: usize| {
        let mut m = a;
        for r in 0..3 {
            m[r][c] = b[r];
        }
        m
    };
    let da = det(col(0));
    let db = det(col(1));
    let dc = det(col(2));
    Some([da / d, db / d, dc / d])
}

/// Model-based active learner: fits `y = a x^2 + b x + c` and proposes the
/// analytic optimum of the surrogate, with light jitter to avoid re-proposing.
#[derive(Debug)]
pub struct QuadraticFitSource {
    var: String,
    bounds: crate::objective::Bounds,
    seed: u64,
    round: u64,
}

impl QuadraticFitSource {
    pub fn new(var: impl Into<String>, bounds: crate::objective::Bounds, seed: u64) -> Self {
        Self {
            var: var.into(),
            bounds,
            seed: seed.max(1),
            round: 0,
        }
    }

    fn seed_points(&self) -> Vec<f64> {
        let n = self.bounds.min;
        let x = self.bounds.max;
        vec![n, (n + x) / 2.0, x]
    }
}

impl ProposalSource for QuadraticFitSource {
    fn propose(&mut self, goal: &Goal, history: &[Trial]) -> ProtocolProposal {
        self.round += 1;
        let mut rng = self.seed.wrapping_add(self.round);
        let proposal_at = |v: f64, rationale: String| ProtocolProposal {
            id: format!("active-{}-{}", goal.id, self.round),
            steps: vec![ProposedStep {
                var: self.var.clone(),
                value: v,
            }],
            rationale,
        };

        // Seed phase: explore the three anchor points before trusting a model.
        if history.len() < 3 {
            let v = self.seed_points()[history.len()];
            return proposal_at(v, "seed exploration (model not yet trusted)".into());
        }

        // Fit quadratic surrogate and propose its optimum.
        let pts: Vec<(f64, f64)> = history
            .iter()
            .filter_map(|t| {
                t.proposal
                    .steps
                    .iter()
                    .find(|s| s.var == self.var)
                    .map(|s| (s.value, t.observed))
            })
            .collect();
        if pts.len() < 3 {
            return proposal_at(self.seed_points()[0], "insufficient fit data".into());
        }
        let mut ata = [[0.0_f64; 3]; 3];
        let mut atb = [0.0_f64; 3];
        for (x, y) in &pts {
            let row = [x * x, *x, 1.0];
            for i in 0..3 {
                for j in 0..3 {
                    ata[i][j] += row[i] * row[j];
                }
                atb[i] += row[i] * y;
            }
        }
        let theta = solve_3x3(ata, atb).unwrap_or([1.0, 0.0, 0.0]);
        let (a, b, _c) = (theta[0], theta[1], theta[2]);
        let optimum = if a.abs() > 1e-9 {
            (-b / (2.0 * a))
                .clamp(self.bounds.min, self.bounds.max)
        } else {
            (self.bounds.min + self.bounds.max) / 2.0
        };
        let jitter = ((xorshift(&mut rng) % 1000) as f64 / 1000.0 - 0.5) * (self.bounds.max - self.bounds.min) * 0.02;
        let v = (optimum + jitter).clamp(self.bounds.min, self.bounds.max);
        proposal_at(v, format!("quadratic surrogate optimum ≈ {optimum:.2} (a={a:.3})"))
    }
}

/// Exhaustive grid-search baseline.
#[derive(Debug)]
pub struct GridSearchSource {
    var: String,
    points: Vec<f64>,
    idx: usize,
}

impl GridSearchSource {
    pub fn new(var: impl Into<String>, bounds: crate::objective::Bounds, steps: u32) -> Self {
        let n = steps.max(1);
        let span = bounds.max - bounds.min;
        let points = (0..n)
            .map(|i| bounds.min + span * (i as f64 / (n - 1) as f64))
            .collect();
        Self {
            var: var.into(),
            points,
            idx: 0,
        }
    }
}

impl ProposalSource for GridSearchSource {
    fn propose(&mut self, goal: &Goal, _history: &[Trial]) -> ProtocolProposal {
        let v = self.points[self.idx % self.points.len()];
        self.idx += 1;
        ProtocolProposal {
            id: format!("grid-{}-{}", goal.id, self.idx),
            steps: vec![ProposedStep {
                var: self.var.clone(),
                value: v,
            }],
            rationale: "grid-search baseline".into(),
        }
    }
}

/// Counts evaluations a grid search needs to reach `target` value within
/// `epsilon`, capped at `max_evals`. Used by the benchmark.
pub fn grid_search_evals_to_target(
    goal: &Goal,
    objective: &dyn Objective,
    bounds: crate::objective::Bounds,
    steps: u32,
    target: f64,
    epsilon: f64,
    max_evals: u32,
) -> u32 {
    let mut src = GridSearchSource::new(objective.var().to_string(), bounds, steps);
    for i in 0..max_evals {
        let p = src.propose(goal, &[]);
        let v = objective.evaluate(&p);
        if (v - target).abs() <= epsilon {
            return i + 1;
        }
    }
    max_evals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::SimulatedObjective;

    #[test]
    fn quadratic_source_seeds_then_optimizes() {
        let obj = SimulatedObjective::default();
        let mut src = QuadraticFitSource::new("volume_ul", obj.bounds(), 1);
        let goal = Goal {
            id: "g".into(),
            maximize: true,
            description: String::new(),
        };
        // Seed three points.
        let mut history: Vec<Trial> = Vec::new();
        for _ in 0..3 {
            let p = src.propose(&goal, &history);
            let o = obj.evaluate(&p);
            history.push(Trial { proposal: p, observed: o });
        }
        assert_eq!(history.len(), 3);
        // After seeding, the source proposes a model optimum near 100.
        let p = src.propose(&goal, &history);
        let v = p.steps[0].value;
        assert!((80.0..=120.0).contains(&v), "expected near-optimum, got {v}");
    }

    #[test]
    fn solve_3x3_recovers_quadratic() {
        // y = 2x^2 - 3x + 1
        let pts = [(0.0, 1.0), (1.0, 0.0), (2.0, 3.0)];
        let mut ata = [[0.0_f64; 3]; 3];
        let mut atb = [0.0_f64; 3];
        for (x, y) in pts {
            let row = [x * x, x, 1.0];
            for i in 0..3 {
                for j in 0..3 {
                    ata[i][j] += row[i] * row[j];
                }
                atb[i] += row[i] * y;
            }
        }
        let t = solve_3x3(ata, atb).unwrap();
        assert!((t[0] - 2.0).abs() < 1e-9);
        assert!((t[1] + 3.0).abs() < 1e-9);
        assert!((t[2] - 1.0).abs() < 1e-9);
    }
}
