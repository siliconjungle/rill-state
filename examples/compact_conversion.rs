//! Single-growth diagnostic. Setup and complete retained-root checks are untimed.
//! These are raw map operations, not complete engine throughput or service latency.
#![forbid(unsafe_code)]
#[cfg(feature = "comparisons")]
mod diagnostic {
    use rill_state::experimental::{CompactIndexMap, OwnedHasher};
    use std::{fmt::Debug, hash::Hash, hint::black_box, time::Instant};

    trait Key: Clone + Hash + Eq + Debug {
        fn make(id: u64) -> Self;
    }
    impl Key for u64 {
        fn make(id: u64) -> Self {
            id.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
        }
    }
    impl Key for String {
        fn make(id: u64) -> Self {
            format!("key-{:016x}-{:016x}-{:016x}", id, id.rotate_left(13), !id)
        }
    }
    trait Value: Clone + Eq + Debug {
        fn make(id: u64) -> Self;
    }
    impl Value for [u64; 4] {
        fn make(id: u64) -> Self {
            [id, !id, id.rotate_left(13), id.wrapping_mul(317)]
        }
    }
    impl Value for Vec<u8> {
        fn make(id: u64) -> Self {
            let mut value = vec![id as u8; 256];
            value[..8].copy_from_slice(&id.to_le_bytes());
            value[248..].copy_from_slice(&(!id).to_le_bytes());
            value
        }
    }
    struct Options {
        shape: String,
        load: usize,
        adaptive: bool,
        pin: bool,
        capacity: usize,
    }
    fn run<K: Key, V: Value, const L: usize, const A: bool>(o: Options) {
        assert_eq!(o.load, L);
        assert_eq!(o.adaptive, A);
        assert!([32768, 65536, 131072].contains(&o.capacity));
        type Builder = hashbrown::DefaultHashBuilder;
        let mut map = CompactIndexMap::<K, V, Builder, L, OwnedHasher, true, A>::default();
        let count = o.capacity / 100 * L + o.capacity % 100 * L / 100;
        for i in 0..count {
            assert!(map.insert(K::make(i as u64), V::make(i as u64)).is_none());
        }
        assert_eq!(map.index_capacity(), o.capacity);
        assert_eq!(map.len(), count);
        let width_before = map.index_word_bytes();
        let retained = o.pin.then(|| map.clone());
        // Allocate incoming key/value before the measured raw insertion.
        let key = black_box(K::make(count as u64));
        let value = black_box(V::make(count as u64));
        let start = Instant::now();
        let previous = map.insert(key, value);
        let insertion_ns = start.elapsed().as_nanos();
        assert!(black_box(previous).is_none());
        assert_eq!(map.index_capacity(), o.capacity * 2);
        assert_eq!(map.len(), count + 1);
        let width_after = map.index_word_bytes();
        assert_eq!(width_before, if A && o.capacity <= 65536 { 4 } else { 8 });
        assert_eq!(
            width_after,
            if A && o.capacity * 2 <= 65536 { 4 } else { 8 }
        );
        // Check complete values/unique iteration and old roots after timing.
        let mut seen = std::collections::HashSet::new();
        for (key, value) in map.iter() {
            assert!(seen.insert(key));
            assert_eq!(map.get(key), Some(value));
        }
        assert_eq!(seen.len(), count + 1);
        for i in 0..=count {
            assert_eq!(map.get(&K::make(i as u64)), Some(&V::make(i as u64)));
        }
        if let Some(old) = retained {
            assert_eq!(old.len(), count);
            assert_eq!(old.index_capacity(), o.capacity);
            assert_eq!(old.index_word_bytes(), width_before);
            for i in 0..count {
                assert_eq!(old.get(&K::make(i as u64)), Some(&V::make(i as u64)));
            }
            assert!(old.get(&K::make(count as u64)).is_none());
        }
        println!("{{\"kind\":\"compact-single-growth\",\"shape\":\"{}\",\"load\":{},\"adaptive\":{},\"pin_initial\":{},\"records_before\":{},\"capacity_before\":{},\"capacity_after\":{},\"word_bytes_before\":{},\"word_bytes_after\":{},\"insertion_ns\":{},\"verified\":true}}", o.shape,L,A,o.pin,count,o.capacity,o.capacity*2,width_before,width_after,insertion_ns);
    }
    fn policy<K: Key, V: Value>(o: Options) {
        macro_rules! call {
            ($l:literal) => {
                if o.adaptive {
                    run::<K, V, $l, true>(o)
                } else {
                    run::<K, V, $l, false>(o)
                }
            };
        }
        match o.load {
            25 => call!(25),
            50 => call!(50),
            70 => call!(70),
            85 => call!(85),
            _ => panic!("unsupported load"),
        }
    }
    pub fn main() {
        let args: Vec<_> = std::env::args().collect();
        let get = |name: &str, default: &str| {
            args.windows(2)
                .find(|p| p[0] == name)
                .map(|p| p[1].clone())
                .unwrap_or_else(|| default.into())
        };
        let o = Options {
            shape: get("--shape", "u64-inline"),
            load: get("--load", "70").parse().unwrap(),
            adaptive: get("--adaptive", "true").parse().unwrap(),
            pin: get("--pin-initial", "true").parse().unwrap(),
            capacity: get("--capacity", "65536").parse().unwrap(),
        };
        match o.shape.as_str() {
            "u64-inline" => policy::<u64, [u64; 4]>(o),
            "string-inline" => policy::<String, [u64; 4]>(o),
            "u64-owned256" => policy::<u64, Vec<u8>>(o),
            "string-owned256" => policy::<String, Vec<u8>>(o),
            _ => panic!("unsupported shape"),
        }
    }
}
#[cfg(feature = "comparisons")]
fn main() {
    diagnostic::main();
}
#[cfg(not(feature = "comparisons"))]
fn main() {
    eprintln!("enable --features comparisons");
    std::process::exit(2);
}
