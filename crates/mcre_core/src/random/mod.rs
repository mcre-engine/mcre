mod gaussian;
mod legacy;
mod weighted_list;

pub use gaussian::GaussianGenerator;
pub use legacy::LegacyRand;
pub use weighted_list::*;

pub trait RandomImpl {
    fn split(&mut self) -> Self;

    fn next_i32(&mut self) -> i32;

    fn next_bounded_i32(&mut self, bound: i32) -> i32;

    fn next_inbetween_i32(&mut self, min: i32, max: i32) -> i32 {
        self.next_bounded_i32(max - min + 1) + min
    }

    fn next_i64(&mut self) -> i64;

    fn next_bool(&mut self) -> bool;

    fn next_f32(&mut self) -> f32;

    fn next_f64(&mut self) -> f64;

    fn next_gaussian(&mut self) -> f64;

    fn next_triangular(&mut self, mode: f64, deviation: f64) -> f64 {
        mode + deviation * (self.next_f64() - self.next_f64())
    }

    fn skip(&mut self, count: i32) {
        for _ in 0..count {
            self.next_i64();
        }
    }

    fn next_inbetween_i32_exclusive(&mut self, min: i32, max: i32) -> i32 {
        min + self.next_bounded_i32(max - min)
    }
}
