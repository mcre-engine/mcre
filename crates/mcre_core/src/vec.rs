use core::{
    array, mem,
    ops::{Add, Deref, DerefMut, Index, IndexMut, Sub},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VecN<T, const LEN: usize>([T; LEN]);

pub type Vec3i = VecN<i32, 3>;
pub type Vec3u = VecN<u32, 3>;
pub type Vec3f = VecN<f32, 3>;
pub type Vec4i = VecN<i32, 4>;
pub type Vec4u = VecN<u32, 4>;
pub type Vec4f = VecN<f32, 4>;

impl<T, const LEN: usize> Serialize for VecN<T, LEN>
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

impl<'de, T, const LEN: usize> Deserialize<'de> for VecN<T, LEN>
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

impl<T, const LEN: usize> VecN<T, LEN> {
    pub fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.0.get_mut(index)
    }

    pub fn set(&mut self, index: usize, value: T) -> Option<T> {
        self.0.get_mut(index).map(|slot| mem::replace(slot, value))
    }
}

impl<T, const LEN: usize> Deref for VecN<T, LEN> {
    type Target = [T; LEN];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, const LEN: usize> DerefMut for VecN<T, LEN> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T, const LEN: usize> Index<usize> for VecN<T, LEN> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<T, const LEN: usize> IndexMut<usize> for VecN<T, LEN> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl<T> VecN<T, 3> {
    pub fn new(x: T, y: T, z: T) -> Self {
        Self([x, y, z])
    }
}

impl<T> VecN<T, 4> {
    pub fn new(x: T, y: T, z: T, w: T) -> Self {
        Self([x, y, z, w])
    }
}

impl<T: Add + Copy, const LEN: usize> Add for VecN<T, LEN> {
    type Output = VecN<T::Output, LEN>;

    fn add(self, other: Self) -> Self::Output {
        VecN(array::from_fn(|i| self.0[i] + other.0[i]))
    }
}

impl<T: Sub + Copy, const LEN: usize> Sub for VecN<T, LEN> {
    type Output = VecN<T::Output, LEN>;

    fn sub(self, other: Self) -> Self::Output {
        VecN(array::from_fn(|i| self.0[i] - other.0[i]))
    }
}
