use alloc::boxed::Box;

use crate::RandomImpl;

#[derive(Clone, Debug)]
pub struct WeightedList<T> {
    elements: Box<[Weighted<T>]>,
    total_weight: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weighted<T> {
    pub value: T,
    pub weight: u8,
}

impl<T> WeightedList<T> {
    pub fn get_random(&self, source: &mut impl RandomImpl) -> Option<&T> {
        if self.total_weight == 0 {
            return None;
        }

        let mut n = source.next_bounded_i32(self.total_weight as i32) as u8;

        for entry in &self.elements {
            if n < entry.weight {
                return Some(&entry.value);
            }
            n -= entry.weight;
        }

        None
    }
}

impl<T> From<Box<[Weighted<T>]>> for WeightedList<T> {
    fn from(elements: Box<[Weighted<T>]>) -> Self {
        let total_weight = elements.iter().map(|e| e.weight).sum();
        WeightedList {
            elements,
            total_weight,
        }
    }
}
