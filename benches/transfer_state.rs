//! Fixed copying/transfer policies over the unchanged state harness.
#[allow(dead_code)]
#[path = "state.rs"]
mod harness;
fn main() {
    harness::run_transfer();
}
