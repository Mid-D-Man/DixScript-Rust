//! Dictionary - Hash map with C# style API

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone, PartialEq)]
pub struct Dictionary<K, V> {
    data: HashMap<K, V>,
}

impl<K, V> Dictionary<K, V>
where
    K: Eq + Hash,
{
    /// Creates a new empty Dictionary
    pub fn New() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Creates a Dictionary with specified capacity
    pub fn WithCapacity(capacity: usize) -> Self {
        Self {
            data: HashMap::with_capacity(capacity),
        }
    }

    /// Adds a key-value pair
    pub fn Add(&mut self, key: K, value: V) {
        self.data.insert(key, value);
    }

    /// Sets a value for a key (C# indexer setter)
    pub fn Set(&mut self, key: K, value: V) {
        self.data.insert(key, value);
    }

    /// Gets a value by key
    pub fn Get(&self, key: &K) -> Option<&V> {
        self.data.get(key)
    }

    /// Gets a mutable reference to a value
    pub fn GetMut(&mut self, key: &K) -> Option<&mut V> {
        self.data.get_mut(key)
    }

    /// Tries to get a value (C# TryGetValue)
    pub fn TryGetValue(&self, key: &K) -> Option<&V> {
        self.data.get(key)
    }

    /// Removes a key-value pair
    pub fn Remove(&mut self, key: &K) -> Option<V> {
        self.data.remove(key)
    }

    /// Checks if a key exists
    pub fn ContainsKey(&self, key: &K) -> bool {
        self.data.contains_key(key)
    }

    /// Checks if a value exists
    pub fn ContainsValue(&self, value: &V) -> bool
    where
        V: PartialEq,
    {
        self.data.values().any(|v| v == value)
    }

    /// Gets the number of key-value pairs
    pub fn Count(&self) -> usize {
        self.data.len()
    }

    /// Returns true if empty
    pub fn IsEmpty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clears all entries
    pub fn Clear(&mut self) {
        self.data.clear();
    }

    /// Returns an iterator over keys
    pub fn Keys(&self) -> impl Iterator<Item = &K> {
        self.data.keys()
    }

    /// Returns an iterator over values
    pub fn Values(&self) -> impl Iterator<Item = &V> {
        self.data.values()
    }

    /// Returns an iterator over key-value pairs
    pub fn Iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.data.iter()
    }

    /// Returns a mutable iterator over key-value pairs
    pub fn IterMut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.data.iter_mut()
    }

    // ========== LINQ-like Methods ==========

    /// Select: Maps values to a new form
    pub fn Select<U, F>(&self, selector: F) -> crate::DixCore::List<U>
    where
        F: Fn(&K, &V) -> U,
    {
        crate::DixCore::List::From(
            self.data.iter().map(|(k, v)| selector(k, v)).collect()
        )
    }

    /// Where: Filters entries
    pub fn Where<F>(&self, predicate: F) -> Dictionary<K, V>
    where
        F: Fn(&K, &V) -> bool,
        K: Clone,
        V: Clone,
    {
        let filtered: HashMap<K, V> = self
            .data
            .iter()
            .filter(|(k, v)| predicate(k, v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Dictionary { data: filtered }
    }

    /// Any: Returns true if any entry matches predicate
    pub fn Any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        self.data.iter().any(|(k, v)| predicate(k, v))
    }

    /// All: Returns true if all entries match predicate
    pub fn All<F>(&self, predicate: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        self.data.iter().all(|(k, v)| predicate(k, v))
    }

    /// ToList: Converts to a list of tuples
    pub fn ToList(&self) -> crate::DixCore::List<(K, V)>
    where
        K: Clone,
        V: Clone,
    {
        crate::DixCore::List::From(
            self.data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        )
    }
}

impl<K, V> Default for Dictionary<K, V>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self::New()
    }
}

impl<K, V> FromIterator<(K, V)> for Dictionary<K, V>
where
    K: Eq + Hash,
{
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self {
            data: iter.into_iter().collect(),
        }
    }
}