use std::{
    any::type_name,
    fmt::{self, Debug, Display},
    marker::PhantomData,
};

use serde::{Deserialize, Deserializer};

pub struct NamespacedId<S> {
    pub namespace: String,
    pub id: String,
    _marker: PhantomData<S>,
}

pub trait AssetScope {
    const NAME: &'static str;
}

pub struct BlockModelScope;

impl AssetScope for BlockModelScope {
    const NAME: &'static str = "block";
}

pub type BlockModelId = NamespacedId<BlockModelScope>;

pub struct BlockTextureScope;

impl AssetScope for BlockTextureScope {
    const NAME: &'static str = "block";
}

pub type BlockTextureId = NamespacedId<BlockTextureScope>;

pub struct ItemTextureScope;

impl AssetScope for ItemTextureScope {
    const NAME: &'static str = "item";
}

pub type ItemTextureId = NamespacedId<ItemTextureScope>;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TextureId {
    Block(BlockTextureId),
    Item(ItemTextureId),
}

impl<S: AssetScope> NamespacedId<S> {
    pub fn new(namespace: String, id: String) -> Self {
        Self {
            namespace,
            id,
            _marker: PhantomData,
        }
    }
}

impl<S: AssetScope> Clone for NamespacedId<S> {
    fn clone(&self) -> Self {
        Self {
            namespace: self.namespace.clone(),
            id: self.id.clone(),
            _marker: PhantomData,
        }
    }
}

impl<S: AssetScope> Debug for NamespacedId<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(&format!("NamespacedId<{}>", type_name::<S>()))
            .field("namespace", &self.namespace)
            .field("id", &self.id)
            .finish()
    }
}

impl<S: AssetScope> Display for NamespacedId<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}/{}", self.namespace, S::NAME, self.id)
    }
}

impl<'a, S: AssetScope> Deserialize<'a> for NamespacedId<S> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.split(':').collect();

        let (namespace, path) = if parts.len() == 1 {
            ("minecraft".to_string(), parts[0])
        } else if parts.len() == 2 {
            (parts[0].to_string(), parts[1])
        } else {
            return Err(serde::de::Error::custom("invalid format"));
        };

        let parts: Vec<&str> = path.split("/").collect();

        if parts.len() != 2 {
            return Err(serde::de::Error::custom("invalid format"));
        }

        if parts[0] != S::NAME {
            return Err(serde::de::Error::custom("invalid scope"));
        }

        let id = parts[1].to_string();

        Ok(NamespacedId {
            namespace,
            id,
            _marker: PhantomData,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum RefOr<T> {
    Ref(ReferenceId),
    Value(T),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReferenceId(String);

impl ReferenceId {
    pub fn new(id: String) -> Self {
        ReferenceId(id)
    }
}

impl Display for ReferenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl<'de> Deserialize<'de> for ReferenceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let stripped = s.strip_prefix('#').unwrap_or(&s);
        if !stripped.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(serde::de::Error::custom("invalid format"));
        }
        Ok(ReferenceId(stripped.to_string()))
    }
}
