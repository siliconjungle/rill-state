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
    let map = scc::HashMap::<u64, [u64; 4]>::new();
    for i in 0u64..512 {
        map.entry_sync(i.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17))
            .and_modify(|v| *v = [i; 4])
            .or_insert([i; 4]);
    }
    eprintln!("inserted {}", map.len());
    let mut old = Vec::new();
    for r in 0..16 {
        eprintln!("revision {r}: clone start");
        let next = map.clone();
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
        old.push(next);
        if old.len() > 5 {
            old.remove(0);
        }
        eprintln!("revision {r}: checked");
    }
    assert_eq!(map.read_sync(&0, |_, v| *v), Some([0; 4]));
    done.store(true, Ordering::Release);
    println!("native SCC probe passed");
}
