//! ImmutableArray - Immutable array with C# style API

use std::ops::Index;

#[derive(Debug, Clone)]
pub struct ImmutableArray<T> {
    data: Vec<T>,
}

impl<T> ImmutableArray<T> {
    /// Creates an empty ImmutableArray
    pub fn Empty() -> Self {
        Self { data: Vec::new() }
    }

    /// Creates an ImmutableArray from a Vec
    pub fn Create(items: Vec<T>) -> Self {
        Self { data: items }
    }

    /// Creates an ImmutableArray from an iterator
    pub fn CreateRange<I>(items: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        Self {
            data: items.into_iter().collect(),
        }
    }

    /// Gets the length of the array
    pub fn Length(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the array is empty
    pub fn IsEmpty(&self) -> bool {
        self.data.is_empty()
    }

    /// Gets an element at the specified index
    pub fn Get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Converts to a Vec
    pub fn ToVec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.data.clone()
    }

    /// Returns an iterator
    pub fn Iter(&self) -> std::slice::Iter<T> {
        self.data.iter()
    }
}

impl<T> Index<usize> for ImmutableArray<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T> FromIterator<T> for ImmutableArray<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::CreateRange(iter)
    }
}

impl<T> IntoIterator for ImmutableArray<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a ImmutableArray<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}