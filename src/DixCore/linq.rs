//! LINQ - C# LINQ Extension Methods for Rust
//!
//! Provides C# style LINQ operations that work on any collection

use crate::DixCore::{List, ImmutableArray, Dictionary};
use std::collections::HashMap;
use std::hash::Hash;

/// LINQ operations for iterables
pub struct Linq;

impl Linq {
    // ========== Projection ==========

    /// Select: Projects each element (C# Select)
    pub fn Select<T, U, F, I>(source: I, selector: F) -> List<U>
    where
        I: IntoIterator<Item = T>,
        F: Fn(T) -> U,
    {
        let result: Vec<U> = source.into_iter().map(selector).collect();
        List::From(result)
    }

    /// SelectMany: Projects each element to a sequence and flattens
    pub fn SelectMany<T, U, F, I>(source: I, selector: F) -> List<U>
    where
        I: IntoIterator<Item = T>,
        F: Fn(T) -> Vec<U>,
    {
        let result: Vec<U> = source.into_iter().flat_map(selector).collect();
        List::From(result)
    }

    // ========== Filtering ==========

    /// Where: Filters elements (C# Where)
    pub fn Where<T, F, I>(source: I, predicate: F) -> List<T>
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> bool,
    {
        let result: Vec<T> = source.into_iter().filter(predicate).collect();
        List::From(result)
    }

    // ========== Ordering ==========

    /// OrderBy: Sorts by key (C# OrderBy)
    pub fn OrderBy<T, K, F, I>(source: I, key_selector: F) -> List<T>
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> K,
        K: Ord,
    {
        let mut items: Vec<T> = source.into_iter().collect();
        items.sort_by_key(key_selector);
        List::From(items)
    }

    /// OrderByDescending: Sorts by key descending
    pub fn OrderByDescending<T, K, F, I>(source: I, key_selector: F) -> List<T>
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> K,
        K: Ord,
    {
        let mut items: Vec<T> = source.into_iter().collect();
        items.sort_by_key(key_selector);
        items.reverse();
        List::From(items)
    }

    /// Reverse: Reverses the sequence
    pub fn Reverse<T, I>(source: I) -> List<T>
    where
        I: IntoIterator<Item = T>,
    {
        let mut items: Vec<T> = source.into_iter().collect();
        items.reverse();
        List::From(items)
    }

    // ========== Quantifiers ==========

    /// Any: Returns true if any element matches
    pub fn Any<T, F, I>(source: I, mut predicate: F) -> bool
    where
        I: IntoIterator<Item = T>,
        F: FnMut(&T) -> bool,
    {
        source.into_iter().any(|x| predicate(&x))
    }

    /// All: Returns true if all elements match
    pub fn All<T, F, I>(source: I, mut predicate: F) -> bool
    where
        I: IntoIterator<Item = T>,
        F: FnMut(&T) -> bool,
    {
        source.into_iter().all(|x| predicate(&x))
    }

    /// Contains: Returns true if sequence contains item
    pub fn Contains<T, I>(source: I, item: &T) -> bool
    where
        I: IntoIterator<Item = T>,
        T: PartialEq,
    {
        source.into_iter().any(|x| &x == item)
    }

    // ========== Element Operators ==========

    /// First: Returns first element
    pub fn First<T, I>(source: I) -> Option<T>
    where
        I: IntoIterator<Item = T>,
    {
        source.into_iter().next()
    }

    /// FirstOrDefault: Returns first element or None
    pub fn FirstOrDefault<T, I>(source: I) -> Option<T>
    where
        I: IntoIterator<Item = T>,
    {
        source.into_iter().next()
    }

    /// Last: Returns last element
    pub fn Last<T, I>(source: I) -> Option<T>
    where
        I: IntoIterator<Item = T>,
    {
        source.into_iter().last()
    }

    /// Single: Returns the only element (panics if not exactly one)
    pub fn Single<T, I>(source: I) -> Option<T>
    where
        I: IntoIterator<Item = T>,
    {
        let mut iter = source.into_iter();
        let first = iter.next()?;
        if iter.next().is_some() {
            panic!("Sequence contains more than one element");
        }
        Some(first)
    }

    /// ElementAt: Returns element at index
    pub fn ElementAt<T, I>(source: I, index: usize) -> Option<T>
    where
        I: IntoIterator<Item = T>,
    {
        source.into_iter().nth(index)
    }

    // ========== Aggregation ==========

