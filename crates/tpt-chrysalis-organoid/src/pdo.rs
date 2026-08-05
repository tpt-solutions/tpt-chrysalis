//! Patient-Derived Organoid (PDO) screening workflow (spec.txt §2, Phase 2).
//!
//! A [`PdoScreen`] turns a set of candidate [`Therapy`] definitions into a
//! concrete plan of per-therapy simulated dosing runs, then evaluates each run
//! against a [`PdoSimBackend`]. The default backend is a deterministic
//! Hill-type response model (no randomness, no network) so screening results
//! are reproducible and auditable; a real microphysiological readout can be
//! dropped in later behind the same trait without touching the screen logic.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// A candidate therapy to screen, expressed as a dose sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Therapy {
    pub id: String,
    /// Compound/reagent identifier being screened.
    pub compound: String,
    /// Lowest dose in the sweep, micromolar.
    pub min_um: f64,
    /// Highest dose in the sweep, micromolar.
    pub max_um: f64,
    /// Number of evenly-spaced doses (>= 1).
    pub steps: u32,
    /// Simulated exposure duration, minutes.
    pub duration_min: u64,
}

impl Therapy {
    /// The concretely planned doses across the sweep, inclusive of both ends.
    pub fn doses(&self) -> Vec<f64> {
        if self.steps <= 1 {
            return alloc::vec![self.min_um];
        }
        let span = self.max_um - self.min_um;
        let step = span / (self.steps - 1) as f64;
        (0..self.steps)
            .map(|i| self.min_um + step * i as f64)
            .collect()
    }
}

/// One per-therapy simulated dosing run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoseRun {
    pub therapy_id: String,
    pub compound: String,
    pub dose_um: f64,
    pub well: String,
}

/// The measured outcome of a [`DoseRun`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoseOutcome {
    pub run: DoseRun,
    /// Cell viability in [0,1] (1.0 = full viability).
    pub viability: f64,
    /// TEER barrier integrity in [0,1] (1.0 = intact).
    pub barrier_integrity: f64,
}

/// The backend a screen evaluates runs against. Swappable so a real
/// microphysiological readout can replace the synthetic model without changing
/// the screen orchestration.
pub trait PdoSimBackend {
    /// Evaluates a single run, returning a measured outcome.
    fn evaluate(&self, run: &DoseRun, therapy: &Therapy) -> DoseOutcome;
}

/// A deterministic Hill-type response backend: viability and barrier integrity
/// fall off with dose. Reproducible and bounded in [0,1] for every finite
/// input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyntheticPdoBackend {
    /// Dose (µM) at which viability reaches 50% (half-max).
    pub ic50_um: f64,
    /// Dose (µM) at which barrier integrity reaches 50% (half-max).
    pub barrier_half_max_um: f64,
}

impl Default for SyntheticPdoBackend {
    fn default() -> Self {
        Self {
            ic50_um: 25.0,
            barrier_half_max_um: 40.0,
        }
    }
}

impl PdoSimBackend for SyntheticPdoBackend {
    fn evaluate(&self, run: &DoseRun, _therapy: &Therapy) -> DoseOutcome {
        let d = run.dose_um.max(0.0);
        let hill = |half: f64| (1.0 + d / half).recip().clamp(0.0, 1.0);
        DoseOutcome {
            run: run.clone(),
            viability: hill(self.ic50_um),
            barrier_integrity: hill(self.barrier_half_max_um),
        }
    }
}

/// A patient-derived organoid screening definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdoScreen {
    pub id: String,
    pub therapies: Vec<Therapy>,
}

impl PdoScreen {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            therapies: Vec::new(),
        }
    }

    pub fn add_therapy(&mut self, therapy: Therapy) -> &mut Self {
        self.therapies.push(therapy);
        self
    }

    /// Expands the therapies into concrete dosing runs, assigning each a stable
    /// well id (`<therapy>-<index>`).
    pub fn plan(&self) -> Vec<DoseRun> {
        let mut runs = Vec::new();
        for t in &self.therapies {
            for (i, dose) in t.doses().iter().enumerate() {
                runs.push(DoseRun {
                    therapy_id: t.id.clone(),
                    compound: t.compound.clone(),
                    dose_um: *dose,
                    well: format!("{}-{}", t.id, i),
                });
            }
        }
        runs
    }

    /// Runs the full plan against `backend`, returning outcomes in well order.
    pub fn run<B: PdoSimBackend>(&self, backend: &B) -> Vec<DoseOutcome> {
        self.plan()
            .iter()
            .map(|run| {
                let therapy = self
                    .therapies
                    .iter()
                    .find(|t| t.id == run.therapy_id)
                    .expect("run references a planned therapy");
                backend.evaluate(run, therapy)
            })
            .collect()
    }

    /// Selects the dose per therapy that maximizes viability subject to a
    /// minimum barrier-integrity threshold. Used by the closed-loop Phase 3
    /// controller as the safe dosing recommendation.
    pub fn recommend<B: PdoSimBackend>(
        &self,
        backend: &B,
        min_barrier: f64,
    ) -> Vec<DoseOutcome> {
        let mut chosen = Vec::new();
        for t in &self.therapies {
            let mut best: Option<DoseOutcome> = None;
            for run in self.plan().iter().filter(|r| r.therapy_id == t.id) {
                let outcome = backend.evaluate(run, t);
                if outcome.barrier_integrity < min_barrier {
                    continue;
                }
                match &best {
                    Some(b) if b.viability >= outcome.viability => {}
                    _ => best = Some(outcome),
                }
            }
            if let Some(o) = best {
                chosen.push(o);
            }
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> PdoScreen {
        let mut s = PdoScreen::new("pdo-1");
        s.add_therapy(Therapy {
            id: "tx-A".into(),
            compound: "CompoundX".into(),
            min_um: 0.0,
            max_um: 100.0,
            steps: 5,
            duration_min: 240,
        });
        s.add_therapy(Therapy {
            id: "tx-B".into(),
            compound: "CompoundY".into(),
            min_um: 10.0,
            max_um: 10.0,
            steps: 1,
            duration_min: 120,
        });
        s
    }

    #[test]
    fn dose_sweep_is_inclusive() {
        let t = Therapy {
            id: "t".into(),
            compound: "C".into(),
            min_um: 0.0,
            max_um: 100.0,
            steps: 5,
            duration_min: 1,
        };
        assert_eq!(t.doses(), alloc::vec![0.0, 25.0, 50.0, 75.0, 100.0]);
    }

    #[test]
    fn plan_expands_all_therapies() {
        assert_eq!(screen().plan().len(), 6);
    }

    #[test]
    fn higher_dose_lowers_viability_deterministically() {
        let b = SyntheticPdoBackend::default();
        let s = screen();
        let low = s.run(&b)[0].clone();
        let high = s.run(&b)[4].clone();
        assert!(high.viability < low.viability);
        // determinism: a second run yields identical outcomes
        assert_eq!(s.run(&b), s.run(&b));
    }

    #[test]
    fn recommend_respects_barrier_threshold() {
        let b = SyntheticPdoBackend::default();
        let s = screen();
        let rec = s.recommend(&b, 0.5);
        assert!(!rec.is_empty());
        for o in &rec {
            assert!(o.barrier_integrity >= 0.5);
        }
    }
}
