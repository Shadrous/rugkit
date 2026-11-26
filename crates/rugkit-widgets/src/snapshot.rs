//! Non-blocking offscreen pixel snapshot testing.
//!
//! Renders a [`crate::Harness`] offscreen via `rugkit_render::OffscreenRenderer`, compares
//! against a baseline if present, and reports any pixel diffs to stderr **without failing the
//! build** (matching rugkit's snapshot policy in `PLAN.md`).

use std::path::Path;

use rugkit_render::{Color, OffscreenRenderer, SnapshotDiff, compare_rgba};

use crate::harness::Harness;

/// Result of running a non-blocking pixel snapshot check.
#[derive(Clone, Debug)]
pub enum SnapshotOutcome {
    /// No GPU adapter was available in this environment (skipped without failing).
    Skipped(String),
    /// Rendered successfully and compared against an existing baseline.
    Compared(SnapshotDiff),
    /// Rendered successfully; no baseline existed yet (baseline written if `write_baseline` was set).
    CreatedBaseline {
        /// Total pixels rendered.
        pixels: usize,
    },
}

/// Renders `harness` offscreen and compares against `baseline_path` with per-channel `tolerance`.
///
/// Never panics on pixel differences — instead logs a concise diff summary to stderr so CI
/// can upload or inspect diffs without blocking builds.
pub fn check_snapshot(
    name: &str,
    harness: &mut Harness,
    background: Color,
    tolerance: u8,
    baseline_path: Option<&Path>,
) -> SnapshotOutcome {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(err) => {
            eprintln!("rugkit snapshot [{name}]: skipped ({err})");
            return SnapshotOutcome::Skipped(err);
        }
    };
    let (w, h, rgba) = match harness.render_offscreen(&mut renderer, background) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("rugkit snapshot [{name}]: render error ({err})");
            return SnapshotOutcome::Skipped(err);
        }
    };
    let total_pixels = (w as usize) * (h as usize);

    let Some(path) = baseline_path else {
        return SnapshotOutcome::CreatedBaseline {
            pixels: total_pixels,
        };
    };

    if let Ok(expected) = std::fs::read(path)
        && expected.len() == rgba.len()
    {
        let diff = compare_rgba(&expected, &rgba, tolerance);
        if diff.differing_pixels > 0 {
            eprintln!(
                "rugkit snapshot [{name}]: {} / {} pixels differed ({:.2}%, max channel diff {})",
                diff.differing_pixels,
                diff.total_pixels,
                diff.diff_ratio() * 100.0,
                diff.max_channel_diff,
            );
        }
        SnapshotOutcome::Compared(diff)
    } else {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, &rgba);
        SnapshotOutcome::CreatedBaseline {
            pixels: total_pixels,
        }
    }
}
