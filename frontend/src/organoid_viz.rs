//! Deterministic 3D Organ-on-a-Chip array visualization (spec §4 Layer 4).
//!
//! Pure projection/colour helpers are kept free of any `web-sys` dependency so
//! they can be unit-tested on the host target; [`render_scene`] is the only
//! function that touches the canvas and is driven by the Leptos page via a
//! `requestAnimationFrame` loop, synced to the live sensor WebSocket feed.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// A node in the 3D array: stable id, world position, latest normalised value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub value: f64,
}

/// Project a world point onto the 2D canvas using a rotation about the
/// vertical (Y) axis and an oblique orthographic projection.
///
/// Returns `(screen_x, screen_y, depth)` where `depth` is used for
/// painter's-algorithm sorting (larger = further away).
pub fn project(
    point: (f64, f64, f64),
    angle: f64,
    scale: f64,
    cx: f64,
    cy: f64,
) -> (f64, f64, f64) {
    let (x, y, z) = point;
    let cos = angle.cos();
    let sin = angle.sin();
    // Rotate about Y.
    let xr = x * cos - z * sin;
    let zr = x * sin + z * cos;
    let sx = cx + xr * scale;
    // Y is up (subtract), depth gives a slight vertical offset for the iso look.
    let sy = cy - y * scale + zr * scale * 0.45;
    (sx, sy, zr)
}

