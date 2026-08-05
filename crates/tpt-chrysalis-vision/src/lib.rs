//! `tpt-chrysalis-vision`: the Phase 2 high-content imaging pipeline
//! (spec.txt §4 Layer 4, §5D).
//!
//! - [`store`] — S3/MinIO-compatible object storage with SHA-256
//!   checksum-on-write, behind a single [`store::ObjectStore`] trait
//!   (memory / filesystem / MinIO backends).
//! - [`features`] — a deterministic, edge-computed CV pipeline extracting cell
//!   morphology, neurite outgrowth, and TEER barrier-integrity features from a
//!   (synthetic) microscopy frame, written back as checksummed objects and
//!   PDSG-ready observations.

extern crate alloc;

pub mod features;
pub mod store;

pub use features::{
    extract_features, SyntheticMicroscopyImage, VisionFeatures, VisionObservation, VisionPipeline,
};
pub use store::{
    aws_sigv4_s3, LocalFsObjectStore, MemoryObjectStore, MinioConfig, MinioStore, ObjectStore,
    StoredObject, StoreError,
};
