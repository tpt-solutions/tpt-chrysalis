//! Local computer-vision pipeline (spec.txt §5D, Phase 2).
//!
//! Edge-computed, deterministic, and validated against synthetic microscopy
//! data (no real imaging hardware is connected in Phase 2). The pipeline
//! extracts measurable morphology features from a grayscale
//! [`SyntheticMicroscopyImage`] and writes the raw image to an
//! [`crate::store::ObjectStore`] (checksum-on-write). The structured features
//! are exposed as a [`VisionObservation`] for storage into the PDSG as a
//! Sensor/Observer Node update.
//!
//! All randomness comes from a seeded xorshift so a given `(seed, size,
//! density)` always yields the same image and therefore the same features —
//! the basis for the golden-file tests.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::store::{ObjectStore, StoreError, StoredObject};

/// A grayscale microscopy frame: `pixels.len() == width * height`, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticMicroscopyImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

impl SyntheticMicroscopyImage {
    /// Deterministically synthesizes a microscopy frame: `cell_density` * area
    /// filled disks of random radius/intensity over a noisy `background`.
    pub fn synthetic(
        seed: u64,
        width: u32,
        height: u32,
        cell_density: f64,
        background: u8,
    ) -> Self {
        let mut rng = seed.max(1);
        let area = (width * height) as usize;
        let mut pixels = vec![background; area];
        let n = (cell_density * area as f64) as u32;
        for _ in 0..n {
            let cx = (xorshift(&mut rng) % width as u64) as u32;
            let cy = (xorshift(&mut rng) % height as u64) as u32;
            let r = 2 + (xorshift(&mut rng) % 4) as u32;
            let intensity = 180 + (xorshift(&mut rng) % 60) as u8;
            let r2 = (r as i32) * (r as i32);
            let y0 = (cy as i32 - r as i32).max(0);
            let y1 = (cy as i32 + r as i32).min(height as i32 - 1);
            let x0 = (cx as i32 - r as i32).max(0);
            let x1 = (cx as i32 + r as i32).min(width as i32 - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let dx = x - cx as i32;
                    let dy = y - cy as i32;
                    if dx * dx + dy * dy <= r2 {
                        let idx = (y as u32 * width + x as u32) as usize;
                        pixels[idx] = intensity;
                    }
                }
            }
        }
        for p in pixels.iter_mut() {
            let noise = (xorshift(&mut rng) % 20) as i32 - 10;
            let v = (*p as i32) + noise + background as i32;
            *p = v.clamp(0, 255) as u8;
        }
        Self {
            width,
            height,
            pixels,
        }
    }
}

const THRESHOLD: u8 = 128;
/// Optimal monolayer confluence for barrier integrity (a too-sparse or
/// too-dense monolayer both reduce TEER).
const OPTIMAL_CONFLUENCE: f64 = 0.6;

fn neighbors(x: u32, y: u32, w: u32, h: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if x > 0 {
        out.push((x - 1, y));
    }
    if x + 1 < w {
        out.push((x + 1, y));
    }
    if y > 0 {
        out.push((x, y - 1));
    }
    if y + 1 < h {
        out.push((x, y + 1));
    }
    out
}

/// The structured features extracted from a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisionFeatures {
    pub cell_count: u32,
    pub mean_cell_area: f64,
    pub foreground_fraction: f64,
    /// Directional extension proxy in [0,1]: foreground mass in the right
    /// half over total foreground. A proxy for neurite/process outgrowth
    /// toward a target region.
    pub neurite_outgrowth_index: f64,
    /// TEER barrier-integrity proxy in [0,1], peaking at optimal confluence.
    pub barrier_integrity: f64,
}

/// Extracts features from a frame. Deterministic: identical pixels ⇒ identical
/// features.
pub fn extract_features(img: &SyntheticMicroscopyImage) -> VisionFeatures {
    let w = img.width;
    let h = img.height;
    let total = (w * h) as usize;
    let mut visited = vec![false; total];
    let mut count = 0u32;
    let mut areas: Vec<u32> = Vec::new();
    let mut foreground = 0u64;
    let mut right_foreground = 0u64;
    let mid = w / 2;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let fg = img.pixels[idx] > THRESHOLD;
            if fg {
                foreground += 1;
                if x >= mid {
                    right_foreground += 1;
                }
            }
            if visited[idx] || !fg {
                continue;
            }
            let mut stack = vec![(x, y)];
            visited[idx] = true;
            let mut area = 0u32;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                for (nx, ny) in neighbors(cx, cy, w, h) {
                    let nidx = (ny * w + nx) as usize;
                    if !visited[nidx] && img.pixels[nidx] > THRESHOLD {
                        visited[nidx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            count += 1;
            areas.push(area);
        }
    }

    let mean_cell_area = if count > 0 {
        areas.iter().map(|a| *a as f64).sum::<f64>() / count as f64
    } else {
        0.0
    };
    let frac = foreground as f64 / total as f64;
    let neurite = if foreground > 0 {
        right_foreground as f64 / foreground as f64
    } else {
        0.0
    };
    let barrier = (1.0 - (frac - OPTIMAL_CONFLUENCE).abs() / OPTIMAL_CONFLUENCE).clamp(0.0, 1.0);
    VisionFeatures {
        cell_count: count,
        mean_cell_area,
        foreground_fraction: frac,
        neurite_outgrowth_index: neurite,
        barrier_integrity: barrier,
    }
}

/// A time-stamped, PDSG-ready observation derived from a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisionObservation {
    pub t_ms: u64,
    pub image_key: String,
    pub features: VisionFeatures,
}

