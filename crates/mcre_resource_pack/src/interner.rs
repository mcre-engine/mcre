use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// A string interner that maps strings to compact `u32` integer IDs.
///
/// Each unique string is assigned a monotonically increasing ID.
/// The interner can resolve IDs back to strings.
#[derive(Debug, Clone)]
pub struct StringInterner {
    strings: Vec<Box<str>>,
    map: FxHashMap<Box<str>, u32>,
}

impl StringInterner {
    /// Creates a new empty interner with pre-allocated capacity.
    pub fn new() -> Self {
        Self {
            strings: Vec::with_capacity(1024),
            map: HashMap::with_capacity_and_hasher(1024, FxBuildHasher),
        }
    }

    /// Creates a new empty interner with the given initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            strings: Vec::with_capacity(capacity),
            map: HashMap::with_capacity_and_hasher(capacity, FxBuildHasher),
        }
    }

    /// Interns a string, returning its unique `u32` ID.
    /// If the string was already interned, returns the existing ID.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        let boxed: Box<str> = s.into();
        self.map.insert(boxed.clone(), id);
        self.strings.push(boxed);
        id
    }

    /// Resolves an ID back to its string, or `None` if the ID is invalid.
    pub fn resolve(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_ref())
    }

    /// Resolves an ID back to its string.
    ///
    /// # Panics
    /// Panics if the ID is out of bounds.
    pub fn resolve_unchecked(&self, id: u32) -> &str {
        &self.strings[id as usize]
    }

    /// Returns the number of unique interned strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Returns `true` if no strings are interned.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Returns a reference to the string table (indexed by ID).
    pub fn strings(&self) -> &[Box<str>] {
        &self.strings
    }

    /// Consumes the interner and returns the string table.
    pub fn into_strings(self) -> Vec<Box<str>> {
        self.strings
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_resolve() {
        let mut interner = StringInterner::new();
        let id1 = interner.intern("hello");
        let id2 = interner.intern("world");
        let id3 = interner.intern("hello");

        assert_eq!(id1, id3, "interning same string returns same ID");
        assert_ne!(id1, id2, "different strings get different IDs");
        assert_eq!(interner.resolve(id1), Some("hello"));
        assert_eq!(interner.resolve(id2), Some("world"));
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn test_invalid_id() {
        let interner = StringInterner::new();
        assert_eq!(interner.resolve(0), None);
        assert_eq!(interner.resolve(999), None);
    }

    #[test]
    fn test_empty() {
        let interner = StringInterner::new();
        assert!(interner.is_empty());
        assert_eq!(interner.len(), 0);
    }

    #[test]
    fn test_into_strings() {
        let mut interner = StringInterner::new();
        interner.intern("a");
        interner.intern("b");
        let strings = interner.into_strings();
        assert_eq!(strings.len(), 2);
    }
}
