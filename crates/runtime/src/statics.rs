//! Static-span analysis for exports: where the comp provably does not move.
//!
//! Export re-renders every frame through the full pipeline, yet most
//! motion-graphics frames repeat pixels exactly: holds between keyframes,
//! settled bouncies, static backs under a single animated element, end
//! cards held on purpose. This module scans the document's time-varying
//! inputs — keyframe tracks (transform *and* effect parameters), layer in/out
//! windows, nested comp timing — and returns the maximal time ranges where
//! all of them evaluate to identical values. Rendering one frame per range
//! and reusing it is then pixel-identical to rendering each frame, because
//! the reference renderer is a pure function of these inputs.
//!
//! Anything that can sample independently of document keys disqualifies the
//! whole comp (`None`): file-backed or embedded footage playback, shape
//! generators (they draw from their own parameters), and the orbit
//! turntable. That conservatism is the contract — a static span must be
//! *provable*, never probable.

use bonaparte_model::{CompId, Easing, LayerKind, Project, PropValue, Time, Track};
use std::collections::BTreeSet;

/// A half-open tick range `[from, to)` inside the export range where the
/// rendered content is constant.
pub type Span = (i64, i64);

/// Maximal static spans of `project`'s comp between `start` and `end`.
/// `None` means the comp has content that moves on its own and no span
/// should be trusted.
pub fn static_spans(
    project: &Project,
    comp_id: CompId,
    start: Time,
    end: Time,
) -> Option<Vec<Span>> {
    let mut boundaries = BTreeSet::new();
    let mut dynamic = Vec::new();
    collect(
        project,
        comp_id,
        start.0,
        end.0,
        0,
        &mut boundaries,
        &mut dynamic,
    )?;
    boundaries.insert(start.0);
    boundaries.insert(end.0);
    let points: Vec<i64> = boundaries.into_iter().collect();
    let mut spans: Vec<Span> = Vec::new();
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b <= a {
            continue;
        }
        // A dynamic interval (da, db) moves the content anywhere strictly
        // between its ends; an open-interval overlap marks this slice unsafe.
        let moves = dynamic.iter().any(|&(da, db)| da < b && db > a);
        if moves {
            continue;
        }
        // Spans deliberately do not merge across a boundary even when both
        // sides are static: a boundary is a keyframe or window edge, and the
        // content either side of it is only *provably* equal within a slice.
        spans.push((a, b));
    }
    Some(spans)
}

/// Frames that share a static span collapse onto its first fully-covered
/// frame. The returned vector maps export frame index → anchor frame index
/// (self-anchored frames render normally). `None` disables reuse entirely.
pub fn reuse_plan(
    project: &Project,
    comp_id: CompId,
    start: Time,
    end: Time,
    tpf: i64,
    count: usize,
    frame_bytes: usize,
) -> Option<Vec<usize>> {
    if tpf <= 0 || count < 2 {
        return None;
    }
    let spans = static_spans(project, comp_id, start, end)?;
    let mut plan: Vec<usize> = (0..count).collect();
    let mut anchors = 0usize;
    for (from, to) in spans {
        // A frame's pixels depend only on its sample instant: every frame
        // time that lies inside the span shows identical content.
        let rel = from - start.0;
        let first = if rel <= 0 {
            0usize
        } else {
            ((rel + tpf - 1) / tpf) as usize
        };
        let rel_end = to - start.0;
        let last = if rel_end <= 0 {
            0usize
        } else {
            ((rel_end + tpf - 1) / tpf).min(count as i64) as usize
        };
        if last.saturating_sub(first) >= 2 {
            for k in first..last {
                plan[k] = first;
            }
        }
    }
    anchors = plan.iter().enumerate().filter(|(k, a)| k == *a).count();
    // Only pay for reuse when it actually removes work; and never hold more
    // anchor frames than the memory budget allows — encode-time memory, not
    // a correctness constraint.
    if anchors == 0 || anchors >= count {
        return None;
    }
    let budget = 256 * 1024 * 1024;
    if anchors.saturating_mul(frame_bytes.max(1)) > budget {
        return None;
    }
    Some(plan)
}