/// Map a value in `[lo, hi]` to a blue→red heat colour.
pub fn heat_color(value: f64, lo: f64, hi: f64) -> (u8, u8, u8) {
    let t = if hi > lo {
        ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    hsv_to_rgb(240.0 * (1.0 - t), 0.75, 0.95)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = if hp < 1.0 {
        (c, x, 0.0)
    } else if hp < 2.0 {
        (x, c, 0.0)
    } else if hp < 3.0 {
        (0.0, c, x)
    } else if hp < 4.0 {
        (0.0, x, c)
    } else if hp < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = v - c;
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

fn shade((r, g, b): (u8, u8, u8), factor: f64) -> String {
    let f = factor.clamp(0.0, 1.0);
    format!(
        "rgb({},{},{})",
        (r as f64 * f) as u8,
        (g as f64 * f) as u8,
        (b as f64 * f) as u8
    )
}

/// Render the array onto `ctx`. `nodes` are projected and drawn back-to-front.
pub fn render_scene(
    ctx: &CanvasRenderingContext2d,
    canvas: &HtmlCanvasElement,
    nodes: &[ArrayNode],
    angle: f64,
    lo: f64,
    hi: f64,
) {
    let width = canvas.width() as f64;
    let height = canvas.height() as f64;
    ctx.clear_rect(0.0, 0.0, width, height);

    ctx.set_fill_style_str("rgba(15, 23, 42, 1.0)");
    ctx.fill_rect(0.0, 0.0, width, height);

    let cx = width / 2.0;
    let cy = height / 2.0;
    let scale = (width.min(height) / 6.0).max(8.0);

    // Painter's algorithm: far (large depth) first.
    let mut projected: Vec<(f64, f64, f64, &ArrayNode)> = nodes
        .iter()
        .map(|n| {
            let (sx, sy, depth) = project((n.x, n.y, n.z), angle, scale, cx, cy);
            (sx, sy, depth, n)
        })
        .collect();
    projected.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let s = (scale * 0.5).clamp(8.0, 28.0);
    let d = (s * 0.35, -s * 0.35); // oblique depth offset

    for (sx, sy, _depth, n) in projected {
        let color = heat_color(n.value, lo, hi);

        // Top face.
        ctx.begin_path();
        ctx.move_to(sx - s / 2.0, sy - s / 2.0);
        ctx.line_to(sx + s / 2.0, sy - s / 2.0);
        ctx.line_to(sx + s / 2.0 + d.0, sy - s / 2.0 + d.1);
        ctx.line_to(sx - s / 2.0 + d.0, sy - s / 2.0 + d.1);
        ctx.close_path();
        ctx.set_fill_style_str(&shade(color, 1.25));
        ctx.fill();

        // Right face.
        ctx.begin_path();
        ctx.move_to(sx + s / 2.0, sy - s / 2.0);
        ctx.line_to(sx + s / 2.0, sy + s / 2.0);
        ctx.line_to(sx + s / 2.0 + d.0, sy + s / 2.0 + d.1);
        ctx.line_to(sx + s / 2.0 + d.0, sy - s / 2.0 + d.1);
        ctx.close_path();
        ctx.set_fill_style_str(&shade(color, 0.7));
        ctx.fill();

        // Front face.
        ctx.begin_path();
        ctx.rect(sx - s / 2.0, sy - s / 2.0, s, s);
        ctx.set_fill_style_str(&shade(color, 1.0));
        ctx.fill();
        ctx.set_stroke_style_str("rgba(226, 232, 240, 0.7)");
        ctx.set_line_width(1.0);
        ctx.stroke();

        // Label.
        ctx.set_fill_style_str("rgba(226, 232, 240, 0.9)");
        ctx.set_font("10px monospace");
        ctx.fill_text(&n.id, sx - s / 2.0, sy + s / 2.0 + 12.0)
            .expect("fill_text");
    }
}

/// Convenience: resolve the latest sensor values into [`ArrayNode`]s using the
/// world positions from the PDSG snapshot's sensor nodes.
pub fn build_nodes(
    sensors: &[(String, crate::types::Location3D)],
    values: &HashMap<String, f64>,
) -> Vec<ArrayNode> {
    sensors
        .iter()
        .map(|(id, loc)| ArrayNode {
            id: id.clone(),
            x: loc.x,
            y: loc.y,
            z: loc.z,
            value: *values.get(id).unwrap_or(&0.0),
        })
        .collect()
}

/// Re-export so the page can obtain a 2D context without repeating casts.
pub fn canvas_2d(canvas: &HtmlCanvasElement) -> Option<CanvasRenderingContext2d> {
    canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_centers_origin() {
        let (sx, sy, depth) = project((0.0, 0.0, 0.0), 0.0, 10.0, 100.0, 100.0);
        assert_eq!((sx, sy), (100.0, 100.0));
        assert_eq!(depth, 0.0);
    }

    #[test]
    fn project_rotates_about_y() {
        // At angle 0, +z stays at depth +z; at PI/2, +z maps to -x.
        let (_, _, depth0) = project((0.0, 0.0, 1.0), 0.0, 1.0, 0.0, 0.0);
        let (sx90, _, _) = project((0.0, 0.0, 1.0), std::f64::consts::FRAC_PI_2, 1.0, 0.0, 0.0);
        assert!((depth0 - 1.0).abs() < 1e-9);
        assert!((sx90 + 1.0).abs() < 1e-9);
    }

    #[test]
    fn heat_color_clamps_at_bounds() {
        assert_eq!(heat_color(-5.0, 0.0, 10.0), heat_color(0.0, 0.0, 10.0));
        assert_eq!(heat_color(50.0, 0.0, 10.0), heat_color(10.0, 0.0, 10.0));
    }

    #[test]
    fn heat_color_uniform_range_is_midpoint() {
        // A uniform range (hi == lo) falls back to the midpoint colour, which is
        // the same colour produced by any value at t = 0.5.
        let (r, g, b) = heat_color(5.0, 5.0, 5.0);
        assert_eq!((r, g, b), heat_color(0.5, 0.0, 1.0));
    }

    #[test]
    fn build_nodes_uses_latest_value() {
        let sensors = vec![("s1".into(), crate::types::Location3D { x: 1.0, y: 2.0, z: 3.0 })];
        let mut values = HashMap::new();
        values.insert("s1".into(), 4.2);
        let nodes = build_nodes(&sensors, &values);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].value, 4.2);
        assert_eq!(nodes[0].y, 2.0);
    }
}
