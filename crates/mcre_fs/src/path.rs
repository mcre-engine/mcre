use std::{boxed::Box, string::String, vec, vec::Vec};
use core::fmt;

/// A platform-agnostic path composed of parts without separators.
///
/// This type represents a path as a sequence of components (e.g., ["app", "saves", "world1"])
/// without any platform-specific separators. It can be converted to:
/// - `std::path::PathBuf` for native platforms (using OS separator)
/// - OPFS path string (using forward slashes)
///
/// # Examples
/// ```
/// use mcre_fs::FsPath;
///
/// let path = FsPath::new(vec!["saves".into(), "world1".into(), "level.dat".into()]);
/// assert_eq!(path.to_opfs_string(), "saves/world1/level.dat");
///
/// // Convert to std path (uses OS separator)
/// let std_path = path.to_std_path();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FsPath {
    parts: Vec<Box<str>>,
}

impl FsPath {
    /// Creates a new path from parts.
    pub fn new(parts: Vec<Box<str>>) -> Self {
        Self { parts }
    }

    /// Creates an empty path.
    pub fn empty() -> Self {
        Self { parts: vec![] }
    }

    /// Creates a path with a single component.
    pub fn single(part: impl Into<Box<str>>) -> Self {
        Self {
            parts: vec![part.into()],
        }
    }

    /// Returns the path components.
    pub fn parts(&self) -> &[Box<str>] {
        &self.parts
    }

    /// Returns the number of path components.
    pub fn depth(&self) -> usize {
        self.parts.len()
    }

    /// Returns true if the path is empty.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Appends a component to the path.
    pub fn push(&mut self, part: impl Into<Box<str>>) {
        self.parts.push(part.into());
    }

    /// Appends a component and returns self for chaining.
    pub fn with(mut self, part: impl Into<Box<str>>) -> Self {
        self.push(part);
        self
    }

    /// Appends a component and returns a new path (non-consuming).
    pub fn join(&self, part: impl Into<Box<str>>) -> Self {
        let mut copy = self.clone();
        copy.push(part);
        copy
    }

    /// Returns the parent path (all components except the last).
    pub fn parent(&self) -> Option<FsPath> {
        if self.parts.is_empty() {
            None
        } else {
            Some(FsPath {
                parts: self.parts[..self.parts.len() - 1].to_vec(),
            })
        }
    }

    /// Returns the file name (last component), if any.
    pub fn file_name(&self) -> Option<&str> {
        self.parts.last().map(|s| s.as_ref())
    }

    /// Converts to a `std::path::PathBuf` using the OS separator.
    pub fn to_std_path(&self) -> std::path::PathBuf {
        let mut path = std::path::PathBuf::new();
        for part in &self.parts {
            path.push(part.as_ref());
        }
        path
    }

    /// Converts to a forward-slash separated string (for OPFS/web).
    pub fn to_opfs_string(&self) -> String {
        self.parts
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join("/")
    }
}

impl fmt::Display for FsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, part) in self.parts.iter().enumerate() {
            if i > 0 {
                write!(f, "/")?;
            }
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}

impl FromIterator<Box<str>> for FsPath {
    fn from_iter<I: IntoIterator<Item = Box<str>>>(iter: I) -> Self {
        Self {
            parts: iter.into_iter().collect(),
        }
    }
}

impl From<Vec<Box<str>>> for FsPath {
    fn from(parts: Vec<Box<str>>) -> Self {
        Self { parts }
    }
}

impl From<&str> for FsPath {
    fn from(s: &str) -> Self {
        Self {
            parts: s
                .split('/')
                .filter(|s| !s.is_empty())
                .map(Box::from)
                .collect(),
        }
    }
}

impl From<String> for FsPath {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed_strs(s: &[&str]) -> Vec<Box<str>> {
        s.iter().map(|s| Box::from(*s)).collect()
    }

    #[test]
    fn test_empty_path() {
        let path = FsPath::empty();
        assert!(path.is_empty());
        assert_eq!(path.depth(), 0);
        assert_eq!(path.file_name(), None);
        assert_eq!(path.to_opfs_string(), "");
    }

    #[test]
    fn test_single_part() {
        let path = FsPath::single("saves");
        assert_eq!(path.depth(), 1);
        assert_eq!(path.file_name(), Some("saves"));
        assert_eq!(path.to_opfs_string(), "saves");
    }

    #[test]
    fn test_multi_part() {
        let path = FsPath::new(vec!["saves".into(), "world1".into(), "level.dat".into()]);
        assert_eq!(path.depth(), 3);
        assert_eq!(path.file_name(), Some("level.dat"));
        assert_eq!(path.to_opfs_string(), "saves/world1/level.dat");
    }

    #[test]
    fn test_parent() {
        let path = FsPath::new(vec!["a".into(), "b".into(), "c".into()]);
        let parent = path.parent().unwrap();
        assert_eq!(parent.parts(), boxed_strs(&["a", "b"]).as_slice());
    }

    #[test]
    fn test_from_str() {
        let path = FsPath::from("saves/world1/level.dat");
        assert_eq!(
            path.parts(),
            boxed_strs(&["saves", "world1", "level.dat"]).as_slice()
        );
    }

    #[test]
    fn test_from_str_leading_slash() {
        let path = FsPath::from("/saves/world1");
        assert_eq!(
            path.parts(),
            boxed_strs(&["saves", "world1"]).as_slice()
        );
    }

    #[test]
    fn test_display() {
        let path = FsPath::new(vec!["a".into(), "b".into()]);
        assert_eq!(format!("{}", path), "a/b");
    }

    #[test]
    fn test_with_chaining() {
        let path = FsPath::single("root").with("child").with("leaf");
        assert_eq!(
            path.parts(),
            boxed_strs(&["root", "child", "leaf"]).as_slice()
        );
    }
}
