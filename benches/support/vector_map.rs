//! Actual PVec primitives behind an explicit key-index/versioning adapter.
//! The index duplicates canonical keys and copies on structural changes. Indexed
//! borrowed traversal is used because upstream exposes no borrowed row iterator.
use rill_state::Mutation;
use std::{
    fmt::Debug,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    sync::Arc,
};

pub trait Rows<T>: Clone + Send + Sync {
    fn get(&self, index: usize) -> Option<&T>;
    fn get_mut(&mut self, index: usize) -> Option<&mut T>;
    fn push(&mut self, value: T);
    fn pop(&mut self) -> Option<T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
macro_rules! rows {
    ($ty:ty) => {
        impl<T: Clone + Debug + Send + Sync> Rows<T> for $ty {
            fn get(&self, i: usize) -> Option<&T> {
                self.get(i)
            }
            fn get_mut(&mut self, i: usize) -> Option<&mut T> {
                self.get_mut(i)
            }
            fn push(&mut self, v: T) {
                self.push(v)
            }
            fn pop(&mut self) -> Option<T> {
                self.pop()
            }
            fn len(&self) -> usize {
                self.len()
            }
        }
    };
}
rows!(pvec::PVec<T>);
rows!(pvec::core::RrbVec<T>);
rows!(pvec::core::RbVec<T>);
impl<T: Clone + Debug + Send + Sync> Rows<T> for Arc<Vec<T>> {
    fn get(&self, i: usize) -> Option<&T> {
        self.as_slice().get(i)
    }
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        Arc::make_mut(self).get_mut(i)
    }
    fn push(&mut self, v: T) {
        Arc::make_mut(self).push(v)
    }
    fn pop(&mut self) -> Option<T> {
        Arc::make_mut(self).pop()
    }
    fn len(&self) -> usize {
        self.as_ref().len()
    }
}
pub trait VectorKind: Clone + Send + Sync {
    type Vector<T: Clone + Debug + Send + Sync>: Rows<T>;
    fn create<T: Clone + Debug + Send + Sync>() -> Self::Vector<T>;
}
macro_rules! kind {
    ($name:ident,$ty:ty,$new:expr) => {
        #[derive(Clone)]
        pub struct $name;
        impl VectorKind for $name {
            type Vector<T: Clone + Debug + Send + Sync> = $ty;
            fn create<T: Clone + Debug + Send + Sync>() -> Self::Vector<T> {
                $new
            }
        }
    };
}
kind!(Flat, pvec::PVec<T>, pvec::PVec::new());
kind!(Tree, pvec::PVec<T>, pvec::PVec::new_with_tree());
kind!(Rrb, pvec::core::RrbVec<T>, pvec::core::RrbVec::new());
kind!(Rb, pvec::core::RbVec<T>, pvec::core::RbVec::new());
kind!(CowVec, Arc<Vec<T>>, Arc::new(Vec::new()));

pub struct VectorMap<K, V, S, Kind: VectorKind>
where
    K: Clone + Debug + Send + Sync,
    V: Clone + Debug + Send + Sync,
{
    index: Arc<hashbrown::HashMap<K, usize, S>>,
    rows: Kind::Vector<(K, V)>,
    marker: PhantomData<fn() -> Kind>,
}
impl<K, V, S, Kind: VectorKind> Clone for VectorMap<K, V, S, Kind>
where
    K: Clone + Debug + Send + Sync,
    V: Clone + Debug + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            index: self.index.clone(),
            rows: self.rows.clone(),
            marker: PhantomData,
        }
    }
}
impl<K, V, S, Kind: VectorKind> VectorMap<K, V, S, Kind>
where
    K: Clone + Hash + Eq + Debug + Send + Sync,
    V: Clone + Debug + Send + Sync,
    S: BuildHasher + Clone + Default,
{
    pub fn new() -> Self {
        let map = Self {
            index: Arc::new(hashbrown::HashMap::with_hasher(S::default())),
            rows: Kind::create(),
            marker: PhantomData,
        };
        debug_assert!(map.is_empty());
        map
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.index
            .get(key)
            .and_then(|&i| self.rows.get(i))
            .map(|(_, v)| v)
    }
    pub fn put(&mut self, key: K, value: V) {
        if let Some(&i) = self.index.get(&key) {
            self.rows.get_mut(i).unwrap().1 = value;
        } else {
            let i = self.rows.len();
            self.rows.push((key.clone(), value));
            Arc::make_mut(&mut self.index).insert(key, i);
        }
    }
    pub fn del(&mut self, key: &K) {
        let Some(&i) = self.index.get(key) else {
            return;
        };
        let last = self.rows.len() - 1;
        // Repair the key index using borrowed canonical keys before moving rows.
        // User failures can leave this private staging object partially applied;
        // the shared engine publishes it only after the complete batch succeeds.
        let index = Arc::make_mut(&mut self.index);
        if i != last {
            *index.get_mut(&self.rows.get(last).unwrap().0).unwrap() = i;
        }
        index.remove(key);
        let last = self.rows.pop().unwrap();
        if i != self.rows.len() {
            let removed = std::mem::replace(self.rows.get_mut(i).unwrap(), last);
            drop(removed);
        }
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn visit(&self, mut f: impl FnMut(&K, &V)) {
        for i in 0..self.rows.len() {
            let (k, v) = self.rows.get(i).unwrap();
            f(k, v);
        }
    }
    pub fn apply(&mut self, ops: &[Mutation<K, V>]) {
        for op in ops {
            match op {
                Mutation::Put(k, v) => {
                    if let Some(&i) = self.index.get(k) {
                        self.rows.get_mut(i).unwrap().1 = v.clone();
                    } else {
                        self.put(k.clone(), v.clone());
                    }
                }
                Mutation::Delete(k) => self.del(k),
            }
        }
    }
}

impl<K, V, S, Kind: VectorKind> Default for VectorMap<K, V, S, Kind>
where
    K: Clone + Hash + Eq + Debug + Send + Sync,
    V: Clone + Debug + Send + Sync,
    S: BuildHasher + Clone + Default,
{
    fn default() -> Self {
        Self::new()
    }
}
