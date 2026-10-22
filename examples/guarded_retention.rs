//! Native retained-reader progress probe, not a throughput comparison.
#![forbid(unsafe_code)]
#[cfg(feature = "comparisons")]
mod probe {
    use std::{sync::mpsc, thread, time::Duration};
    #[derive(Debug)]
    enum Event {
        Attempt(u64),
        Published(u64),
    }
    fn writer(mut publish: impl FnMut(u64), events: mpsc::Sender<Event>, stop: mpsc::Receiver<()>) {
        for revision in 1..=3 {
            events.send(Event::Attempt(revision)).unwrap();
            publish(revision);
            events.send(Event::Published(revision)).unwrap();
        }
        stop.recv().unwrap();
    }
    fn held(name: &str, events: &mpsc::Receiver<Event>) -> usize {
        let mut trace = Vec::new();
        loop {
            match events.recv_timeout(Duration::from_millis(250)) {
                Ok(event) => {
                    let done = matches!(event, Event::Published(3));
                    trace.push(event);
                    assert!(
                        !done,
                        "{name}: no native progress limit observed; revisit admission"
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(error) => panic!("{name}: writer disconnected: {error}"),
            }
        }
        assert!(
            matches!(trace.last(), Some(Event::Attempt(1..=3))),
            "{name}: writer never reached a native publication attempt: {trace:?}"
        );
        let published = trace
            .iter()
            .filter(|e| matches!(e, Event::Published(_)))
            .count();
        eprintln!("{name}: held-reader events {trace:?}");
        published
    }
    fn released(name: &str, events: &mpsc::Receiver<Event>) {
        loop {
            let event = events.recv_timeout(Duration::from_secs(2)).unwrap();
            eprintln!("{name}: released-reader event {event:?}");
            if matches!(event, Event::Published(3)) {
                break;
            }
        }
    }
    fn report(name: &str, before: usize) {
        println!("{{\"engine\":\"{name}\",\"published_before_release\":{before},\"published_after_release\":3,\"old_value_preserved\":true,\"native_guard_progress_limit_observed\":true}}");
    }
    fn flashmap() {
        let (mut write, read) = flashmap::new::<u64, u64>();
        write.guard().insert(1, 0);
        let old = read.guard();
        assert_eq!(old.get(&1), Some(&0));
        let (tx, rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            writer(
                |r| {
                    write.guard().insert(1, r);
                },
                tx,
                stop_rx,
            )
        });
        let before = held("flashmap", &rx);
        assert_eq!(old.get(&1), Some(&0));
        drop(old);
        released("flashmap", &rx);
        {
            let current = read.guard();
            assert_eq!(current.get(&1), Some(&3));
        }
        stop_tx.send(()).unwrap();
        worker.join().unwrap();
        report("flashmap", before);
    }
    fn evmap() {
        let (mut write, read) = evmap::new::<u64, u64>();
        write.update(1, 0).publish();
        let old = read.enter().unwrap();
        assert_eq!(old.get_one(&1), Some(&0));
        let (tx, rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            writer(
                |r| {
                    write.update(1, r).publish();
                },
                tx,
                stop_rx,
            )
        });
        let before = held("evmap", &rx);
        assert_eq!(old.get_one(&1), Some(&0));
        drop(old);
        released("evmap", &rx);
        {
            let current = read.enter().unwrap();
            assert_eq!(current.get_one(&1), Some(&3));
        }
        stop_tx.send(()).unwrap();
        worker.join().unwrap();
        report("evmap", before);
    }
    #[derive(Default, Clone)]
    struct Number(u64);
    impl left_right::Absorb<u64> for Number {
        fn absorb_first(&mut self, operation: &mut u64, _: &Self) {
            self.0 = *operation;
        }
        fn sync_with(&mut self, other: &Self) {
            self.0 = other.0;
        }
    }
    fn left_right() {
        let (mut write, read) = left_right::new::<Number, u64>();
        write.append(0).publish();
        let old = read.enter().unwrap();
        assert_eq!(old.0, 0);
        let (tx, rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            writer(
                |r| {
                    write.append(r).publish();
                },
                tx,
                stop_rx,
            )
        });
        let before = held("left-right", &rx);
        assert_eq!(old.0, 0);
        drop(old);
        released("left-right", &rx);
        {
            let current = read.enter().unwrap();
            assert_eq!(current.0, 3);
        }
        stop_tx.send(()).unwrap();
        worker.join().unwrap();
        report("left-right", before);
    }
    pub fn run() {
        thread::spawn(|| {
            thread::sleep(Duration::from_secs(5));
            eprintln!("native guard probe deadline reached");
            std::process::exit(124);
        });
        flashmap();
        evmap();
        left_right();
    }
}
fn main() {
    #[cfg(feature = "comparisons")]
    probe::run();
    #[cfg(not(feature = "comparisons"))]
    eprintln!("Enable --features comparisons for the native progress probe.");
}
