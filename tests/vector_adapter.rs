#![cfg(feature = "comparisons")]
#[path = "../benches/support/vector_map.rs"]
mod vector_map;
use rill_state::Mutation;
use std::{
    collections::BTreeMap,
    hash::{BuildHasher, Hash, Hasher},
};
use vector_map::{VectorKind, VectorMap};

#[derive(Clone, Debug)]
struct Key {
    id: u64,
    metadata: u64,
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Key {}
impl Hash for Key {
    fn hash<H: Hasher>(&self, h: &mut H) {
        0u64.hash(h)
    }
}
fn check<S: BuildHasher + Clone + Default, Kind: VectorKind>(
    map: &VectorMap<Key, Vec<u8>, S, Kind>,
    model: &BTreeMap<u64, (u64, Vec<u8>)>,
) {
    assert_eq!(map.len(), model.len());
    let mut found = BTreeMap::new();
    map.visit(|k, v| {
        assert!(found.insert(k.id, (k.metadata, v.clone())).is_none());
    });
    assert_eq!(&found, model);
    for id in 0..128 {
        let expected = model.get(&id).map(|(_, v)| v);
        assert_eq!(
            map.get(&Key {
                id,
                metadata: u64::MAX
            }),
            expected
        );
    }
}
fn exercise<S: BuildHasher + Clone + Default, Kind: VectorKind>() {
    let mut map = VectorMap::<Key, Vec<u8>, S, Kind>::new();
    let mut model = BTreeMap::new();
    let mut old = Vec::new();
    let mut random = 789112u64;
    for step in 0..1000u64 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let id = random % 128;
        let key = Key { id, metadata: step };
        let mut staged = map.clone();
        if random & 7 < 3 {
            staged.apply(&[Mutation::Delete(key.clone()), Mutation::Delete(key)]);
            model.remove(&id);
        } else {
            let mut value = vec![step as u8; 257];
            value[..8].copy_from_slice(&step.to_le_bytes());
            staged.apply(&[Mutation::Put(key, value.clone())]);
            if let Some((_, prior)) = model.get_mut(&id) {
                *prior = value;
            } else {
                model.insert(id, (step, value));
            }
        }
        map = staged;
        if step % 53 == 0 {
            check(&map, &model);
            if old.len() < 4 {
                old.push((map.clone(), model.clone()));
            }
            for (view, expected) in &old {
                check(view, expected);
            }
        }
    }
    check(&map, &model);
    for (view, expected) in &old {
        check(view, expected);
    }
    // Remove every key, including repeated misses, checking all possible keys.
    for id in 0..128 {
        map.del(&Key {
            id,
            metadata: 10000,
        });
        model.remove(&id);
        check(&map, &model);
    }
    assert!(map.is_empty());
    map.put(
        Key {
            id: 1,
            metadata: 20000,
        },
        vec![7],
    );
    assert_eq!(map.get(&Key { id: 1, metadata: 0 }), Some(&vec![7]));
    for (view, expected) in &old {
        check(view, expected);
    }
}
#[test]
fn all_vector_backends_preserve_key_identity_swap_indices_and_retained_states() {
    macro_rules! run {
        ($kind:ty) => {
            exercise::<std::collections::hash_map::RandomState, $kind>();
            exercise::<hashbrown::DefaultHashBuilder, $kind>();
        };
    }
    run!(vector_map::Flat);
    run!(vector_map::Tree);
    run!(vector_map::Rrb);
    run!(vector_map::Rb);
    run!(vector_map::CowVec);
}
