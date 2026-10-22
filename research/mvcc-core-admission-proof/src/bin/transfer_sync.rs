#![forbid(unsafe_code)]
fn require_sync<T: Sync>() {}
fn main() {
    require_sync::<mvcc::Transaction<'static, mvcc::Snapshot>>();
}
