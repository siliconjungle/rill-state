//! Independently measure snapshot publication. No state mutations, journal or durability
//! are attributed to these reference-publication primitives.
use std::hint::black_box;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

#[derive(Debug)]
struct Version {
    revision: u64,
    mirror: u64,
    payload: [u64; 8],
}
fn data(revision: u64) -> Arc<Version> {
    Arc::new(Version {
        revision,
        mirror: !revision,
        payload: [revision; 8],
    })
}
fn check(v: &Version) -> u64 {
    assert_eq!(v.revision, !v.mirror);
    assert!(v.payload.iter().all(|x| *x == v.revision));
    v.revision
}
trait Publication: Send + Sync + 'static {
    fn new() -> Self;
    fn publish(&self, version: Arc<Version>);
    fn read(&self) -> Arc<Version>;
}
impl Publication for RwLock<Arc<Version>> {
    fn new() -> Self {
        Self::new(data(0))
    }
    fn publish(&self, v: Arc<Version>) {
        *self.write().unwrap() = v;
    }
    fn read(&self) -> Arc<Version> {
        RwLock::read(self).unwrap().clone()
    }
}
impl Publication for Mutex<Arc<Version>> {
    fn new() -> Self {
        Self::new(data(0))
    }
    fn publish(&self, v: Arc<Version>) {
        *self.lock().unwrap() = v;
    }
    fn read(&self) -> Arc<Version> {
        self.lock().unwrap().clone()
    }
}
impl Publication for arc_swap::ArcSwap<Version> {
    fn new() -> Self {
        Self::new(data(0))
    }
    fn publish(&self, v: Arc<Version>) {
        self.store(v);
    }
    fn read(&self) -> Arc<Version> {
        self.load_full()
    }
}
impl Publication for swap_arc::SwapArc<Version> {
    fn new() -> Self {
        Self::new(data(0))
    }
    fn publish(&self, v: Arc<Version>) {
        self.store(v);
    }
    fn read(&self) -> Arc<Version> {
        self.load_full()
    }
}
impl Publication for rcu_cell::RcuCell<Version> {
    fn new() -> Self {
        Self::new(Version {
            revision: 0,
            mirror: !0,
            payload: [0; 8],
        })
    }
    fn publish(&self, v: Arc<Version>) {
        self.write(v);
    }
    fn read(&self) -> Arc<Version> {
        rcu_cell::RcuCell::read(self).unwrap()
    }
}
fn run<P: Publication>(name: &str, threads: usize, rounds: usize, readers: usize, proof: bool) {
    let publication = Arc::new(P::new());
    let old = publication.read();
    let barrier = Arc::new(std::sync::Barrier::new(threads + 1));
    let start = Instant::now();
    let counts = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..threads {
            let state = publication.clone();
            let barrier = barrier.clone();
            handles.push(scope.spawn(move || {
                barrier.wait();
                let mut last = 0;
                let mut sum = 0u64;
                for _ in 0..rounds {
                    for _ in 0..readers.div_ceil(threads) {
                        let v = state.read();
                        if proof {
                            let r = check(&v);
                            assert!(r >= last);
                            last = r;
                        }
                        sum = sum.wrapping_add(black_box(v).revision);
                    }
                }
                sum
            }));
        }
        barrier.wait();
        for r in 1..=rounds {
            publication.publish(data(r as u64));
            if proof {
                check(&old);
            }
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .fold(0u64, |a, b| a.wrapping_add(b))
    });
    let elapsed = start.elapsed().as_nanos();
    assert_eq!(check(&publication.read()), rounds as u64);
    assert_eq!(check(&old), 0);
    println!("{{\"layer\":\"publication\",\"engine\":\"{name}\",\"threads\":{threads},\"rounds\":{rounds},\"logical_readers\":{readers},\"actual_reads\":{},\"wall_ns\":{elapsed},\"proof\":{proof},\"checksum\":{counts}}}",threads*readers.div_ceil(threads)*rounds);
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let arg = |key: &str, default: &str| {
        args.windows(2)
            .find(|a| a[0] == key)
            .map(|a| a[1].clone())
            .unwrap_or_else(|| default.into())
    };
    let engine = arg("--engine", "arc-swap");
    let threads = arg("--threads", "4").parse().unwrap();
    let rounds = arg("--rounds", "100").parse().unwrap();
    let readers = arg("--readers", "20000").parse().unwrap();
    let proof = args.iter().any(|a| a == "--verify");
    assert!(threads > 0 && readers > 0 && rounds > 0);
    match engine.as_str() {
        "rwlock" => run::<RwLock<Arc<Version>>>(&engine, threads, rounds, readers, proof),
        "mutex" => run::<Mutex<Arc<Version>>>(&engine, threads, rounds, readers, proof),
        "arc-swap" => run::<arc_swap::ArcSwap<Version>>(&engine, threads, rounds, readers, proof),
        "swap-arc" => run::<swap_arc::SwapArc<Version>>(&engine, threads, rounds, readers, proof),
        "rcu-cell" => run::<rcu_cell::RcuCell<Version>>(&engine, threads, rounds, readers, proof),
        _ => panic!("unknown engine"),
    }
}
