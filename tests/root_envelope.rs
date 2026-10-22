#[path = "../benches/support/root.rs"]
mod root;
use root::{MapRoot, NativeRoot};
use std::sync::Arc;

#[derive(Clone)]
struct Backend {
    data: Arc<Vec<u64>>,
    metadata: u64,
}
fn retained<R: MapRoot<Backend>>() {
    let mut current = R::from_map(Backend {
        data: Arc::new((0..20_000).collect()),
        metadata: 0,
    });
    let old = current.clone();
    let mut staged = current.as_map().clone();
    Arc::make_mut(&mut staged.data)[17] = 9000;
    staged.metadata = 1;
    current = R::from_map(staged);
    assert_eq!(current.as_map().data[17], 9000);
    assert_eq!(current.as_map().metadata, 1);
    assert_eq!(old.as_map().data[17], 17);
    assert_eq!(old.as_map().metadata, 0);
    drop(current);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let view = &old;
            scope.spawn(move || {
                for i in 0..20_000 {
                    assert_eq!(view.as_map().data[i], i as u64);
                }
            });
        }
    });
}
#[test]
fn arc_and_native_roots_preserve_staged_data_metadata_and_lifetimes() {
    retained::<Arc<Backend>>();
    retained::<NativeRoot<Backend>>();
}
#[test]
fn native_clones_the_backend_while_arc_clones_only_its_envelope() {
    let native = NativeRoot::from_map(Backend {
        data: Arc::new(vec![7]),
        metadata: 0,
    });
    let native_copy = native.clone();
    assert_eq!(Arc::strong_count(&native.as_map().data), 2);
    drop(native_copy);
    assert_eq!(Arc::strong_count(&native.as_map().data), 1);
    let arc = Arc::from_map(Backend {
        data: Arc::new(vec![7]),
        metadata: 0,
    });
    let arc_copy = arc.clone();
    assert_eq!(Arc::strong_count(&arc), 2);
    assert_eq!(Arc::strong_count(&arc.as_map().data), 1);
    drop(arc_copy);
}
