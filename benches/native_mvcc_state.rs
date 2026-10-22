//! Actual native MVCC sessions and explicitly materialized transferable views.
#![forbid(unsafe_code)]
#[allow(dead_code)]
#[path = "state.rs"]
mod harness;
fn main() {
    harness::run_native_mvcc();
}
