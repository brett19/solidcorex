// Written for solidcorex. Apache-2.0.
//! Stage 1: the face pairs whose boxes meet.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use scx_brep::FaceId;
use scx_num::budget::Stop;
use scx_num::extent3d::Extent3d;

use crate::error::{BooleanDefect, BooleanError};
use crate::geom::{Operands, Side};

/// Every pair of a face of `a` and a face of `b` whose boxes ([`scx_brep::Face::
/// bounding_box`], grown by accuracy and resolution) meet, sorted by `a`'s id then `b`'s.
/// `b`'s box tree names the candidates for each face of `a`; a pair whose boxes do not
/// meet holds no point within the resolution of both faces, so no interference.
///
/// # Errors
///
/// [`BooleanDefect::NoFaceBox`] for a face with no box: a finished body's faces are
/// bounded. [`BooleanError::Cancelled`] when `stop` is set.
pub(crate) fn face_pairs(
    ops: Operands<'_>,
    stop: &Stop,
) -> Result<Vec<(FaceId, FaceId)>, BooleanError> {
    let no_box = |side: Side, face: FaceId| {
        BooleanError::Defect(BooleanDefect::NoFaceBox {
            operand: side.operand(),
            face,
        })
    };
    let mut out = Vec::new();
    let mut boxes: BTreeMap<FaceId, Extent3d> = BTreeMap::new();
    let (Some(ba), Some(bb)) = (ops.a.bounding_box(), ops.b.bounding_box()) else {
        // A body with no faces has no box and meets nothing.
        return Ok(out);
    };
    if ba.intersect(bb).is_none() {
        return Ok(out);
    }
    for fa in ops.a.faces() {
        stop.check()?;
        let xa = fa.bounding_box().ok_or_else(|| no_box(Side::A, fa.id()))?;
        if xa.intersect(bb).is_none() {
            continue;
        }
        for fb in ops.b.faces_meeting(xa) {
            // A face's box is derived in time linear in its uses: once each.
            let xb = match boxes.entry(fb.id()) {
                Entry::Occupied(o) => *o.get(),
                Entry::Vacant(v) => {
                    *v.insert(fb.bounding_box().ok_or_else(|| no_box(Side::B, fb.id()))?)
                }
            };
            if xa.intersect(xb).is_some() {
                out.push((fa.id(), fb.id()));
            }
        }
    }
    // `faces()` and `faces_meeting` are in id order, so this is a no-op kept as the
    // statement of the order.
    out.sort_unstable();
    Ok(out)
}
