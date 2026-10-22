//! Complete borrowed-value oracle checks; no payload clones or hash-only checks.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

pub struct ExactRows<'a, K: Ord, V> {
    expected: &'a BTreeMap<K, V>,
    seen: BTreeSet<K>,
}
impl<'a, K: Clone + Ord + Debug, V: Eq + Debug> ExactRows<'a, K, V> {
    pub fn new(expected: &'a BTreeMap<K, V>, reported_len: usize) -> Self {
        assert_eq!(reported_len, expected.len(), "reported cardinality");
        Self {
            expected,
            seen: BTreeSet::new(),
        }
    }
    pub fn observe(&mut self, key: &K, value: &V) {
        assert_eq!(
            self.expected.get(key),
            Some(value),
            "complete value mismatch for {key:?}"
        );
        assert!(
            self.seen.insert(key.clone()),
            "duplicate iterated key {key:?}"
        );
    }
    pub fn finish(self) {
        assert_eq!(
            self.seen.len(),
            self.expected.len(),
            "missing iterated keys"
        );
    }
}

pub fn assert_gets<'a, K: Ord + Debug, V: Eq + Debug + 'a>(
    expected: &BTreeMap<K, V>,
    mut get: impl FnMut(&K) -> Option<&'a V>,
) {
    for (key, value) in expected {
        assert_eq!(get(key), Some(value), "point lookup mismatch for {key:?}");
    }
}
