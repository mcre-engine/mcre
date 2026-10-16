/// Column-major 4x4 matrix (`arr[col * 4 + row]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub const IDENTITY: Self = Self([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);

    /// Right-handed perspective projection mapping to WebGPU depth range `0..1`.
    pub fn perspective(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y_radians * 0.5).tan();
        let a = far / (near - far);
        let b = (far * near) / (near - far);
        Self([
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            f,
            0.0,
            0.0,
            0.0,
            0.0,
            a,
            -1.0,
            0.0,
            0.0,
            b,
            0.0,
        ])
    }

    /// Right-handed view matrix looking from `eye` towards `target`.
    pub fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> Self {
        let f = normalize(sub(target, eye));
        let s = normalize(cross(f, up));
        let u = cross(s, f);
        Self([
            s[0],
            u[0],
            -f[0],
            0.0,
            s[1],
            u[1],
            -f[1],
            0.0,
            s[2],
            u[2],
            -f[2],
            0.0,
            -dot(s, eye),
            -dot(u, eye),
            dot(f, eye),
            1.0,
        ])
    }

    /// Transforms a point (w = 1) and performs the perspective divide.
    pub fn transform_point(&self, p: [f32; 3]) -> [f32; 3] {
        let m = self.0;
        let x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12];
        let y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13];
        let z = m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14];
        let w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
        [x / w, y / w, z / w]
    }
}

impl core::ops::Mul for Mat4 {
    type Output = Mat4;

    /// Matrix product `self * rhs`.
    fn mul(self, rhs: Mat4) -> Mat4 {
        let a = self.0;
        let b = rhs.0;
        let mut out = [0.0; 16];
        for col in 0..4 {
            for row in 0..4 {
                out[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
            }
        }
        Mat4(out)
    }
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len > 0.0 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_depth_range() {
        let proj = Mat4::perspective(1.0, 16.0 / 9.0, 0.1, 100.0);
        let near = proj.transform_point([0.0, 0.0, -0.1]);
        let far = proj.transform_point([0.0, 0.0, -100.0]);
        assert!((near[2] - 0.0).abs() < 1e-5, "{}", near[2]);
        assert!((far[2] - 1.0).abs() < 1e-5, "{}", far[2]);
    }

    #[test]
    fn look_at_maps_forward_to_negative_z() {
        let eye = [3.0, 2.0, 5.0];
        let target = [1.0, 2.0, -4.0];
        let view = Mat4::look_at(eye, target, [0.0, 1.0, 0.0]);

        let at_target = view.transform_point(target);
        assert!(at_target[0].abs() < 1e-5, "{}", at_target[0]);
        assert!(at_target[1].abs() < 1e-5, "{}", at_target[1]);
        assert!(
            at_target[2] < 0.0,
            "target should be in front: {}",
            at_target[2]
        );

        let at_eye = view.transform_point(eye);
        assert!(at_eye.iter().all(|v| v.abs() < 1e-5), "{:?}", at_eye);
    }

    #[test]
    fn mul_identity() {
        let p = Mat4::perspective(1.2, 1.5, 0.1, 50.0);
        assert_eq!(p * Mat4::IDENTITY, p);
        assert_eq!(Mat4::IDENTITY * p, p);
    }
}
