//! Table types and the generic [`Table`] container.
//!
//! Tables store named, reusable definitions that entities reference:
//!
//! | Type | Purpose |
//! |------|----------|
//! | [`Layer`] | Drawing layers (color, linetype, visibility) |
//! | [`LineType`] | Dash patterns |
//! | [`TextStyle`] | Font / text formatting |
//! | [`DimStyle`] | Dimension appearance |
//! | [`BlockRecord`] | Block definition registry |
//! | [`AppId`] | Application identifier (XData) |
//! | [`View`] | Named view configurations |
//! | [`VPort`] | Viewport configurations |
//! | [`Ucs`] | User coordinate systems |

use crate::types::Handle;
use indexmap::IndexMap;

pub mod layer;
pub mod linetype;
pub mod textstyle;
pub mod block_record;
pub mod dimstyle;
pub mod appid;
pub mod view;
pub mod vport;
pub mod ucs;

pub use layer::{Layer, LayerFlags};
pub use linetype::{LineType, LineTypeComplexData, LineTypeComplexContent, LineTypeElement};
pub use textstyle::{TextStyle, TextGenerationFlags};
pub use block_record::BlockRecord;
pub use dimstyle::DimStyle;
pub use appid::AppId;
pub use view::View;
pub use vport::VPort;
pub use ucs::Ucs;

/// Base trait for all table entries
pub trait TableEntry {
    /// Get the entry's unique handle
    fn handle(&self) -> Handle;

    /// Set the entry's handle
    fn set_handle(&mut self, handle: Handle);

    /// Get the entry's name
    fn name(&self) -> &str;

    /// Set the entry's name
    fn set_name(&mut self, name: String);

    /// Check if this is a standard/default entry
    fn is_standard(&self) -> bool {
        false
    }
}

/// Generic table for storing named entries
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Table<T: TableEntry> {
    /// Entries stored by name (case-insensitive)
    entries: IndexMap<String, T>,
    /// Table handle
    handle: Handle,
}

impl<T: TableEntry> Table<T> {
    /// Create a new empty table
    pub fn new() -> Self {
        Table {
            entries: IndexMap::new(),
            handle: Handle::NULL,
        }
    }

    /// Create a table with a specific handle
    pub fn with_handle(handle: Handle) -> Self {
        Table {
            entries: IndexMap::new(),
            handle,
        }
    }

    /// Get the table's handle
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Set the table's handle
    pub fn set_handle(&mut self, handle: Handle) {
        self.handle = handle;
    }

    /// Add an entry to the table
    pub fn add(&mut self, entry: T) -> Result<(), String> {
        let name = entry.name().to_uppercase();
        if self.entries.contains_key(&name) {
            return Err(format!("Entry '{}' already exists in table", entry.name()));
        }
        self.entries.insert(name, entry);
        Ok(())
    }

    /// Add or replace an entry in the table (parsed data wins over defaults)
    pub fn add_or_replace(&mut self, entry: T) {
        let name = entry.name().to_uppercase();
        self.entries.insert(name, entry);
    }

    /// Add an entry while preserving existing entries with the same display
    /// name. This is needed for AutoCAD VPORT tables, where tiled model-space
    /// viewports can all be named "*Active".
    pub fn add_allow_duplicate(&mut self, entry: T) {
        let name = entry.name().to_uppercase();
        if !self.entries.contains_key(&name) {
            self.entries.insert(name, entry);
            return;
        }

        let handle = entry.handle();
        let mut key = if handle.is_valid() {
            format!("{}\u{0}{:X}", name, handle.value())
        } else {
            format!("{}\u{0}{}", name, self.entries.len())
        };
        let mut n = 1usize;
        while self.entries.contains_key(&key) {
            key = format!("{}\u{0}{}-{}", name, handle.value(), n);
            n += 1;
        }
        self.entries.insert(key, entry);
    }

    /// Get an entry by name (case-insensitive)
    pub fn get(&self, name: &str) -> Option<&T> {
        self.entries.get(&name.to_uppercase())
    }

    /// Get a mutable entry by name (case-insensitive)
    pub fn get_mut(&mut self, name: &str) -> Option<&mut T> {
        self.entries.get_mut(&name.to_uppercase())
    }

    /// Remove an entry by name (case-insensitive)
    pub fn remove(&mut self, name: &str) -> Option<T> {
        self.entries.shift_remove(&name.to_uppercase())
    }

    /// Check if an entry exists (case-insensitive)
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(&name.to_uppercase())
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.values()
    }

    /// Iterate over all entries mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.entries.values_mut()
    }

    /// Get all entry names
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.values().map(|e| e.name())
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl<T: TableEntry> Default for Table<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock table entry for testing
    #[derive(Debug, Clone)]
    struct MockEntry {
        handle: Handle,
        name: String,
    }

    impl TableEntry for MockEntry {
        fn handle(&self) -> Handle {
            self.handle
        }

        fn set_handle(&mut self, handle: Handle) {
            self.handle = handle;
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn set_name(&mut self, name: String) {
            self.name = name;
        }
    }

    #[test]
    fn test_table_add_and_get() {
        let mut table = Table::new();
        let entry = MockEntry {
            handle: Handle::new(1),
            name: "Test".to_string(),
        };
        
        assert!(table.add(entry).is_ok());
        assert!(table.contains("Test"));
        assert!(table.contains("test")); // Case-insensitive
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_table_duplicate_entry() {
        let mut table = Table::new();
        let entry1 = MockEntry {
            handle: Handle::new(1),
            name: "Test".to_string(),
        };
        let entry2 = MockEntry {
            handle: Handle::new(2),
            name: "test".to_string(), // Same name, different case
        };
        
        assert!(table.add(entry1).is_ok());
        assert!(table.add(entry2).is_err()); // Should fail
    }

    #[test]
    fn test_table_allow_duplicate_entries() {
        let mut table = Table::new();
        let entry1 = MockEntry {
            handle: Handle::new(1),
            name: "*Active".to_string(),
        };
        let entry2 = MockEntry {
            handle: Handle::new(2),
            name: "*Active".to_string(),
        };

        table.add_allow_duplicate(entry1);
        table.add_allow_duplicate(entry2);

        assert_eq!(table.len(), 2);
        assert_eq!(table.names().collect::<Vec<_>>(), vec!["*Active", "*Active"]);
        assert_eq!(table.get("*active").unwrap().handle(), Handle::new(1));
    }

    #[test]
    fn test_table_remove() {
        let mut table = Table::new();
        let entry = MockEntry {
            handle: Handle::new(1),
            name: "Test".to_string(),
        };
        
        table.add(entry).unwrap();
        assert_eq!(table.len(), 1);
        
        let removed = table.remove("test");
        assert!(removed.is_some());
        assert_eq!(table.len(), 0);
    }
}
