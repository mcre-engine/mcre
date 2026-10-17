//! Vanilla `VoxelShape` face-slab geometry, mirroring
//! `Shapes.faceShapeOccludes` and `BlockBehaviour.getFaceOcclusionShape`.

use alloc::vec::Vec;

use mcre_core::{Axis, Direction};

use crate::BlockState;

const FULL_CUBE: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];

/// Vanilla `LightEngine.isEmptyShape`: states that opt out of shape-aware
/// occlusion behave like air for light transport.
pub fn is_empty_shape(state: BlockState) -> bool {
    !state.can_occlude() || !state.use_shape_for_light_occlusion()
}

/// Vanilla `LightEngine.shapeOccludes`: propagation from `from` to `to`
/// across `direction` is blocked when the two face slabs together cover the
/// full face.
pub fn shape_occludes(from: BlockState, to: BlockState, direction: Direction) -> bool {
    let from_face = if is_empty_shape(from) {
        Vec::new()
    } else {
        face_boxes(from.occlusion_boxes(), direction)
    };
    let to_face = if is_empty_shape(to) {
        Vec::new()
    } else {
        face_boxes(to.occlusion_boxes(), direction.opposite())
    };
    if from_face.is_empty() && to_face.is_empty() {
        return false;
    }
    let mut union = from_face;
    union.extend(to_face);
    is_covered(&FULL_CUBE, &union)
}

pub fn face_axis(direction: Direction) -> usize {
    match direction.axis() {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    }
}

/// Face-occlusion slabs for one side: occlusion boxes overlapping the
/// outermost voxel layer toward the face, normalized to span the full normal
/// axis (vanilla compares face slabs by 2D footprint — the sliced axis
/// always reports full range). Returns empty when the shape does not reach
/// the face side.
pub fn face_boxes(boxes: &[[f32; 6]], direction: Direction) -> Vec<[f32; 6]> {
    let Some((lo, hi)) = face_interval(boxes, direction) else {
        return Vec::new();
    };
    let axis = face_axis(direction);
    boxes
        .iter()
        .filter(|b| b[axis] < hi && b[axis + 3] > lo)
        .map(|b| {
            let mut slab = *b;
            slab[axis] = 0.0;
            slab[axis + 3] = 1.0;
            slab
        })
        .collect()
}

/// The outermost voxel interval toward `direction`, mirroring vanilla's
/// `findIndex` + `SubShape` slice selection over the shape's coordinate grid.
fn face_interval(boxes: &[[f32; 6]], direction: Direction) -> Option<(f32, f32)> {
    let axis = face_axis(direction);
    let mut coords = Vec::new();
    for b in boxes {
        coords.push(b[axis]);
        coords.push(b[axis + 3]);
    }
    coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
    coords.dedup();
    if coords.len() < 2 {
        return None;
    }
    let query = if direction.is_positive() {
        0.9999999
    } else {
        0.0000001
    };
    let mut from = 0usize;
    let mut len = coords.len();
    while len > 0 {
        let half = len / 2;
        let middle = from + half;
        if query < coords[middle] {
            len = half;
        } else {
            from = middle + 1;
            len -= half + 1;
        }
    }
    let size = coords.len() - 1;
    if from == 0 || from > size {
        return None;
    }
    Some((coords[from - 1], coords[from]))
}

/// Exact set coverage: every cell of `own` covered by `occluder`.
/// Cell boundaries come from the box planes themselves, so containment is
/// exact with no epsilon.
pub fn is_covered(own: &[[f32; 6]], occluder: &[[f32; 6]]) -> bool {
    if own.is_empty() {
        return true;
    }
    let mut edges = [Vec::new(), Vec::new(), Vec::new()];
    for boxes in [own, occluder] {
        for b in boxes.iter() {
            for (axis, edge) in edges.iter_mut().enumerate() {
                edge.push(b[axis]);
                edge.push(b[axis + 3]);
            }
        }
    }
    for edge in edges.iter_mut() {
        let values = core::mem::take(edge);
        let mut sorted = values;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup();
        *edge = sorted;
    }
    let inside = |boxes: &[[f32; 6]], mins: [f32; 3], maxs: [f32; 3]| {
        boxes.iter().any(|b| {
            b[0] <= mins[0]
                && maxs[0] <= b[3]
                && b[1] <= mins[1]
                && maxs[1] <= b[4]
                && b[2] <= mins[2]
                && maxs[2] <= b[5]
        })
    };
    for xi in 0..edges[0].len().saturating_sub(1) {
        for yi in 0..edges[1].len().saturating_sub(1) {
            for zi in 0..edges[2].len().saturating_sub(1) {
                let mins = [edges[0][xi], edges[1][yi], edges[2][zi]];
                let maxs = [edges[0][xi + 1], edges[1][yi + 1], edges[2][zi + 1]];
                if inside(own, mins, maxs) && !inside(occluder, mins, maxs) {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn is_covered_covers() {
        let full = [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];
        let empty: [[f32; 6]; 0] = [];
        let bottom_half = [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
        let top_half = [[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]];
        let west_post = [[0.0, 0.0, 0.0, 0.25, 1.0, 1.0]];
        let east_post = [[0.75, 0.0, 0.0, 1.0, 1.0, 1.0]];
        assert!(is_covered(&full, &full));
        assert!(is_covered(&empty, &full));
        assert!(is_covered(&bottom_half, &full));
        assert!(!is_covered(&full, &bottom_half));
        assert!(!is_covered(&top_half, &bottom_half));
        assert!(!is_covered(&bottom_half, &top_half));
        assert!(!is_covered(&west_post, &east_post));
        assert!(is_covered(&west_post, &west_post));
        let mut split = Vec::new();
        split.extend(bottom_half);
        split.extend(top_half);
        assert!(is_covered(&split, &full));
        assert!(is_covered(&full, &split));
    }

    #[test]
    fn face_boxes_select_touching_plane() {
        let bottom_half = [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
        assert_eq!(
            face_boxes(&bottom_half, Direction::Down),
            vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]
        );
        assert!(face_boxes(&bottom_half, Direction::Up).is_empty());
        assert_eq!(face_boxes(&bottom_half, Direction::North).len(), 1);
        let empty: [[f32; 6]; 0] = [];
        assert!(face_boxes(&empty, Direction::Up).is_empty());
    }
}
