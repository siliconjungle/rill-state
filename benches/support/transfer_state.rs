//! Matched fixed copying/transfer policies; original staging, traces and proofs.
use super::*;
use rill_state::experimental::{AdaptiveMap, FlatMode, InlineFlat, ReuseSharedFlat, SharedFlat};
fn policy<
    S: BuildHasher + Clone + Default + Send + Sync,
    F: FlatMode<Key, Value, S> + Send + Sync,
>(
    o: &Options,
    p: &[&str],
) where
    F::Data: Send + Sync,
{
    assert_eq!(p.len(), 7);
    assert_eq!(&p[2..5], &["1", "3", "drop"]);
    macro_rules! call {
        ($page:literal, $reset:literal) => {
            run(
                Compared::<
                    AdaptiveMap<Key, Value, S, 1, 3, true, $page, F, $reset>,
                    NativeRoot<AdaptiveMap<Key, Value, S, 1, 3, true, $page, F, $reset>>,
                >::new(),
                o,
            )
        };
    }
    match (p[5], p[6]) {
        ("16", "oneway") => call!(16, false),
        ("16", "reset") => call!(16, true),
        ("64", "oneway") => call!(64, false),
        ("64", "reset") => call!(64, true),
        _ => panic!("unregistered transfer policy"),
    }
}
fn dispatch(o: &Options) {
    let Some(tail) = o.engine.strip_prefix("adaptive-") else {
        super::adaptive_dispatch::reference(o);
        return;
    };
    let p: Vec<_> = tail.split('-').collect();
    assert_eq!(p.len(), 7);
    match (p[0], p[1]) {
        ("inline", "sip") => policy::<std::collections::hash_map::RandomState, InlineFlat>(o, &p),
        ("inline", "fast") => policy::<hashbrown::DefaultHashBuilder, InlineFlat>(o, &p),
        ("shared", "sip") => {
            policy::<std::collections::hash_map::RandomState, SharedFlat<0>>(o, &p)
        }
        ("shared", "fast") => policy::<hashbrown::DefaultHashBuilder, SharedFlat<0>>(o, &p),
        ("cache", "sip") => policy::<std::collections::hash_map::RandomState, SharedFlat<2>>(o, &p),
        ("cache", "fast") => policy::<hashbrown::DefaultHashBuilder, SharedFlat<2>>(o, &p),
        ("reuse", "sip") => {
            policy::<std::collections::hash_map::RandomState, ReuseSharedFlat<0>>(o, &p)
        }
        ("reuse", "fast") => policy::<hashbrown::DefaultHashBuilder, ReuseSharedFlat<0>>(o, &p),
        ("reusecache", "sip") => {
            policy::<std::collections::hash_map::RandomState, ReuseSharedFlat<2>>(o, &p)
        }
        ("reusecache", "fast") => {
            policy::<hashbrown::DefaultHashBuilder, ReuseSharedFlat<2>>(o, &p)
        }
        _ => panic!("unregistered transfer mode/builder"),
    }
}
pub(super) fn entry() {
    let mut o = options();
    if !std::env::args().any(|a| a == "--engine") {
        o.engine = "adaptive-reuse-fast-1-3-drop-16-reset".into();
    }
    dispatch(&o);
}
