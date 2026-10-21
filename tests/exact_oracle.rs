#[path = "../benches/support/exact.rs"]
mod exact;
use exact::{assert_gets, ExactRows};
use std::collections::BTreeMap;

#[test]
fn unordered_rows_and_nonclone_payloads_are_checked_without_copying() {
    #[derive(Debug, Eq, PartialEq)]
    struct NonClone(u64);
    let expected = BTreeMap::from([(1, NonClone(10)), (2, NonClone(20))]);
    let mut check = ExactRows::new(&expected, 2);
    check.observe(&2, &NonClone(20));
    check.observe(&1, &NonClone(10));
    check.finish();
    assert_gets(&expected, |key| expected.get(key));
}
#[test]
fn duplicate_rows_cannot_hide_an_omitted_key() {
    let expected = BTreeMap::from([(1, 10), (2, 20)]);
    assert!(std::panic::catch_unwind(|| {
        let mut check = ExactRows::new(&expected, 2);
        check.observe(&1, &10);
        check.observe(&1, &10);
        check.finish();
    })
    .is_err());
}
#[test]
fn missing_rows_and_unknown_keys_are_rejected() {
    let expected = BTreeMap::from([(1, 10), (2, 20)]);
    assert!(std::panic::catch_unwind(|| {
        let mut check = ExactRows::new(&expected, 2);
        check.observe(&1, &10);
        check.finish();
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| {
        let mut check = ExactRows::new(&expected, 2);
        check.observe(&3, &30);
    })
    .is_err());
}
#[test]
fn incorrect_reported_cardinality_is_rejected() {
    let expected = BTreeMap::from([(1, 10)]);
    assert!(std::panic::catch_unwind(|| ExactRows::new(&expected, 2)).is_err());
}
#[test]
fn middle_payload_corruption_is_checked_beyond_touched_prefix_and_suffix() {
    let expected = BTreeMap::from([(1, vec![0u8; 1024])]);
    let mut corrupt = vec![0u8; 1024];
    corrupt[512] = 1;
    assert!(std::panic::catch_unwind(|| {
        let mut check = ExactRows::new(&expected, 1);
        check.observe(&1, &corrupt);
    })
    .is_err());
}
#[test]
fn point_lookup_faults_are_rejected_independently_of_iteration() {
    let expected = BTreeMap::from([(1, 10)]);
    assert!(std::panic::catch_unwind(|| assert_gets(&expected, |_| None)).is_err());
    let wrong = 11;
    assert!(std::panic::catch_unwind(|| assert_gets(&expected, |_| Some(&wrong))).is_err());
}
