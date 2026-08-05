//! Phase 2 domain integration for the API service: multi-organ linking
//! (organoid) and high-content imaging (vision). Both are scoped by the
//! capability-token resource classes added in `tpt-chrysalis-core::security`
//! (`LinkChip`, `ScreenPdo`, `CaptureImage`, `StoreObject`) and reuse the same
//! deterministic, auditable building blocks from `tpt-chrysalis-organoid` and
//! `tpt-chrysalis-vision`.

use alloc::string::String;
use alloc::vec::Vec;

use tpt_chrysalis_core::{ORGANOID_SCOPE, VISION_SCOPE};
use tpt_chrysalis_organoid::{
    verify_cross_chip, CrossChipVerdict, FlowPlan, OrganLinkGraph, PdoScreen,
    SyntheticPdoBackend,
};
use tpt_chrysalis_safety::Environment;
use tpt_chrysalis_vision::{MemoryObjectStore, ObjectStore, StoredObject, StoreError, SyntheticMicroscopyImage, VisionFeatures, VisionPipeline};

/// The organoid (multi-organ linking) domain, scoped to [`ORGANOID_SCOPE`].
#[derive(Debug)]
pub struct OrganoidDomain {
    graph: OrganLinkGraph,
}

impl OrganoidDomain {
    pub fn new(graph: OrganLinkGraph) -> Self {
        Self { graph }
    }

    pub fn graph(&self) -> &OrganLinkGraph {
        &self.graph
    }

    /// Verifies a proposed cross-chip flow plan against the graph's structural
    /// limits and the biological-contamination rule (see
    /// `tpt_chrysalis_organoid::safety`). Pure and deterministic.
    pub fn verify_cross_chip(
        &self,
        plan: &FlowPlan,
        active_reagents: &[String],
    ) -> CrossChipVerdict {
        let mut env = Environment::new();
        verify_cross_chip(&self.graph, plan, active_reagents, &mut env)
    }

    /// Runs a PDO screening sweep against the deterministic backend.
    pub fn screen(&self, screen: &PdoScreen) -> Vec<tpt_chrysalis_organoid::DoseOutcome> {
        screen.run(&SyntheticPdoBackend::default())
    }
}

/// The vision (high-content imaging) domain, scoped to [`VISION_SCOPE`]. Uses
/// an in-process store by default; a MinIO-backed [`tpt_chrysalis_vision::MinioStore`]
/// can be swapped in behind the `minio` feature without changing callers.
#[derive(Debug, Default)]
pub struct VisionDomain {
    store: MemoryObjectStore,
}

impl VisionDomain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Extracts features from a synthetic frame and writes the raw image to the
    /// object store with checksum-on-write.
    pub fn capture_and_store(
        &self,
        img: &SyntheticMicroscopyImage,
        key: &str,
    ) -> Result<(VisionFeatures, StoredObject), StoreError> {
        VisionPipeline.process_and_store(img, &self.store, key)
    }

    /// Re-verifies a previously stored image by recomputing its SHA-256.
    pub fn verify_stored(&self, key: &str) -> Result<bool, StoreError> {
        self.store.verify(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn organoid_graph() -> OrganLinkGraph {
        let mut g = OrganLinkGraph::new();
        g.add_chip(tpt_chrysalis_organoid::Chip {
            id: "liver".into(),
            organ: tpt_chrysalis_organoid::OrganKind::Liver,
            tolerated_reagents: vec!["A".into(), "B".into()],
        })
        .unwrap();
        g.add_chip(tpt_chrysalis_organoid::Chip {
            id: "bbb".into(),
            organ: tpt_chrysalis_organoid::OrganKind::BloodBrainBarrier,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_chip(tpt_chrysalis_organoid::Chip {
            id: "pancreas".into(),
            organ: tpt_chrysalis_organoid::OrganKind::Pancreas,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_link(tpt_chrysalis_organoid::Link {
            from: "liver".into(),
            to: "bbb".into(),
            max_ul_per_min: 20.0,
            allows_reactive: false,
        })
        .unwrap();
        g.add_link(tpt_chrysalis_organoid::Link {
            from: "pancreas".into(),
            to: "bbb".into(),
            max_ul_per_min: 20.0,
            allows_reactive: false,
        })
        .unwrap();
        g
    }

    #[test]
    fn organoid_and_vision_domains_work() {
        let od = OrganoidDomain::new(organoid_graph());
        let mut plan = FlowPlan::default();
        plan.set(od.graph().link("liver->bbb").unwrap(), 10.0);
        let v = od.verify_cross_chip(&plan, &["B".into()]);
        assert!(!v.allowed); // pancreas->bbb is incompatible + reactive
        assert!(v.contaminated);

        let vd = VisionDomain::new();
        let img = SyntheticMicroscopyImage::synthetic(3, 32, 32, 0.02, 10);
        let (feats, stored) = vd.capture_and_store(&img, "chip1/frame0.raw").unwrap();
        assert!(vd.verify_stored("chip1/frame0.raw").unwrap());
        assert!(feats.cell_count > 0);
        assert_eq!(stored.len as usize, 8 + img.pixels.len());
    }
}
