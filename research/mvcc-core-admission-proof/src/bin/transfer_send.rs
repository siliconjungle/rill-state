#![forbid(unsafe_code)]
fn require_send<T: Send>() {}
fn main() {
    require_send::<mvcc::Transaction<'static, mvcc::Snapshot>>();
}
