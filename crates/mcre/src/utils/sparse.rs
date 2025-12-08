use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
};

use bevy::reflect::Reflect;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Reflect, Deserialize, Serialize)]
pub struct SparseVec<T> {
    data: HashMap<usize, T, BuildHasherDefault<SparseHasher>>,
}

impl<T> SparseVec<T> {
    pub fn empty() -> Self {
        SparseVec {
            data: HashMap::default(),
        }
    }

    /// Inserts into sparse array as long as the index is less than `N`
    /// Replaces old value if there
    pub fn insert(&mut self, idx: usize, value: T) -> Option<T> {
        self.data.insert(idx, value)
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        self.data.get(&idx)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        self.data.get_mut(&idx)
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.data.iter().map(|(i, t)| (*i, t))
    }

    pub fn from<U: Into<T>>(value: SparseVec<U>) -> Self {
        SparseVec {
            data: value.data.into_iter().map(|(i, v)| (i, v.into())).collect(),
        }
    }
}

#[derive(Default, Reflect)]
struct SparseHasher(usize);

impl Hasher for SparseHasher {
    fn finish(&self) -> u64 {
        self.0 as u64
    }

    fn write(&mut self, _bytes: &[u8]) {
        unimplemented!("IdentityHasher only supports usize keys")
    }
    fn write_usize(&mut self, i: usize) {
        self.0 = i;
    }
}

impl<T> FromIterator<(usize, T)> for SparseVec<T> {
    fn from_iter<I: IntoIterator<Item = (usize, T)>>(iter: I) -> Self {
        let data = HashMap::from_iter(iter);
        SparseVec { data }
    }
}
