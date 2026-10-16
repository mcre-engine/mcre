use crate::math::Mat4;

const MAX_PITCH: f32 = 1.5533; // ~89 degrees

/// First-person fly camera (yaw/pitch, right-handed, +Y up).
pub struct FlyCamera {
    pub position: [f32; 3],
    /// Yaw around +Y, 0 faces north (-Z), positive turns east (+X).
    pub yaw: f32,
    /// Pitch, positive looks up.
    pub pitch: f32,
    pub fov: f32,
    pub speed: f32,
    /// Radians per pixel of mouse movement.
    pub sensitivity: f32,
}

impl FlyCamera {
    pub fn new(position: [f32; 3]) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: -0.15,
            fov: 70.0_f32.to_radians(),
            speed: 8.0,
            sensitivity: 0.002,
        }
    }

    pub fn forward(&self) -> [f32; 3] {
        let cp = self.pitch.cos();
        [self.yaw.sin() * cp, self.pitch.sin(), -self.yaw.cos() * cp]
    }

    pub fn right(&self) -> [f32; 3] {
        [self.yaw.cos(), 0.0, self.yaw.sin()]
    }

    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn view_matrix(&self) -> Mat4 {
        let target = [
            self.position[0] + self.forward()[0],
            self.position[1] + self.forward()[1],
            self.position[2] + self.forward()[2],
        ];
        Mat4::look_at(self.position, target, [0.0, 1.0, 0.0])
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective(self.fov, aspect, 0.05, 512.0)
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_is_unit_length_and_yaw_zero_faces_north() {
        let mut cam = FlyCamera::new([0.0, 0.0, 0.0]);
        cam.yaw = 0.0;
        cam.pitch = 0.0;
        let f = cam.forward();
        assert!((f[0] - 0.0).abs() < 1e-6);
        assert!((f[1] - 0.0).abs() < 1e-6);
        assert!((f[2] - -1.0).abs() < 1e-6);

        cam.yaw = 1.234;
        cam.pitch = -0.5;
        let f = cam.forward();
        let len = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
    }

    #[test]
    fn right_at_yaw_zero_is_east() {
        let cam = FlyCamera::new([0.0, 0.0, 0.0]);
        let r = cam.right();
        assert!((r[0] - 1.0).abs() < 1e-6);
        assert!(r[1].abs() < 1e-6);
        assert!(r[2].abs() < 1e-6);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut cam = FlyCamera::new([0.0, 0.0, 0.0]);
        cam.pitch = 0.0;
        cam.rotate(0.0, 10.0);
        assert!(cam.pitch <= MAX_PITCH);
        cam.rotate(0.0, -20.0);
        assert!(cam.pitch >= -MAX_PITCH);
    }
}