    /// Count: Counts elements
    pub fn Count<T, I>(source: I) -> usize
    where
        I: IntoIterator<Item = T>,
    {
        source.into_iter().count()
    }

    /// CountWhere: Counts elements matching predicate
    pub fn CountWhere<T, F, I>(source: I, predicate: F) -> usize
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> bool,
    {
        source.into_iter().filter(predicate).count()
    }

    /// Sum: Sums numeric elements
    pub fn Sum<T, I>(source: I) -> T
    where
        I: IntoIterator<Item = T>,
        T: std::ops::Add<Output = T> + Default,
    {
        source.into_iter().fold(T::default(), |acc, x| acc + x)
    }

    /// Min: Returns minimum element
    pub fn Min<T, I>(source: I) -> Option<T>
    where
        I: IntoIterator<Item = T>,
        T: Ord,
    {
        source.into_iter().min()
    }

    /// Max: Returns maximum element
    pub fn Max<T, I>(source: I) -> Option<T>
    where
        I: IntoIterator<Item = T>,
        T: Ord,
    {
        source.into_iter().max()
    }

    /// Average: Returns average of numeric elements
    pub fn Average<T, I>(source: I) -> Option<f64>
    where
        I: IntoIterator<Item = T>,
        T: Into<f64>,
    {
        let items: Vec<f64> = source.into_iter().map(|x| x.into()).collect();
        if items.is_empty() {
            None
        } else {
            Some(items.iter().sum::<f64>() / items.len() as f64)
        }
    }

    // ========== Partitioning ==========

    /// Take: Takes first n elements
    pub fn Take<T, I>(source: I, count: usize) -> List<T>
    where
        I: IntoIterator<Item = T>,
    {
        let result: Vec<T> = source.into_iter().take(count).collect();
        List::From(result)
    }

    /// Skip: Skips first n elements
    pub fn Skip<T, I>(source: I, count: usize) -> List<T>
    where
        I: IntoIterator<Item = T>,
    {
        let result: Vec<T> = source.into_iter().skip(count).collect();
        List::From(result)
    }

    /// TakeWhile: Takes elements while predicate is true
    pub fn TakeWhile<T, F, I>(source: I, predicate: F) -> List<T>
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> bool,
    {
        let result: Vec<T> = source.into_iter().take_while(predicate).collect();
        List::From(result)
    }

    /// SkipWhile: Skips elements while predicate is true
    pub fn SkipWhile<T, F, I>(source: I, predicate: F) -> List<T>
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> bool,
    {
        let result: Vec<T> = source.into_iter().skip_while(predicate).collect();
        List::From(result)
    }

    // ========== Set Operations ==========

    /// Distinct: Returns unique elements
    pub fn Distinct<T, I>(source: I) -> List<T>
    where
        I: IntoIterator<Item = T>,
        T: Eq + Hash + Clone,
    {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for item in source.into_iter() {
            if seen.insert(item.clone()) {
                result.push(item);
            }
        }
        List::From(result)
    }

    /// Union: Returns elements from both sequences
    pub fn Union<T, I1, I2>(first: I1, second: I2) -> List<T>
    where
        I1: IntoIterator<Item = T>,
        I2: IntoIterator<Item = T>,
        T: Eq + Hash + Clone,
    {
        let mut set = std::collections::HashSet::new();
        let mut result = Vec::new();

        for item in first.into_iter().chain(second.into_iter()) {
            if set.insert(item.clone()) {
                result.push(item);
            }
        }
        List::From(result)
    }

    /// Intersect: Returns elements present in both sequences
    pub fn Intersect<T, I1, I2>(first: I1, second: I2) -> List<T>
    where
        I1: IntoIterator<Item = T>,
        I2: IntoIterator<Item = T>,
        T: Eq + Hash + Clone,
    {
        let second_set: std::collections::HashSet<T> = second.into_iter().collect();
        let mut result_set = std::collections::HashSet::new();
        let mut result = Vec::new();

        for item in first.into_iter() {
            if second_set.contains(&item) && result_set.insert(item.clone()) {
                result.push(item);
            }
        }
        List::From(result)
    }

    /// Except: Returns elements in first but not in second
    pub fn Except<T, I1, I2>(first: I1, second: I2) -> List<T>
    where
        I1: IntoIterator<Item = T>,
        I2: IntoIterator<Item = T>,
        T: Eq + Hash + Clone,
    {
        let second_set: std::collections::HashSet<T> = second.into_iter().collect();
        let mut result_set = std::collections::HashSet::new();
        let mut result = Vec::new();

        for item in first.into_iter() {
            if !second_set.contains(&item) && result_set.insert(item.clone()) {
                result.push(item);
            }
        }
        List::From(result)
    }

