use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vec<T, const LEN: usize>([T; LEN]);

pub type Vec3i = Vec<i32, 3>;
pub type Vec3u = Vec<u32, 3>;
pub type Vec3f = Vec<f32, 3>;
pub type Vec4i = Vec<i32, 4>;
pub type Vec4u = Vec<u32, 4>;
pub type Vec4f = Vec<f32, 4>;

impl<T, const LEN: usize> Serialize for Vec<T, LEN>
where
    [T; LEN]: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, T, const LEN: usize> Deserialize<'de> for Vec<T, LEN>
where
    [T; LEN]: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let array = <[T; LEN]>::deserialize(deserializer)?;
        Ok(Self(array))
    }
}

impl<T, const LEN: usize> Vec<T, LEN> {
    pub fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.0.get_mut(index)
    }

    pub fn set(&mut self, index: usize, value: T) -> Option<T> {
        self.0
            .get_mut(index)
            .map(|slot| std::mem::replace(slot, value))
    }
}

impl<T, const LEN: usize> Index<usize> for Vec<T, LEN> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<T, const LEN: usize> IndexMut<usize> for Vec<T, LEN> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}
