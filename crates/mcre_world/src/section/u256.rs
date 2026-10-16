use core::{fmt, iter};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct U256(u128, u128);

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U256(0b{:0128b}{:0128b})", self.0, self.1)
    }
}

impl U256 {
    pub const ZERO: Self = Self(0, 0);
    #[allow(unused)]
    pub const MAX: Self = Self(u128::MAX, u128::MAX);

    #[inline]
    pub fn set_bit(&mut self, bit: u8) {
        let (word, bit) = (bit / 128, bit % 128);
        self.0 |= ((word == 0) as u128) << bit;
        self.1 |= ((word == 1) as u128) << bit;
    }

    #[inline]
    pub fn clear_bit(&mut self, bit: u8) {
        let (word, bit) = (bit / 128, bit % 128);
        self.0 &= !(((word == 0) as u128) << bit);
        self.1 &= !(((word == 1) as u128) << bit);
    }

    #[inline]
    #[allow(unused)]
    pub fn is_set(&self, bit: u8) -> bool {
        let (word, bit) = (bit / 128, bit % 128);
        match word {
            0 => self.0 & (1 << bit) != 0,
            1 => self.1 & (1 << bit) != 0,
            _ => false,
        }
    }

    #[inline]
    pub fn count_ones(&self) -> u32 {
        self.0.count_ones() + self.1.count_ones()
    }

    #[inline]
    #[allow(unused)]
    pub fn count_zeros(&self) -> u32 {
        self.0.count_zeros() + self.1.count_zeros()
    }

    #[inline]
    #[allow(unused)]
    pub fn leading_zeros(self) -> u32 {
        let high = self.1;
        if high != 0 {
            high.leading_zeros()
        } else {
            128 + self.0.leading_zeros()
        }
    }

    #[inline]
    #[allow(unused)]
    pub fn trailing_zeros(self) -> u32 {
        let low = self.0;
        if low != 0 {
            low.trailing_zeros()
        } else {
            128 + self.1.trailing_zeros()
        }
    }

    pub fn iter_bits(self) -> impl Iterator<Item = u8> {
        let mut lo = self.0;
        let mut hi = self.1;
        let mut offset = 0;

        iter::from_fn(move || {
            if lo != 0 {
                let tz = lo.trailing_zeros();
                lo &= lo - 1;
                return Some(offset + tz as u8);
            }

            if offset == 0 {
                offset = 128;
            }

            if hi != 0 {
                let tz = hi.trailing_zeros();
                hi &= hi - 1;
                return Some(offset + tz as u8);
            }

            None
        })
    }

    #[inline]
    pub fn first_zero(self) -> Option<u8> {
        let inv_lo = !self.0;
        if inv_lo != 0 {
            return Some(inv_lo.trailing_zeros() as u8);
        }

        let inv_hi = !self.1;
        if inv_hi != 0 {
            return Some(128 + inv_hi.trailing_zeros() as u8);
        }

        None
    }

    #[inline]
    pub fn first_one(self) -> Option<u8> {
        let lo = self.0;
        if lo != 0 {
            return Some(lo.trailing_zeros() as u8);
        }

        let hi = self.1;
        if hi != 0 {
            return Some(128 + hi.trailing_zeros() as u8);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    #[test]
    fn test_set_and_get_bits() {
        let mut u = U256::ZERO;

        u.set_bit(10);
        assert!(u.is_set(10));
        assert!(!u.is_set(11));

        u.set_bit(130);
        assert!(u.is_set(130));
        assert!(u.is_set(10));

        u.clear_bit(10);
        assert!(!u.is_set(10));
        assert!(u.is_set(130));
    }

    #[test]
    fn test_iter_bits() {
        let mut u = U256::ZERO;
        let bits = vec![0, 1, 127, 128, 200, 255];

        for &b in &bits {
            u.set_bit(b);
        }

        let collected: Vec<u8> = u.iter_bits().collect();
        assert_eq!(collected, bits);
    }

    #[test]
    fn test_first_zero() {
        let mut u = U256::MAX;
        assert_eq!(u.first_zero(), None);

        u.clear_bit(50);
        assert_eq!(u.first_zero(), Some(50));

        u.clear_bit(150);
        assert_eq!(u.first_zero(), Some(50));

        u.set_bit(50);
        assert_eq!(u.first_zero(), Some(150));
    }

    #[test]
    fn test_first_one() {
        let mut u = U256::ZERO;
        assert_eq!(u.first_one(), None);

        u.set_bit(200);
        assert_eq!(u.first_one(), Some(200));

        u.set_bit(10);
        assert_eq!(u.first_one(), Some(10));
    }

    #[test]
    fn test_count_ones() {
        let mut u = U256::ZERO;
        assert_eq!(u.count_ones(), 0);

        u.set_bit(1);
        u.set_bit(129);
        assert_eq!(u.count_ones(), 2);

        u = U256::MAX;
        assert_eq!(u.count_ones(), 256);
    }
}