fn collect(
    project: &Project,
    comp_id: CompId,
    lo: i64,
    hi: i64,
    depth: u32,
    boundaries: &mut BTreeSet<i64>,
    dynamic: &mut Vec<(i64, i64)>,
) -> Option<()> {
    if depth > 8 {
        // Cycle guard consistent with the renderer's precomp limit.
        return None;
    }
    let comp = project.comps.get(&comp_id)?;
    if comp.turntable.enabled {
        return None;
    }
    for id in &comp.layer_order {
        let layer = comp.layers.get(id)?;
        match &layer.kind {
            // Footage samples its source independently of document keys.
            LayerKind::Footage { .. } => return None,
            // Generators draw from their own parameters; a time-driven
            // generator would be invisible to this analysis.
            LayerKind::Shape {
                generator: Some(_), ..
            } => return None,
            LayerKind::PreComp { comp: child } => {
                // Child content maps 1:1 through the layer window
                // (`child_time = time - layer.start`), so child findings are
                // collected over the shifted range and translated back.
                let child_lo = lo - layer.start.0;
                let child_hi = hi - layer.start.0;
                let mut child_boundaries = BTreeSet::new();
                let mut child_dynamic = Vec::new();
                collect(
                    project,
                    *child,
                    child_lo,
                    child_hi,
                    depth + 1,
                    &mut child_boundaries,
                    &mut child_dynamic,
                )?;
                let shift = layer.start.0;
                for b in child_boundaries {
                    boundaries.insert(b + shift);
                }
                for (a, b) in child_dynamic {
                    dynamic.push((a + shift, b + shift));
                }
            }
            _ => {}
        }
        boundaries.insert(layer.start.0);
        boundaries.insert((layer.start + layer.duration).0);
        for track in layer.tracks.values() {
            track_moments(track, boundaries, dynamic);
        }
        for effect in &layer.effects {
            for track in effect.tracks.values() {
                track_moments(track, boundaries, dynamic);
            }
        }
    }
    boundaries.retain(|t| *t > lo && *t < hi);
    dynamic.retain(|&(a, b)| a < hi && b > lo);
    Some(())
}

fn track_moments(track: &Track, boundaries: &mut BTreeSet<i64>, dynamic: &mut Vec<(i64, i64)>) {
    let mut keys: Vec<_> = track.keys.iter().collect();
    keys.sort_by_key(|k| k.time.0);
    let Some(first) = keys.first() else { return };
    // Before the first key the property evaluates to its static base value
    // (unknown from here), so the first key always splits a span.
    boundaries.insert(first.time.0);
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let same = match (&a.value, &b.value) {
            (PropValue::Scalar(x), PropValue::Scalar(y)) => x == y,
            (PropValue::Vec2(x), PropValue::Vec2(y)) => x == y,
            _ => false,
        };
        match a.easing {
            // The outgoing value holds to `b`, then steps: one jump at b.
            Easing::Hold => {
                if !same {
                    boundaries.insert(b.time.0);
                }
            }
            // A ramp with different endpoints moves everywhere except the
            // endpoints themselves; equal-endpoint linear segments are flat
            // and need no split at all.
            Easing::Linear if !same => {
                boundaries.insert(a.time.0);
                boundaries.insert(b.time.0);
                dynamic.push((a.time.0, b.time.0));
            }
            Easing::Linear => {}
            // Bezier handles can bulge between equal endpoints: treat every
            // Bezier segment as moving, split at the (equal-valued) ends.
            Easing::Bezier { .. } => {
                boundaries.insert(a.time.0);
                boundaries.insert(b.time.0);
                dynamic.push((a.time.0, b.time.0));
            }
        }
    }
}
