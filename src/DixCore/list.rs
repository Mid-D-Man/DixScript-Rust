//! List - Dynamic list with C# style API and LINQ methods

use std::ops::{Index, IndexMut};

#[derive(Debug, Clone)]
pub struct List<T> {
    data: Vec<T>,
}

impl<T> List<T> {
    /// Creates a new empty List
    pub fn New() -> Self {
        Self { data: Vec::new() }
    }

    /// Creates a List with specified capacity
    pub fn WithCapacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Creates a List from a Vec
    pub fn From(items: Vec<T>) -> Self {
        Self { data: items }
    }

    // ========== Basic Operations ==========

    /// Adds an item to the end
    pub fn Add(&mut self, item: T) {
        self.data.push(item);
    }

    /// Adds multiple items
    pub fn AddRange<I>(&mut self, items: I)
    where
        I: IntoIterator<Item = T>,
    {
        self.data.extend(items);
    }

    /// Inserts an item at the specified index
    pub fn Insert(&mut self, index: usize, item: T) {
        self.data.insert(index, item);
    }

    /// Removes an item at the specified index
    pub fn RemoveAt(&mut self, index: usize) -> T {
        self.data.remove(index)
    }

    /// Removes the first occurrence of a specific item
    pub fn Remove(&mut self, item: &T) -> bool
    where
        T: PartialEq,
    {
        if let Some(pos) = self.data.iter().position(|x| x == item) {
            self.data.remove(pos);
            true
        } else {
            false
        }
    }

    /// Removes all items
    pub fn Clear(&mut self) {
        self.data.clear();
    }

    /// Gets the number of items
    pub fn Count(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the list is empty
    pub fn IsEmpty(&self) -> bool {
        self.data.is_empty()
    }

    /// Gets an item at the specified index
    pub fn Get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Gets a mutable reference to an item
    pub fn GetMut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }

    /// Checks if the list contains an item
    pub fn Contains(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        self.data.contains(item)
    }

    /// Finds the index of an item
    pub fn IndexOf(&self, item: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.data.iter().position(|x| x == item)
    }

    /// Converts to a Vec
    pub fn ToVec(self) -> Vec<T> {
        self.data
    }

    /// Returns an iterator
    pub fn Iter(&self) -> std::slice::Iter<T> {
        self.data.iter()
    }

    /// Returns a mutable iterator
    pub fn IterMut(&mut self) -> std::slice::IterMut<T> {
        self.data.iter_mut()
    }

    // ========== LINQ Methods (C# Style) ==========

    /// Select: Maps each element to a new form (C# Select)
    pub fn Select<U, F>(&self, selector: F) -> List<U>
    where
        F: Fn(&T) -> U,
    {
        List::From(self.data.iter().map(selector).collect())
    }

    /// Where: Filters elements (C# Where)
    pub fn Where<F>(&self, predicate: F) -> List<T>
    where
        F: Fn(&T) -> bool,
        T: Clone,
    {
        List::From(self.data.iter().filter(|x| predicate(x)).cloned().collect())
    }

    /// First: Returns the first element (panics if empty)
    pub fn First(&self) -> Option<&T> {
        self.data.first()
    }

    /// FirstOrDefault: Returns the first element or None
    pub fn FirstOrDefault(&self) -> Option<&T> {
        self.data.first()
    }

    /// Last: Returns the last element
    pub fn Last(&self) -> Option<&T> {
        self.data.last()
    }

    /// Any: Returns true if any element matches the predicate
    pub fn Any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.data.iter().any(predicate)
    }

    /// All: Returns true if all elements match the predicate
    pub fn All<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.data.iter().all(predicate)
    }

    /// Count: Returns the number of elements matching the predicate
    pub fn CountWhere<F>(&self, predicate: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        self.data.iter().filter(|x| predicate(x)).count()
    }

    /// OrderBy: Sorts elements by a key (C# OrderBy)
    pub fn OrderBy<K, F>(&self, key_selector: F) -> List<T>
    where
        F: Fn(&T) -> K,
        K: Ord,
        T: Clone,
    {
        let mut sorted = self.data.clone();
        sorted.sort_by_key(key_selector);
        List::From(sorted)
    }

    /// OrderByDescending: Sorts elements by a key in descending order
    pub fn OrderByDescending<K, F>(&self, key_selector: F) -> List<T>
    where
        F: Fn(&T) -> K,
        K: Ord,
        T: Clone,
    {
        let mut sorted = self.data.clone();
        sorted.sort_by_key(key_selector);
        sorted.reverse();
        List::From(sorted)
    }

    /// Take: Takes the first n elements
    pub fn Take(&self, count: usize) -> List<T>
    where
        T: Clone,
    {
        List::From(self.data.iter().take(count).cloned().collect())
    }

    /// Skip: Skips the first n elements
    pub fn Skip(&self, count: usize) -> List<T>
    where
        T: Clone,
    {
        List::From(self.data.iter().skip(count).cloned().collect())
    }

    /// Distinct: Returns unique elements (C# Distinct)
    pub fn Distinct(&self) -> List<T>
    where
        T: Clone + Eq + std::hash::Hash,
    {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for item in &self.data {
            if seen.insert(item.clone()) {
                result.push(item.clone());
            }
        }
        List::From(result)
    }

    /// Sum: Sums numeric elements
    pub fn Sum(&self) -> T
    where
        T: Clone + std::ops::Add<Output = T> + Default,
    {
        self.data.iter().cloned().fold(T::default(), |acc, x| acc + x)
    }

    /// Min: Returns the minimum element
    pub fn Min(&self) -> Option<&T>
    where
        T: Ord,
    {
        self.data.iter().min()
    }

    /// Max: Returns the maximum element
    pub fn Max(&self) -> Option<&T>
    where
        T: Ord,
    {
        self.data.iter().max()
    }

    /// Reverse: Reverses the list
    pub fn Reverse(&mut self) {
        self.data.reverse();
    }

    /// ToArray: Converts to an array (ImmutableArray)
    pub fn ToArray(&self) -> crate::DixCore::ImmutableArray<T>  //With <T>
    where
        T: Clone,
    {
        crate::DixCore::ImmutableArray::Create(self.data.clone())
    }
}

impl<T> Index<usize> for List<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T> IndexMut<usize> for List<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

impl<T> FromIterator<T> for List<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::From(iter.into_iter().collect())
    }
}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self::New()
    }
}

// ========== IntoIterator Implementations ==========

/// Consuming iterator - takes ownership of List
impl<T> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

/// Borrowing iterator - iterates over references
impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

/// Mutable borrowing iterator - iterates over mutable references
impl<'a, T> IntoIterator for &'a mut List<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter_mut()
    }
}