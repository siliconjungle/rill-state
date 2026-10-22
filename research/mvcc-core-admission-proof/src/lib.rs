//! Required native capability proofs, separate from complete-engine admission.
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use mvcc::{Config, Database, Mvcc, Snapshot, Transaction};
    use std::collections::BTreeMap;
    use std::sync::{Barrier, mpsc};
    use std::time::Duration;

    #[derive(Clone, Debug, Mvcc, PartialEq, Eq)]
    #[mvcc(table = "admission")]
    struct Row {
        #[mvcc(primary_key)]
        key: u64,
        value: [u64; 4],
    }
    fn value(k: u64, revision: u64) -> [u64; 4] {
        [k, revision, !k, k.wrapping_add(revision)]
    }
    fn open(records: u64) -> Database {
        let db = Database::open(Config::in_memory()).unwrap();
        db.register::<Row>().unwrap();
        let mut tx = db.begin();
        for key in 0..records {
            tx.insert(Row {
                key,
                value: value(key, 0),
            })
            .unwrap();
        }
        tx.commit().unwrap();
        db
    }
    fn contents(tx: &mut Transaction<'_, Snapshot>) -> BTreeMap<u64, [u64; 4]> {
        let rows = tx.scan::<Row>().unwrap();
        let mut result = BTreeMap::new();
        for row in rows {
            assert!(result.insert(row.key, row.value).is_none());
        }
        result
    }
    fn assert_initial(tx: &mut Transaction<'_, Snapshot>, records: u64) {
        let got = contents(tx);
        assert_eq!(got.len(), records as usize);
        for key in 0..records {
            assert_eq!(got[&key], value(key, 0));
            assert_eq!(tx.get::<Row>(&key).unwrap().unwrap().value, value(key, 0));
        }
        assert!(tx.get::<Row>(&u64::MAX).unwrap().is_none());
    }
    fn put(tx: &mut Transaction<'_, Snapshot>, key: u64, revision: u64) {
        if !tx
            .update::<Row>(&key, |row| row.value = value(key, revision))
            .unwrap()
        {
            tx.insert(Row {
                key,
                value: value(key, revision),
            })
            .unwrap();
        }
    }

    #[test]
    fn retained_native_transactions_match_full_model_at_20k() {
        let db = open(20_000);
        let mut initial = db.begin();
        let mut model: BTreeMap<_, _> = (0..20_000).map(|k| (k, value(k, 0))).collect();
        for revision in 1..=32 {
            let mut previous = db.begin();
            let expected_old = model.clone();
            let mut tx = db.begin();
            for n in 0..200 {
                let key = (revision * 193 + n) % 20_000;
                put(&mut tx, key, revision);
                model.insert(key, value(key, revision));
                if n % 7 == 0 {
                    assert!(tx.delete::<Row>(&key).unwrap());
                    model.remove(&key);
                    // A later put in the same transaction must be visible.
                    put(&mut tx, key, revision + 1000);
                    model.insert(key, value(key, revision + 1000));
                }
                if n % 11 == 0 {
                    assert!(!tx.delete::<Row>(&u64::MAX).unwrap());
                }
            }
            tx.commit().unwrap();
            assert_eq!(contents(&mut previous), expected_old);
            assert_eq!(contents(&mut db.begin()), model);
            assert_initial(&mut initial, 20_000);
        }
        drop(initial);
        assert_eq!(db.stats().active_transactions, 0);
    }

    #[test]
    fn native_transactions_created_on_four_reader_threads_hold_old_values() {
        let db = open(20_000);
        let start = Barrier::new(5);
        std::thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let mut done = Vec::new();
            let mut handles = Vec::new();
            for _ in 0..4 {
                let (done_tx, done_rx) = mpsc::channel();
                done.push(done_tx);
                let db = &db;
                let start = &start;
                let ready_tx = ready_tx.clone();
                handles.push(scope.spawn(move || {
                    // The transaction and its epoch guard stay on this thread.
                    let mut tx = db.begin();
                    ready_tx.send(tx.snapshot()).unwrap();
                    start.wait();
                    for _ in 0..8 {
                        assert_initial(&mut tx, 20_000);
                    }
                    done_rx.recv_timeout(Duration::from_secs(60)).unwrap();
                    assert_initial(&mut tx, 20_000);
                }));
            }
            let mut snapshots = Vec::new();
            for _ in 0..4 {
                snapshots.push(ready_rx.recv_timeout(Duration::from_secs(60)).unwrap());
            }
            assert!(snapshots.iter().all(|s| *s == snapshots[0]));
            start.wait();
            for revision in 1..=16 {
                let mut tx = db.begin();
                for n in 0..200 {
                    put(&mut tx, (revision * 193 + n) % 20_000, revision);
                }
                tx.commit().unwrap();
            }
            for sender in done {
                sender.send(()).unwrap();
            }
            for handle in handles {
                handle.join().unwrap();
            }
        });
        assert_eq!(db.stats().active_transactions, 0);
        assert_eq!(
            db.begin().get::<Row>(&193).unwrap().unwrap().value,
            value(193, 1)
        );
    }

    #[test]
    fn dropped_partial_transaction_and_empty_commit_preserve_native_snapshot() {
        let db = open(1000);
        let mut old = db.begin();
        {
            let mut tx = db.begin();
            put(&mut tx, 17, 1);
            assert!(tx.delete::<Row>(&18).unwrap());
            put(&mut tx, 1001, 1);
            // Drop rolls back the complete partial transaction.
        }
        assert_initial(&mut db.begin(), 1000);
        db.begin().commit().unwrap();
        assert_initial(&mut old, 1000);
        assert_initial(&mut db.begin(), 1000);
        drop(old);
        assert_eq!(db.stats().active_transactions, 0);
    }
}