    // ========== Conversion ==========

    /// ToList: Converts to List
    pub fn ToList<T, I>(source: I) -> List<T>
    where
        I: IntoIterator<Item = T>,
    {
        let result: Vec<T> = source.into_iter().collect();
        List::From(result)
    }

    /// ToArray: Converts to ImmutableArray
    pub fn ToArray<T, I>(source: I) -> ImmutableArray<T>
    where
        I: IntoIterator<Item = T>,
    {
        let result: Vec<T> = source.into_iter().collect();
        ImmutableArray::Create(result)
    }

    /// ToDictionary: Converts to Dictionary by key selector
    pub fn ToDictionary<T, K, V, FK, FV, I>(
        source: I,
        key_selector: FK,
        value_selector: FV,
    ) -> Dictionary<K, V>
    where
        I: IntoIterator<Item = T>,
        FK: Fn(&T) -> K,
        FV: Fn(&T) -> V,
        K: Eq + Hash,
    {
        let map: HashMap<K, V> = source
            .into_iter()
            .map(|item| (key_selector(&item), value_selector(&item)))
            .collect();

        let mut dict = Dictionary::New();
        for (k, v) in map {
            dict.Add(k, v);
        }
        dict
    }

    // ========== Grouping ==========

    /// GroupBy: Groups elements by key
    pub fn GroupBy<T, K, F, I>(source: I, key_selector: F) -> Dictionary<K, List<T>>
    where
        I: IntoIterator<Item = T>,
        F: Fn(&T) -> K,
        K: Eq + Hash + Clone,
    {
        let mut groups: HashMap<K, Vec<T>> = HashMap::new();

        for item in source.into_iter() {
            let key = key_selector(&item);
            groups.entry(key).or_insert_with(Vec::new).push(item);
        }

        let mut dict = Dictionary::New();
        for (k, v) in groups {
            dict.Add(k, List::From(v));
        }
        dict
    }

    // ========== Join Operations ==========

    /// Join: Correlates elements of two sequences
    pub fn Join<T1, T2, K, R, FK1, FK2, FR, I1, I2>(
        outer: I1,
        inner: I2,
        outer_key_selector: FK1,
        inner_key_selector: FK2,
        result_selector: FR,
    ) -> List<R>
    where
        I1: IntoIterator<Item = T1>,
        I2: IntoIterator<Item = T2>,
        FK1: Fn(&T1) -> K,
        FK2: Fn(&T2) -> K,
        FR: Fn(&T1, &T2) -> R,
        K: Eq + Hash,
        T2: Clone,
    {
        let inner_lookup: HashMap<K, Vec<T2>> = {
            let mut map = HashMap::new();
            for item in inner.into_iter() {
                let key = inner_key_selector(&item);
                map.entry(key).or_insert_with(Vec::new).push(item);
            }
            map
        };

        let mut results = Vec::new();
        for outer_item in outer.into_iter() {
            let key = outer_key_selector(&outer_item);
            if let Some(inner_items) = inner_lookup.get(&key) {
                for inner_item in inner_items {
                    results.push(result_selector(&outer_item, inner_item));
                }
            }
        }

        List::From(results)
    }
}

// ========== Extension Trait for Collections ==========

/// Extension trait to allow .Linq() chaining
pub trait LinqExtensions: IntoIterator + Sized {
    fn Select<U, F>(self, selector: F) -> List<U>
    where
        F: Fn(Self::Item) -> U,
    {
        Linq::Select(self, selector)
    }

    fn Where<F>(self, predicate: F) -> List<Self::Item>
    where
        F: Fn(&Self::Item) -> bool,
    {
        Linq::Where(self, predicate)
    }

    fn OrderBy<K, F>(self, key_selector: F) -> List<Self::Item>
    where
        F: Fn(&Self::Item) -> K,
        K: Ord,
    {
        Linq::OrderBy(self, key_selector)
    }

    fn Take(self, count: usize) -> List<Self::Item> {
        Linq::Take(self, count)
    }

    fn Skip(self, count: usize) -> List<Self::Item> {
        Linq::Skip(self, count)
    }
}

// Implement for Vec
impl<T> LinqExtensions for Vec<T> {}

// Implement for List
impl<T> LinqExtensions for List<T> {}