use super::{RandomImpl, gaussian::GaussianGenerator};

pub struct LegacyRand {
    seed: u64,
    internal_next_gaussian: Option<f64>,
}

impl LegacyRand {
    pub fn from_seed(seed: u64) -> Self {
        LegacyRand {
            seed: (seed ^ 0x5DEECE66D) & 0xFFFFFFFFFFFF,
            internal_next_gaussian: None,
        }
    }

    fn next_random(&mut self) -> i64 {
        let l = self.seed as i64;
        let m = l.wrapping_mul(0x5DEECE66D).wrapping_add(11) & 0xFFFFFFFFFFFF;
        self.seed = m as u64;
        m
    }

    fn next(&mut self, bits: u64) -> i32 {
        (self.next_random() >> (48 - bits)) as i32
    }
}

impl GaussianGenerator for LegacyRand {
    fn stored_next_gaussian(&self) -> Option<f64> {
        self.internal_next_gaussian
    }

    fn set_stored_next_gaussian(&mut self, value: Option<f64>) {
        self.internal_next_gaussian = value;
    }
}

impl RandomImpl for LegacyRand {
    fn split(&mut self) -> Self {
        LegacyRand::from_seed(self.next_i64() as u64)
    }

    fn next_i32(&mut self) -> i32 {
        self.next(32)
    }

    fn next_i64(&mut self) -> i64 {
        let i = self.next_i32();
        let j = self.next_i32();
        ((i as i64) << 32).wrapping_add(j as i64)
    }

    fn next_f32(&mut self) -> f32 {
        self.next(24) as f32 * 5.9604645E-8f32
    }

    fn next_f64(&mut self) -> f64 {
        let i = self.next(26);
        let j = self.next(27);
        let l = ((i as i64) << 27).wrapping_add(j as i64);
        l as f64 * 1.110223E-16f32 as f64
    }

    fn next_bool(&mut self) -> bool {
        self.next(1) != 0
    }

    fn next_gaussian(&mut self) -> f64 {
        self.calculate_gaussian()
    }

    fn next_bounded_i32(&mut self, bound: i32) -> i32 {
        if (bound & bound.wrapping_sub(1)) == 0 {
            ((bound as i64).wrapping_mul(self.next(31) as i64) >> 31) as i32
        } else {
            loop {
                let i = self.next(31);
                let j = i % bound;
                if (i.wrapping_sub(j).wrapping_add(bound.wrapping_sub(1))) >= 0 {
                    return j;
                }
            }
        }
    }
}