/// The edge CV pipeline.
#[derive(Debug, Default)]
pub struct VisionPipeline;

impl VisionPipeline {
    /// Extracts features from a frame.
    pub fn extract(&self, img: &SyntheticMicroscopyImage) -> VisionFeatures {
        extract_features(img)
    }

    /// Extracts features and writes the raw frame to `store` (checksum-on-write)
    /// under `key`, returning the stored object metadata.
    pub fn process_and_store<S: ObjectStore>(
        &self,
        img: &SyntheticMicroscopyImage,
        store: &S,
        key: &str,
    ) -> Result<(VisionFeatures, StoredObject), StoreError> {
        let features = self.extract(img);
        let mut bytes = vec![img.width.to_le_bytes().to_vec(), img.height.to_le_bytes().to_vec()]
            .concat();
        bytes.extend_from_slice(&img.pixels);
        let obj = store.put(key, &bytes)?;
        Ok((features, obj))
    }

    /// Builds a PDSG-ready observation from extracted features.
    pub fn to_observation(
        &self,
        t_ms: u64,
        image_key: &str,
        features: VisionFeatures,
    ) -> VisionObservation {
        VisionObservation {
            t_ms,
            image_key: image_key.to_string(),
            features,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryObjectStore;

    #[test]
    fn same_seed_is_deterministic() {
        let a = SyntheticMicroscopyImage::synthetic(42, 48, 48, 0.02, 20);
        let b = SyntheticMicroscopyImage::synthetic(42, 48, 48, 0.02, 20);
        assert_eq!(a, b);
        assert_eq!(extract_features(&a), extract_features(&b));
    }

    #[test]
    fn different_seed_differs() {
        let a = SyntheticMicroscopyImage::synthetic(1, 48, 48, 0.02, 20);
        let b = SyntheticMicroscopyImage::synthetic(2, 48, 48, 0.02, 20);
        assert_ne!(a, b);
    }

    #[test]
    fn features_are_sane() {
        let img = SyntheticMicroscopyImage::synthetic(7, 64, 64, 0.015, 10);
        let f = extract_features(&img);
        assert!(f.cell_count > 0);
        assert!(f.mean_cell_area > 0.0);
        assert!((0.0..=1.0).contains(&f.foreground_fraction));
        assert!((0.0..=1.0).contains(&f.neurite_outgrowth_index));
        assert!((0.0..=1.0).contains(&f.barrier_integrity));
    }

    #[test]
    fn golden_features_match_committed_baseline() {
        // The canonical image must always extract to the committed golden
        // values (tests/golden/features.v1.json) — a regression here means the
        // deterministic CV pipeline changed behavior. Float fields are compared
        // with a tolerance because JSON round-tripping of f64 is not bit-exact.
        let img = SyntheticMicroscopyImage::synthetic(42, 48, 48, 0.02, 20);
        let f = extract_features(&img);

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/features.v1.json");
        let golden: VisionFeatures = serde_json::from_str(
            &std::fs::read_to_string(path).expect("golden file present"),
        )
        .expect("golden parses");

        assert_eq!(f.cell_count, golden.cell_count);
        assert!((f.mean_cell_area - golden.mean_cell_area).abs() < 1e-9);
        assert!((f.foreground_fraction - golden.foreground_fraction).abs() < 1e-9);
        assert!((f.neurite_outgrowth_index - golden.neurite_outgrowth_index).abs() < 1e-9);
        assert!((f.barrier_integrity - golden.barrier_integrity).abs() < 1e-9);
    }

    #[test]
    fn process_and_store_writes_checksummed_image() {
        let img = SyntheticMicroscopyImage::synthetic(7, 32, 32, 0.02, 10);
        let store = MemoryObjectStore::new();
        let (features, obj) = VisionPipeline
            .process_and_store(&img, &store, "chip1/frame0.raw")
            .unwrap();
        assert!(store.verify("chip1/frame0.raw").unwrap());
        assert_eq!(obj.len as usize, 8 + img.pixels.len());
        let obs = VisionPipeline.to_observation(1_000, "chip1/frame0.raw", features);
        assert_eq!(obs.image_key, "chip1/frame0.raw");
    }
}
