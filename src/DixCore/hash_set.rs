//! HashSet - Hash set with C# style API

use std::collections::HashSet as StdHashSet;
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct HashSet<T> {
    data: StdHashSet<T>,
}

impl<T> HashSet<T>
where
    T: Eq + Hash,
{
    /// Creates a new empty HashSet
    pub fn New() -> Self {
        Self {
            data: StdHashSet::new(),
        }
    }

    /// Creates a HashSet with specified capacity
    pub fn WithCapacity(capacity: usize) -> Self {
        Self {
            data: StdHashSet::with_capacity(capacity),
        }
    }

    /// Adds an item
    pub fn Add(&mut self, item: T) -> bool {
        self.data.insert(item)
    }

    /// Removes an item
    pub fn Remove(&mut self, item: &T) -> bool {
        self.data.remove(item)
    }

    /// Checks if an item exists
    pub fn Contains(&self, item: &T) -> bool {
        self.data.contains(item)
    }

    /// Gets the number of items
    pub fn Count(&self) -> usize {
        self.data.len()
    }

    /// Returns true if empty
    pub fn IsEmpty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clears all items
    pub fn Clear(&mut self) {
        self.data.clear();
    }

    /// Returns an iterator
    pub fn Iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    // ========== Set Operations ==========

    /// Union: Returns items in either set
    pub fn UnionWith(&self, other: &HashSet<T>) -> HashSet<T>
    where
        T: Clone,
    {
        HashSet {
            data: self.data.union(&other.data).cloned().collect(),
        }
    }

    /// Intersect: Returns items in both sets
    pub fn IntersectWith(&self, other: &HashSet<T>) -> HashSet<T>
    where
        T: Clone,
    {
        HashSet {
            data: self.data.intersection(&other.data).cloned().collect(),
        }
    }

    /// ExceptWith: Returns items in this set but not in other
    pub fn ExceptWith(&self, other: &HashSet<T>) -> HashSet<T>
    where
        T: Clone,
    {
        HashSet {
            data: self.data.difference(&other.data).cloned().collect(),
        }
    }

    /// IsSubsetOf: Returns true if this is a subset of other
    pub fn IsSubsetOf(&self, other: &HashSet<T>) -> bool {
        self.data.is_subset(&other.data)
    }

    /// IsSupersetOf: Returns true if this is a superset of other
    pub fn IsSupersetOf(&self, other: &HashSet<T>) -> bool {
        self.data.is_superset(&other.data)
    }

    // ========== LINQ-like Methods ==========

    /// Where: Filters items
    pub fn Where<F>(&self, predicate: F) -> HashSet<T>
    where
        F: Fn(&T) -> bool,
        T: Clone,
    {
        HashSet {
            data: self.data.iter().filter(|x| predicate(x)).cloned().collect(),
        }
    }

    /// Any: Returns true if any item matches predicate
    pub fn Any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.data.iter().any(predicate)
    }

    /// All: Returns true if all items match predicate
    pub fn All<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.data.iter().all(predicate)
    }

    /// ToList: Converts to a List
    pub fn ToList(&self) -> crate::DixCore::List<T>
    where
        T: Clone,
    {
        crate::DixCore::List::From(self.data.iter().cloned().collect())
    }
}

impl<T> Default for HashSet<T>
where
    T: Eq + Hash,
{
    fn default() -> Self {
        Self::New()
    }
}

impl<T> FromIterator<T> for HashSet<T>
where
    T: Eq + Hash,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            data: iter.into_iter().collect(),
        }
    }
}