//! Bounded native-API diagnostic, not a timing result.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn main() {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        if !flag.load(Ordering::Acquire) {
            eprintln!("diagnostic deadline reached");
            std::process::exit(124);
        }
    });
    let keep_all = std::env::args().any(|s| s == "--keep-all");
    let fresh_hasher = std::env::args().any(|s| s == "--fresh-hasher");
    let mut map = Arc::new(scc::HashMap::<u64, [u64; 4]>::new());
    for i in 0u64..512 {
        map.entry_sync(i.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17))
            .and_modify(|v| *v = [i; 4])
            .or_insert([i; 4]);
    }
    eprintln!("inserted {}", map.len());
    let first = map.clone();
    let mut old = Vec::new();
    for r in 0..128 {
        eprintln!("revision {r}: clone start");
        let next = if fresh_hasher {
            let mut rows = Vec::new();
            map.iter_sync(|k, v| {
                rows.push((*k, *v));
                true
            });
            let copy = scc::HashMap::with_capacity(map.capacity());
            for (k, v) in rows {
                copy.insert_sync(k, v).unwrap();
            }
            Arc::new(copy)
        } else {
            Arc::new((*map).clone())
        };
        eprintln!("revision {r}: cloned");
        for i in 0u64..512 {
            next.entry_sync(i.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17))
                .and_modify(|v| *v = [r; 4])
                .or_insert([r; 4]);
        }
        eprintln!("revision {r}: updated");
        let mut count = 0;
        next.iter_sync(|_, _| {
            count += 1;
            true
        });
        assert_eq!(count, 512);
        for i in 0u64..512 {
            assert_eq!(
                next.read_sync(
                    &i.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17),
                    |_, v| *v
                ),
                Some([r; 4])
            );
        }
        let contents: std::collections::BTreeMap<_, _> = {
            let mut rows = std::collections::BTreeMap::new();
            next.iter_sync(|k, v| {
                rows.insert(*k, *v);
                true
            });
            rows
        };
        assert_eq!(contents.len(), 512);
        old.push(map);
        map = next;
        if !keep_all && old.len() > 5 {
            old.pop();
        }
        for view in &old {
            let mut count = 0;
            view.iter_sync(|_, _| {
                count += 1;
                true
            });
            assert_eq!(count, 512);
        }
        eprintln!("revision {r}: checked");
    }
    assert_eq!(first.read_sync(&0, |_, v| *v), Some([0; 4]));
    assert_eq!(map.read_sync(&0, |_, v| *v), Some([127; 4]));
    done.store(true, Ordering::Release);
    println!("native SCC probe passed");
}
