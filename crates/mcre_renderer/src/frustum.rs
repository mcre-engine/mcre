use crate::math::Mat4;

#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    pub fn from_matrix(view_proj: &Mat4) -> Self {
        let m = view_proj.0;
        let row = |i: usize| [m[i], m[4 + i], m[8 + i], m[12 + i]];
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let mut frustum = Self {
            planes: [
                add(r3, r0),
                sub(r3, r0),
                add(r3, r1),
                sub(r3, r1),
                r2,
                sub(r3, r2),
            ],
        };
        for plane in &mut frustum.planes {
            let len = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
            if len > 0.0 {
                for v in plane.iter_mut() {
                    *v /= len;
                }
            }
        }
        frustum
    }

    pub fn intersects_aabb(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        for plane in &self.planes {
            let px = if plane[0] >= 0.0 { max[0] } else { min[0] };
            let py = if plane[1] >= 0.0 { max[1] } else { min[1] };
            let pz = if plane[2] >= 0.0 { max[2] } else { min[2] };
            if plane[0] * px + plane[1] * py + plane[2] * pz + plane[3] < 0.0 {
                return false;
            }
        }
        true
    }
}

fn add(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}

fn sub(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}

#[cfg(test)]
mod tests {
    #[test]
    fn spawn_camera_sees_full_ring() {
        use crate::camera::FlyCamera;
        let mut cam = FlyCamera::new([-186.0, 84.0, 52.0]);
        cam.yaw = 0.0;
        let vp = cam.view_projection(16.0 / 9.0);
        let frustum = Frustum::from_matrix(&vp);
        let mut seen = 0;
        for cx in -13..=-11 {
            for cz in 1..=3 {
                let min = [(cx * 16) as f32, -64.0, (cz * 16) as f32];
                let max = [(cx * 16 + 16) as f32, 320.0, (cz * 16 + 16) as f32];
                if frustum.intersects_aabb(min, max) {
                    seen += 1;
                } else {
                    eprintln!("culled chunk ({cx},{cz})");
                }
            }
        }
        assert_eq!(seen, 9);
    }

    use super::*;
    use crate::camera::FlyCamera;

    #[test]
    fn identity_frustum_contains_unit_cube() {
        let frustum = Frustum::from_matrix(&Mat4::IDENTITY);
        assert!(frustum.intersects_aabb([-0.5, -0.5, 0.25], [0.5, 0.5, 0.75]));
    }

    #[test]
    fn identity_frustum_rejects_outside_boxes() {
        let frustum = Frustum::from_matrix(&Mat4::IDENTITY);
        assert!(!frustum.intersects_aabb([2.0, -0.5, 0.25], [3.0, 0.5, 0.75]));
        assert!(!frustum.intersects_aabb([-0.5, 2.0, 0.25], [0.5, 3.0, 0.75]));
        assert!(!frustum.intersects_aabb([-0.5, -2.0, 0.25], [0.5, -1.5, 0.75]));
        assert!(!frustum.intersects_aabb([-0.5, -0.5, 1.5], [0.5, 0.5, 2.0]));
        assert!(!frustum.intersects_aabb([-0.5, -0.5, -2.0], [0.5, 0.5, -1.0]));
    }

    #[test]
    fn straddling_box_counts_as_intersecting() {
        let frustum = Frustum::from_matrix(&Mat4::IDENTITY);
        assert!(frustum.intersects_aabb([-2.0, -0.5, 0.25], [0.0, 0.5, 0.75]));
        assert!(frustum.intersects_aabb([-0.5, -0.5, 0.5], [0.5, 0.5, 5.0]));
    }

    #[test]
    fn touching_plane_counts_as_intersecting() {
        let frustum = Frustum::from_matrix(&Mat4::IDENTITY);
        assert!(frustum.intersects_aabb([1.0, -0.5, 0.25], [2.0, 0.5, 0.75]));
        assert!(frustum.intersects_aabb([-0.5, -0.5, 1.0], [0.5, 0.5, 2.0]));
    }

    #[test]
    fn degenerate_point_aabb() {
        let frustum = Frustum::from_matrix(&Mat4::IDENTITY);
        assert!(frustum.intersects_aabb([0.0, 0.0, 0.5], [0.0, 0.0, 0.5]));
        assert!(!frustum.intersects_aabb([5.0, 0.0, 0.5], [5.0, 0.0, 0.5]));
    }

    #[test]
    fn regression_viewer_camera_chunk_ring() {
        let camera = FlyCamera::new([8.0, 7.0, 26.0]);
        let view_proj = camera.view_projection(16.0 / 9.0);
        let frustum = Frustum::from_matrix(&view_proj);
        assert!(frustum.intersects_aabb([0.0, -64.0, 0.0], [16.0, 320.0, 16.0]));
        assert!(!frustum.intersects_aabb([0.0, -64.0, 40.0], [16.0, 320.0, 56.0]));
        assert!(!frustum.intersects_aabb([0.0, -64.0, -600.0], [16.0, 320.0, -560.0]));
        assert!(frustum.intersects_aabb([7.0, -64.0, 25.0], [9.0, 320.0, 27.0]));
        assert!(frustum.intersects_aabb([-16.0, -64.0, -16.0], [32.0, 320.0, 32.0]));
    }

    #[test]
    fn agrees_with_corner_ndc_when_all_corners_inside() {
        let camera = FlyCamera::new([8.0, 7.0, 26.0]);
        let view_proj = camera.view_projection(16.0 / 9.0);
        let frustum = Frustum::from_matrix(&view_proj);
        let min = [4.0, 2.0, 10.0];
        let max = [12.0, 6.0, 18.0];
        let mut corners_inside = true;
        for i in 0..8 {
            let p = [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ];
            let ndc = view_proj.transform_point(p);
            if !(ndc[0] >= -1.0
                && ndc[0] <= 1.0
                && ndc[1] >= -1.0
                && ndc[1] <= 1.0
                && ndc[2] >= 0.0
                && ndc[2] <= 1.0)
            {
                corners_inside = false;
            }
        }
        assert!(corners_inside);
        assert!(frustum.intersects_aabb(min, max));
    }
}
